//! Cemeteries, sections, rows, plots, plot types, restrictions, monuments,
//! crypt slots and computed plot status (FR-TEM, FR-SIR, FR-EML-01..03).

use crate::sql::{params, params_from_iter, OptionalExtension, Value};
use chrono::NaiveDate;

use super::*;
use crate::audit;
use crate::auth::{Permission, Session};
use crate::rules::{compute_status, resale_allowed_from, PlotStatus, StatusInputs};
use crate::settings;
use crate::util::{fmt_date, new_id, now_ts, parse_date, today};

const PLOT_SPACING: f64 = 5.0; // dm between plots
const ROW_SPACING: f64 = 8.0; // dm path between rows
const SECTION_PADDING: f64 = 6.0;

impl Store {
    // ---------------------------------------------------------------- cemeteries

    pub fn list_cemeteries(&self, include_voided: bool) -> Result<Vec<Cemetery>> {
        let mut stmt = self.conn.prepare("SELECT * FROM cemeteries WHERE (?1 OR voided_at IS NULL) ORDER BY name COLLATE \"hu-x-icu\"")?;
        let rows = stmt.query_map([include_voided], map_cemetery)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn get_cemetery(&self, id: &str) -> Result<Cemetery> {
        self.conn.query_row("SELECT * FROM cemeteries WHERE id=?1", [id], map_cemetery).optional()?.ok_or_else(|| CoreError::not_found("cemetery", id))
    }

    pub fn save_cemetery(&self, s: &Session, input: &Cemetery) -> Result<Cemetery> {
        s.require(Permission::ManageStructure)?;
        non_empty("name", &input.name)?;
        let kind = if input.kind == "denominational" { "denominational" } else { "public" };
        let scope = if input.folio_scope == "yearly" { "yearly" } else { "continuous" };
        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        let id = if input.id.is_empty() {
            let id = new_id();
            tx.execute(
                "INSERT INTO cemeteries (id, name, address, parcel_number, kind, owner, operator, opening_hours, folio_scope, notes, created_at, created_by, updated_at, updated_by)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?11,?12)",
                params![id, input.name.trim(), input.address, input.parcel_number, kind, input.owner, input.operator, input.opening_hours, scope, input.notes, ts, s.user_name],
            )?;
            let new = self.get_cemetery_tx(&tx, &id)?;
            audit::record(&tx, s, "cemetery", &id, "create", None::<&()>, Some(&new), None)?;
            id
        } else {
            let old = self.get_cemetery_tx(&tx, &input.id)?;
            if old.folio_scope != scope {
                let n: i64 = tx.query_row("SELECT count(*) FROM interment_events WHERE cemetery_id=?1", [&input.id], |r| r.get(0))?;
                if n > 0 {
                    return Err(CoreError::new("folio_scope_locked", "folio scope can only change while the cemetery has no events"));
                }
            }
            tx.execute(
                "UPDATE cemeteries SET name=?2, address=?3, parcel_number=?4, kind=?5, owner=?6, operator=?7, opening_hours=?8, folio_scope=?9, notes=?10, updated_at=?11, updated_by=?12 WHERE id=?1",
                params![input.id, input.name.trim(), input.address, input.parcel_number, kind, input.owner, input.operator, input.opening_hours, scope, input.notes, ts, s.user_name],
            )?;
            let new = self.get_cemetery_tx(&tx, &input.id)?;
            audit::record(&tx, s, "cemetery", &input.id, "update", Some(&old), Some(&new), None)?;
            input.id.clone()
        };
        tx.commit()?;
        self.get_cemetery(&id)
    }

    fn get_cemetery_tx(&self, tx: &crate::sql::Connection, id: &str) -> Result<Cemetery> {
        tx.query_row("SELECT * FROM cemeteries WHERE id=?1", [id], map_cemetery).optional()?.ok_or_else(|| CoreError::not_found("cemetery", id))
    }

    // ---------------------------------------------------------------- plot types

    pub fn list_plot_types(&self) -> Result<Vec<PlotType>> {
        let mut stmt = self.conn.prepare("SELECT * FROM plot_types WHERE voided_at IS NULL ORDER BY sort_order, name COLLATE \"hu-x-icu\"")?;
        let rows = stmt.query_map([], map_plot_type)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn get_plot_type(&self, id: &str) -> Result<PlotType> {
        self.conn
            .query_row("SELECT * FROM plot_types WHERE id=?1 OR code=?1", [id], map_plot_type)
            .optional()?
            .ok_or_else(|| CoreError::not_found("plot_type", id))
    }

    /// Plot types are extensible (FR-TEM-04) and parameterised (FR-BEA-01).
    pub fn save_plot_type(&self, s: &Session, t: &PlotType) -> Result<PlotType> {
        s.require(Permission::ManageSettings)?;
        non_empty("name", &t.name)?;
        if !["grave", "crypt", "urn", "scatter", "common"].contains(&t.category.as_str()) {
            return Err(CoreError::validation("category", "invalid category"));
        }
        if t.default_lease_years < 0 || t.rest_period_years < 0 || t.coffin_capacity < 0 || t.urn_capacity < 0 {
            return Err(CoreError::validation("default_lease_years", "values must not be negative"));
        }
        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        let id = if t.id.is_empty() {
            let id = new_id();
            let code = if t.code.trim().is_empty() { format!("custom_{}", &id[..8]) } else { t.code.trim().to_string() };
            tx.execute(
                "INSERT INTO plot_types (id, code, name, category, default_lease_years, rest_period_years, coffin_capacity, urn_capacity, width_cm, length_cm, builtin, sort_order, created_at, created_by, updated_at, updated_by)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0,?11,?12,?13,?12,?13)",
                params![id, code, t.name.trim(), t.category, t.default_lease_years, t.rest_period_years, t.coffin_capacity, t.urn_capacity, t.width_cm, t.length_cm, t.sort_order.max(1), ts, s.user_name],
            )?;
            audit::record(&tx, s, "plot_type", &id, "create", None::<&()>, Some(t), None)?;
            id
        } else {
            let old = tx.query_row("SELECT * FROM plot_types WHERE id=?1", [&t.id], map_plot_type)?;
            tx.execute(
                "UPDATE plot_types SET name=?2, category=?3, default_lease_years=?4, rest_period_years=?5, coffin_capacity=?6, urn_capacity=?7, width_cm=?8, length_cm=?9, sort_order=?10, updated_at=?11, updated_by=?12 WHERE id=?1",
                params![t.id, t.name.trim(), t.category, t.default_lease_years, t.rest_period_years, t.coffin_capacity, t.urn_capacity, t.width_cm, t.length_cm, t.sort_order, ts, s.user_name],
            )?;
            audit::record(&tx, s, "plot_type", &t.id, "update", Some(&old), Some(t), None)?;
            t.id.clone()
        };
        tx.commit()?;
        self.get_plot_type(&id)
    }

    // ---------------------------------------------------------------- sections & rows

    pub fn list_sections(&self, cemetery_id: &str) -> Result<Vec<Section>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.*, (SELECT count(*) FROM plot_rows r WHERE r.section_id=s.id AND r.voided_at IS NULL) AS row_count,
                    (SELECT count(*) FROM plots p WHERE p.section_id=s.id AND p.voided_at IS NULL) AS plot_count
             FROM sections s WHERE s.cemetery_id=?1 AND s.voided_at IS NULL ORDER BY s.code COLLATE \"hu-x-icu\"",
        )?;
        let rows = stmt.query_map([cemetery_id], |r| {
            Ok(Section {
                id: r.get("id")?,
                cemetery_id: r.get("cemetery_id")?,
                code: r.get("code")?,
                name: r.get("name")?,
                map_x: r.get("map_x")?,
                map_y: r.get("map_y")?,
                map_w: r.get("map_w")?,
                map_h: r.get("map_h")?,
                map_rot: r.get("map_rot")?,
                row_count: r.get("row_count")?,
                plot_count: r.get("plot_count")?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn list_rows(&self, section_id: &str) -> Result<Vec<PlotRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, section_id, number, direction FROM plot_rows WHERE section_id=?1 AND voided_at IS NULL
             ORDER BY CAST(number AS INTEGER), number COLLATE \"hu-x-icu\"",
        )?;
        let rows = stmt.query_map([section_id], |r| Ok(PlotRow { id: r.get(0)?, section_id: r.get(1)?, number: r.get(2)?, direction: r.get(3)? }))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// FR-TEM-03: "A parcella, 1–20. sor, soronként 30 hely, sírhely típus".
    /// Existing rows/plots with the same numbers are kept (idempotent).
    pub fn bulk_create(&self, s: &Session, input: &BulkCreateInput) -> Result<BulkCreateResult> {
        s.require(Permission::ManageStructure)?;
        non_empty("sectionCode", &input.section_code)?;
        if input.row_from < 1 || input.row_to < input.row_from || input.row_to - input.row_from > 500 {
            return Err(CoreError::validation("rowTo", "invalid row range"));
        }
        if input.plots_per_row < 1 || input.plots_per_row > 1000 {
            return Err(CoreError::validation("plotsPerRow", "invalid plot count"));
        }
        let first = input.first_plot_number.max(1);
        let ptype = self.get_plot_type(&input.plot_type_id)?;
        self.get_cemetery(&input.cemetery_id)?;
        let code = input.section_code.trim().to_uppercase();
        let pw = ptype.width_cm.unwrap_or(100) as f64 / 10.0;
        let ph = ptype.length_cm.unwrap_or(200) as f64 / 10.0;

        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        let existing: Option<(String, f64, f64)> = tx
            .query_row("SELECT id, map_x, map_y FROM sections WHERE cemetery_id=?1 AND code=?2 AND voided_at IS NULL", params![input.cemetery_id, code], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?;
        let (section_id, ox, oy) = match existing {
            Some(e) => e,
            None => {
                let (ox, oy) = match (input.origin_x, input.origin_y) {
                    (Some(x), Some(y)) => (x, y),
                    _ => {
                        let right: f64 = tx.query_row(
                            "SELECT coalesce(max(map_x + map_w), 0) FROM sections WHERE cemetery_id=?1 AND voided_at IS NULL",
                            [&input.cemetery_id],
                            |r| r.get(0),
                        )?;
                        (if right > 0.0 { right + 30.0 } else { 0.0 }, 0.0)
                    }
                };
                let id = new_id();
                tx.execute(
                    "INSERT INTO sections (id, cemetery_id, code, name, map_x, map_y, created_at, created_by, updated_at, updated_by)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?7,?8)",
                    params![id, input.cemetery_id, code, input.section_name, ox, oy, ts, s.user_name],
                )?;
                audit::record(&tx, s, "section", &id, "create", None::<&()>, Some(&serde_json::json!({"code": code, "name": input.section_name})), None)?;
                (id, ox, oy)
            }
        };

        let mut rows_created = 0;
        let mut plots_created = 0;
        for (ri, row_no) in (input.row_from..=input.row_to).enumerate() {
            let row_no_s = row_no.to_string();
            let row_id: String = match tx
                .query_row("SELECT id FROM plot_rows WHERE section_id=?1 AND number=?2 AND voided_at IS NULL", params![section_id, row_no_s], |r| r.get(0))
                .optional()?
            {
                Some(id) => id,
                None => {
                    let id = new_id();
                    tx.execute(
                        "INSERT INTO plot_rows (id, section_id, number, created_at, created_by, updated_at, updated_by) VALUES (?1,?2,?3,?4,?5,?4,?5)",
                        params![id, section_id, row_no_s, ts, s.user_name],
                    )?;
                    rows_created += 1;
                    id
                }
            };
            let y = oy + SECTION_PADDING + (row_no - 1) as f64 * (ph + ROW_SPACING);
            let _ = ri;
            for n in first..first + input.plots_per_row {
                let identifier = format!("{code}/{row_no}/{n}");
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM plots WHERE cemetery_id=?1 AND identifier=?2 AND voided_at IS NULL)",
                    params![input.cemetery_id, identifier],
                    |r| r.get(0),
                )?;
                if exists {
                    continue;
                }
                let x = ox + SECTION_PADDING + (n - 1) as f64 * (pw + PLOT_SPACING);
                let id = new_id();
                tx.execute(
                    "INSERT INTO plots (id, cemetery_id, section_id, row_id, number, identifier, plot_type_id, coffin_capacity, urn_capacity,
                        width_cm, length_cm, map_x, map_y, map_w, map_h, created_at, created_by, updated_at, updated_by)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?16,?17)",
                    params![
                        id,
                        input.cemetery_id,
                        section_id,
                        row_id,
                        n.to_string(),
                        identifier,
                        ptype.id,
                        ptype.coffin_capacity,
                        ptype.urn_capacity,
                        ptype.width_cm,
                        ptype.length_cm,
                        x,
                        y,
                        pw,
                        ph,
                        ts,
                        s.user_name
                    ],
                )?;
                if ptype.category == "crypt" {
                    create_slots(&tx, s, &id, ptype.coffin_capacity)?;
                }
                plots_created += 1;
            }
        }
        update_section_bounds(&tx, &section_id)?;
        audit::record(
            &tx,
            s,
            "section",
            &section_id,
            "bulk_create",
            None::<&()>,
            Some(&serde_json::json!({
                "rowFrom": input.row_from, "rowTo": input.row_to, "plotsPerRow": input.plots_per_row,
                "plotType": ptype.code, "rowsCreated": rows_created, "plotsCreated": plots_created
            })),
            None,
        )?;
        tx.commit()?;
        Ok(BulkCreateResult { section_id, rows_created, plots_created })
    }

    /// Creates a single plot in an existing (or new) row – used by import and quick entry.
    pub fn ensure_plot(&self, s: &Session, cemetery_id: &str, section_code: &str, row_no: &str, plot_no: &str, plot_type_id: &str) -> Result<String> {
        s.require(Permission::ManageStructure)?;
        let code = section_code.trim().to_uppercase();
        let identifier = format!("{code}/{}/{}", row_no.trim(), plot_no.trim());
        if let Some(id) = self
            .conn
            .query_row("SELECT id FROM plots WHERE cemetery_id=?1 AND identifier=?2 AND voided_at IS NULL", params![cemetery_id, identifier], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
        {
            return Ok(id);
        }
        let row_i: i32 = row_no.trim().parse().unwrap_or(1);
        let plot_i: i32 = plot_no.trim().parse().unwrap_or(1);
        // create section/row if needed through bulk_create semantics for a single cell
        if row_no.trim().parse::<i32>().is_ok() && plot_no.trim().parse::<i32>().is_ok() {
            self.bulk_create(
                s,
                &BulkCreateInput {
                    cemetery_id: cemetery_id.into(),
                    section_code: code.clone(),
                    section_name: String::new(),
                    row_from: row_i,
                    row_to: row_i,
                    plots_per_row: 1,
                    first_plot_number: plot_i,
                    plot_type_id: plot_type_id.into(),
                    origin_x: None,
                    origin_y: None,
                },
            )?;
        } else {
            return Err(CoreError::validation("identifier", "row and plot numbers must be numeric").with("value", &identifier));
        }
        Ok(self
            .conn
            .query_row("SELECT id FROM plots WHERE cemetery_id=?1 AND identifier=?2 AND voided_at IS NULL", params![cemetery_id, identifier], |r| r.get(0))?)
    }

    // ---------------------------------------------------------------- plots

    pub fn get_plot(&self, id: &str) -> Result<Plot> {
        self.conn.query_row("SELECT * FROM plots WHERE id=?1", [id], map_plot).optional()?.ok_or_else(|| CoreError::not_found("plot", id))
    }

    pub fn find_plot(&self, cemetery_id: &str, identifier: &str) -> Result<Option<Plot>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM plots WHERE cemetery_id=?1 AND lower(identifier)=lower(?2) AND voided_at IS NULL",
                params![cemetery_id, identifier.trim()],
                map_plot,
            )
            .optional()?)
    }

    /// Updates plot attributes (FR-TEM-06, FR-EML-03). A manual status override requires a reason.
    pub fn update_plot(&self, s: &Session, p: &Plot, reason: Option<&str>) -> Result<Plot> {
        s.require(Permission::ManageStructure)?;
        let old = self.get_plot(&p.id)?;
        if old.voided_at.is_some() {
            return Err(CoreError::new("immutable", "voided plot"));
        }
        if let Some(st) = &p.status_override {
            if PlotStatus::parse(st).is_none() {
                return Err(CoreError::validation("statusOverride", "invalid status"));
            }
        }
        let override_changed = old.status_override != p.status_override;
        let reason = if override_changed || old.protected != p.protected { Some(require_reason(reason)?) } else { reason.map(|r| r.to_string()) };
        if p.coffin_capacity < 0 || p.urn_capacity < 0 {
            return Err(CoreError::validation("coffinCapacity", "capacity must not be negative"));
        }
        let ptype = self.get_plot_type(&p.plot_type_id)?;
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE plots SET plot_type_id=?2, coffin_capacity=?3, urn_capacity=?4, width_cm=?5, length_cm=?6, rest_period_years=?7,
                status_override=?8, status_override_reason=?9, unusable=?10, protected=?11, protection_kind=?12, protection_reason=?13,
                further_burial_allowed=?14, notes=?15, gps_lat=?16, gps_lon=?17, crypt_number=?18, crypt_built_year=?19, crypt_permit=?20,
                crypt_holder_instructions=?21, updated_at=?22, updated_by=?23
             WHERE id=?1",
            params![
                p.id,
                ptype.id,
                p.coffin_capacity,
                p.urn_capacity,
                p.width_cm,
                p.length_cm,
                p.rest_period_years,
                p.status_override,
                if p.status_override.is_some() { reason.clone() } else { None },
                p.unusable,
                p.protected,
                if p.protected { p.protection_kind.clone() } else { None },
                opt_s(&p.protection_reason),
                p.further_burial_allowed,
                p.notes,
                p.gps_lat,
                p.gps_lon,
                opt_s(&p.crypt_number),
                p.crypt_built_year,
                opt_s(&p.crypt_permit),
                opt_s(&p.crypt_holder_instructions),
                now_ts(),
                s.user_name
            ],
        )?;
        if ptype.category == "crypt" {
            let have: i32 = tx.query_row("SELECT count(*) FROM crypt_slots WHERE plot_id=?1 AND voided_at IS NULL", [&p.id], |r| r.get(0))?;
            if have < p.coffin_capacity {
                let max_no: i32 = tx.query_row("SELECT coalesce(max(slot_no),0) FROM crypt_slots WHERE plot_id=?1", [&p.id], |r| r.get(0))?;
                for i in 0..(p.coffin_capacity - have) {
                    insert_slot(&tx, s, &p.id, max_no + 1 + i)?;
                }
            }
        }
        let new = tx.query_row("SELECT * FROM plots WHERE id=?1", [&p.id], map_plot)?;
        audit::record(&tx, s, "plot", &p.id, "update", Some(&old), Some(&new), reason.as_deref())?;
        tx.commit()?;
        Ok(new)
    }

    /// Map editing (FR-TER-01): move/resize/rotate plots and sections.
    pub fn update_geometry(&self, s: &Session, entity: &str, items: &[(String, f64, f64, f64, f64, f64)]) -> Result<()> {
        s.require(Permission::EditMap)?;
        let table = match entity {
            "plot" => "plots",
            "section" => "sections",
            _ => return Err(CoreError::validation("entity", "plot or section")),
        };
        let tx = crate::db::tx(&self.conn)?;
        for (id, x, y, w, h, rot) in items {
            if !(w.is_finite() && h.is_finite() && *w > 0.0 && *h > 0.0) {
                return Err(CoreError::validation("size", "invalid geometry"));
            }
            let n = tx.execute(
                &format!(
                    "UPDATE {table} SET map_x=?2, map_y=?3, map_w=?4, map_h=?5, map_rot=?6, updated_at=?7, updated_by=?8 WHERE id=?1 AND voided_at IS NULL"
                ),
                params![id, x, y, w, h, rot, now_ts(), s.user_name],
            )?;
            if n == 0 {
                return Err(CoreError::not_found(entity, id));
            }
        }
        audit::record(
            &tx,
            s,
            entity,
            &items.first().map(|i| i.0.clone()).unwrap_or_default(),
            "geometry",
            None::<&()>,
            Some(&serde_json::json!({"count": items.len()})),
            None,
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Merges two neighbouring plots into a double grave (FR-TER-01 "egyesítése").
    /// The absorbed plot is voided with a reference to the target; capacities are summed.
    pub fn merge_plots(&self, s: &Session, target_id: &str, absorbed_id: &str, reason: Option<&str>) -> Result<Plot> {
        s.require(Permission::EditMap)?;
        let reason = require_reason(reason)?;
        let t = self.get_plot(target_id)?;
        let a = self.get_plot(absorbed_id)?;
        if t.id == a.id || t.cemetery_id != a.cemetery_id {
            return Err(CoreError::validation("absorbedId", "plots must be different and in the same cemetery"));
        }
        let occupied: i64 =
            self.conn
                .query_row(&format!("SELECT count(*) FROM interment_events e WHERE e.plot_id=?1 AND {ACTIVE_PLACEMENT_SQL}"), [absorbed_id], |r| r.get(0))?;
        let leased: i64 = self.conn.query_row("SELECT count(*) FROM leases WHERE plot_id=?1 AND voided_at IS NULL", [absorbed_id], |r| r.get(0))?;
        if occupied > 0 || leased > 0 {
            return Err(CoreError::new("merge_not_empty", "the absorbed plot must be empty and without leases"));
        }
        let x0 = t.map_x.min(a.map_x);
        let y0 = t.map_y.min(a.map_y);
        let x1 = (t.map_x + t.map_w).max(a.map_x + a.map_w);
        let y1 = (t.map_y + t.map_h).max(a.map_y + a.map_h);
        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        tx.execute(
            "UPDATE plots SET coffin_capacity=coffin_capacity+?2, urn_capacity=urn_capacity+?3, map_x=?4, map_y=?5, map_w=?6, map_h=?7, updated_at=?8, updated_by=?9 WHERE id=?1",
            params![t.id, a.coffin_capacity, a.urn_capacity, x0, y0, x1 - x0, y1 - y0, ts, s.user_name],
        )?;
        tx.execute("UPDATE plots SET merged_into_id=?2, voided_at=?3, voided_by=?4, void_reason=?5 WHERE id=?1", params![a.id, t.id, ts, s.user_name, reason])?;
        audit::record(&tx, s, "plot", &t.id, "merge", Some(&a), Some(&t), Some(&reason))?;
        tx.commit()?;
        self.get_plot(target_id)
    }

    /// Generic voiding (FR-NAP-02) for structural records.
    pub fn void_record(&self, s: &Session, entity: &str, id: &str, reason: Option<&str>) -> Result<()> {
        s.require(Permission::VoidEntries)?;
        let reason = require_reason(reason)?;
        let table = match entity {
            "cemetery" => "cemeteries",
            "section" => "sections",
            "row" => "plot_rows",
            "plot" => "plots",
            "restriction" => "plot_restrictions",
            "monument" => "monuments",
            "notice" => "notices",
            "fee_rate" => "fee_rates",
            "holder" => "plot_holders",
            "deceased" => "deceased",
            "person" => "persons",
            "plot_type" => "plot_types",
            _ => return Err(CoreError::validation("entity", "unsupported entity")),
        };
        if entity == "plot" {
            let used: i64 = self.conn.query_row(
                "SELECT (SELECT count(*) FROM interment_events WHERE plot_id=?1 AND voided_at IS NULL) + (SELECT count(*) FROM leases WHERE plot_id=?1 AND voided_at IS NULL)",
                [id],
                |r| r.get(0),
            )?;
            if used > 0 {
                return Err(CoreError::new("void_in_use", "plot has events or leases"));
            }
        }
        if entity == "deceased" {
            let used: i64 = self.conn.query_row("SELECT count(*) FROM interment_events WHERE deceased_id=?1 AND voided_at IS NULL", [id], |r| r.get(0))?;
            if used > 0 {
                return Err(CoreError::new("void_in_use", "deceased has register entries; void the entries first"));
            }
        }
        let tx = crate::db::tx(&self.conn)?;
        let n = tx.execute(
            &format!("UPDATE {table} SET voided_at=?2, voided_by=?3, void_reason=?4 WHERE id=?1 AND voided_at IS NULL"),
            params![id, now_ts(), s.user_name, reason],
        )?;
        if n == 0 {
            return Err(CoreError::not_found(entity, id));
        }
        if entity == "deceased" || entity == "person" {
            tx.execute("DELETE FROM search_index WHERE ref_id=?1", [id])?;
        }
        audit::record::<(), ()>(&tx, s, entity, id, "void", None, None, Some(&reason))?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- restrictions & monuments

    pub fn add_restriction(&self, s: &Session, r: &Restriction) -> Result<Restriction> {
        s.require(Permission::RecordEvents)?;
        non_empty("text", &r.text)?;
        let kind = if r.kind == "ban" { "ban" } else { "condition" };
        self.get_plot(&r.plot_id)?;
        let id = new_id();
        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        tx.execute(
            "INSERT INTO plot_restrictions (id, plot_id, kind, text, valid_from, valid_to, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?7,?8)",
            params![id, r.plot_id, kind, r.text.trim(), opt_s(&r.valid_from), opt_s(&r.valid_to), ts, s.user_name],
        )?;
        let out = Restriction { id: id.clone(), kind: kind.into(), ..r.clone() };
        audit::record(&tx, s, "restriction", &id, "create", None::<&()>, Some(&out), None)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn add_monument(&self, s: &Session, m: &Monument) -> Result<Monument> {
        s.require(Permission::RecordEvents)?;
        non_empty("description", &m.description)?;
        parse_date(&m.entry_date)?;
        self.get_plot(&m.plot_id)?;
        let id = new_id();
        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        tx.execute(
            "INSERT INTO monuments (id, plot_id, entry_date, description, contractor, material, size, permit_date, protected, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?10,?11)",
            params![id, m.plot_id, m.entry_date, m.description.trim(), m.contractor, m.material, m.size, opt_s(&m.permit_date), m.protected, ts, s.user_name],
        )?;
        let out = Monument { id: id.clone(), ..m.clone() };
        audit::record(&tx, s, "monument", &id, "create", None::<&()>, Some(&out), None)?;
        tx.commit()?;
        Ok(out)
    }

    // ---------------------------------------------------------------- status

    /// Status inputs for one plot.
    pub(crate) fn status_inputs(&self, plot: &Plot) -> Result<StatusInputs> {
        let occupants: i32 =
            self.conn.query_row(&format!("SELECT count(*) FROM interment_events e WHERE e.plot_id=?1 AND {ACTIVE_PLACEMENT_SQL}"), [&plot.id], |r| r.get(0))?;
        let lease: Option<(String, String)> = self
            .conn
            .query_row("SELECT start_date, expiry_date FROM leases WHERE plot_id=?1 AND status='active' AND voided_at IS NULL", [&plot.id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        let last_notice: Option<String> =
            self.conn.query_row("SELECT max(notice_date) FROM notices WHERE plot_id=?1 AND voided_at IS NULL", [&plot.id], |r| r.get(0))?;
        Ok(StatusInputs {
            status_override: plot.status_override.as_deref().and_then(PlotStatus::parse),
            unusable: plot.unusable,
            protected: plot.protected && plot.protection_kind.as_deref() == Some("national"),
            occupants,
            lease_start: lease.as_ref().map(|l| parse_date(&l.0)).transpose()?,
            lease_expiry: lease.as_ref().map(|l| parse_date(&l.1)).transpose()?,
            last_notice: last_notice.as_deref().map(parse_date).transpose()?,
            resale_ready_at: plot.resale_ready_at.as_deref().map(|s| parse_date(&s[..10.min(s.len())])).transpose()?,
        })
    }

    pub fn plot_status(&self, plot: &Plot) -> Result<PlotStatus> {
        Ok(compute_status(&self.status_inputs(plot)?, today()))
    }

    pub fn plot_status_at(&self, plot: &Plot, date: NaiveDate) -> Result<PlotStatus> {
        Ok(compute_status(&self.status_inputs(plot)?, date))
    }

    /// Plot summaries for lists and the map, with status computed set-wise.
    pub fn plot_summaries(&self, f: &PlotFilter) -> Result<Page<PlotSummary>> {
        let mut where_ = vec!["p.voided_at IS NULL".to_string()];
        let mut args: Vec<Value> = vec![];
        let push = |cond: &str, v: Value, args: &mut Vec<Value>| {
            args.push(v);
            cond.replace('?', &format!("?{}", args.len()))
        };
        if let Some(v) = &f.cemetery_id {
            where_.push(push("p.cemetery_id = ?", Value::Text(v.clone()), &mut args));
        }
        if let Some(v) = &f.section_id {
            where_.push(push("p.section_id = ?", Value::Text(v.clone()), &mut args));
        }
        if let Some(v) = &f.row_id {
            where_.push(push("p.row_id = ?", Value::Text(v.clone()), &mut args));
        }
        if let Some(v) = &f.plot_type_id {
            where_.push(push("p.plot_type_id = ?", Value::Text(v.clone()), &mut args));
        }
        if let Some(v) = f.identifier_prefix.as_ref().filter(|v| !v.trim().is_empty()) {
            where_.push(push("p.identifier ILIKE ? ESCAPE '\\'", Value::Text(format!("{}%", like_escape(v.trim()))), &mut args));
        }
        if let Some(v) = &f.expiry_from {
            where_.push(push("l.expiry_date >= ?", Value::Text(v.clone()), &mut args));
        }
        if let Some(v) = &f.expiry_to {
            where_.push(push("l.expiry_date <= ?", Value::Text(v.clone()), &mut args));
        }
        if f.burial_from.is_some() || f.burial_to.is_some() {
            let from = f.burial_from.clone().unwrap_or_else(|| "0000-01-01".into());
            let to = f.burial_to.clone().unwrap_or_else(|| "9999-12-31".into());
            args.push(Value::Text(from));
            let a = args.len();
            args.push(Value::Text(to));
            let b = args.len();
            where_
                .push(format!("EXISTS (SELECT 1 FROM interment_events b WHERE b.plot_id=p.id AND b.voided_at IS NULL AND b.event_date BETWEEN ?{a} AND ?{b})"));
        }
        if f.protected_only {
            where_.push("p.protected = 1".into());
        }
        // Aggregates are computed once per query (grouped CTEs) instead of
        // per plot – keeps the 50 000-plot map under a second (NFR-TEL-02).
        let names_sql = if f.light { "NULL AS names".to_string() } else { "string_agg(trim(d.family_name || ' ' || d.given_name), ', ') AS names".to_string() };
        let names_join = if f.light { "" } else { "JOIN deceased d ON d.id = e.deceased_id" };
        let holder_sql = if f.light {
            "NULL"
        } else {
            "(SELECT trim(pe.family_name || ' ' || pe.given_name) FROM plot_holders h JOIN persons pe ON pe.id=h.person_id
               WHERE h.plot_id=p.id AND h.role='primary' AND h.end_date IS NULL AND h.voided_at IS NULL LIMIT 1)"
        };
        let sql = format!(
            "WITH removed AS (
                SELECT DISTINCT source_event_id AS id FROM interment_events
                WHERE source_event_id IS NOT NULL AND voided_at IS NULL),
             occ AS (
                SELECT e.plot_id, count(*) AS n, {names_sql}
                FROM interment_events e {names_join}
                LEFT JOIN removed rm ON rm.id = e.id
                WHERE e.voided_at IS NULL AND e.plot_id IS NOT NULL AND rm.id IS NULL
                  AND e.event_type IN ('coffin_burial','urn_placement','reburial','common_grave','relocation','ash_scattering')
                GROUP BY e.plot_id),
             nt AS (SELECT plot_id, max(notice_date) AS d FROM notices WHERE voided_at IS NULL GROUP BY plot_id)
             SELECT p.id, p.identifier, p.cemetery_id, p.section_id, p.row_id, t.code, t.name, p.protected, p.protection_kind,
                    p.status_override, p.unusable, p.resale_ready_at, p.map_x, p.map_y, p.map_w, p.map_h, p.map_rot,
                    l.start_date, l.expiry_date,
                    coalesce(occ.n, 0) AS occ,
                    occ.names AS occ_names,
                    nt.d AS last_notice,
                    {holder_sql} AS holder
             FROM plots p
             JOIN plot_types t ON t.id = p.plot_type_id
             LEFT JOIN leases l ON l.plot_id = p.id AND l.status='active' AND l.voided_at IS NULL
             LEFT JOIN occ ON occ.plot_id = p.id
             LEFT JOIN nt ON nt.plot_id = p.id
             WHERE {}",
            where_.join(" AND ")
        );
        let today = today();
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(args.iter()))?;
        let mut items = Vec::new();
        while let Some(r) = rows.next()? {
            let protected: bool = r.get(7)?;
            let pkind: Option<String> = r.get(8)?;
            let inputs = StatusInputs {
                status_override: r.get::<_, Option<String>>(9)?.as_deref().and_then(PlotStatus::parse),
                unusable: r.get(10)?,
                protected: protected && pkind.as_deref() == Some("national"),
                occupants: r.get(19)?,
                lease_start: r.get::<_, Option<String>>(17)?.as_deref().map(parse_date).transpose()?,
                lease_expiry: r.get::<_, Option<String>>(18)?.as_deref().map(parse_date).transpose()?,
                last_notice: r.get::<_, Option<String>>(21)?.as_deref().map(parse_date).transpose()?,
                resale_ready_at: r.get::<_, Option<String>>(11)?.map(|s| parse_date(&s[..10.min(s.len())])).transpose()?,
            };
            let status = compute_status(&inputs, today);
            if !f.statuses.is_empty() && !f.statuses.iter().any(|s| s == status.as_str()) {
                continue;
            }
            items.push(PlotSummary {
                id: r.get(0)?,
                identifier: r.get(1)?,
                cemetery_id: r.get(2)?,
                section_id: r.get(3)?,
                row_id: r.get(4)?,
                type_code: r.get(5)?,
                type_name: r.get(6)?,
                status,
                protected,
                occupants: inputs.occupants,
                lease_expiry: r.get(18)?,
                holder_name: r.get(22)?,
                occupant_names: r.get(20)?,
                x: r.get(12)?,
                y: r.get(13)?,
                w: r.get(14)?,
                h: r.get(15)?,
                rot: r.get(16)?,
            });
        }
        // natural sort by identifier parts (A/2/10 after A/2/9)
        items.sort_by_cached_key(|p| natural_key(&p.identifier));
        let total = items.len() as i64;
        let offset = f.offset.unwrap_or(0).max(0) as usize;
        let items = match f.limit {
            Some(l) if l > 0 => items.into_iter().skip(offset).take(l as usize).collect(),
            _ => items.into_iter().skip(offset).collect(),
        };
        Ok(Page { items, total })
    }

    // ---------------------------------------------------------------- detail

    pub fn plot_detail(&self, s: &Session, plot_id: &str) -> Result<PlotDetail> {
        let plot = self.get_plot(plot_id)?;
        let ptype = self.get_plot_type(&plot.plot_type_id)?;
        let (cemetery_name, section_code, row_number): (String, String, String) = self.conn.query_row(
            "SELECT c.name, s.code, r.number FROM plots p JOIN cemeteries c ON c.id=p.cemetery_id JOIN sections s ON s.id=p.section_id JOIN plot_rows r ON r.id=p.row_id WHERE p.id=?1",
            [plot_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let inputs = self.status_inputs(&plot)?;
        let status = compute_status(&inputs, today());
        let occupants = self.occupants(plot_id)?;
        let events = {
            let mut stmt = self.conn.prepare(&format!(
                "{EVENT_ROW_SQL} WHERE e.plot_id=?1 OR e.source_event_id IN (SELECT id FROM interment_events WHERE plot_id=?1) ORDER BY e.event_date, e.folio"
            ))?;
            let rows = stmt.query_map([plot_id], map_event_row)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let leases = self.leases_for_plot(plot_id)?;
        let current_lease = leases.iter().find(|l| l.status == "active" && l.voided_at.is_none()).cloned();
        let mut holders = self.holders_for_plot(plot_id)?;
        let can_personal = s.can(Permission::ViewPersonal);
        if !can_personal {
            for h in holders.iter_mut() {
                h.person_address = String::new();
            }
        } else if !holders.is_empty() {
            audit::record_simple(&self.conn, s, "plot", plot_id, "view_personal", Some("holders"))?;
        }
        let restrictions = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, plot_id, kind, text, valid_from, valid_to, voided_at FROM plot_restrictions WHERE plot_id=?1 ORDER BY created_at")?;
            let rows = stmt.query_map([plot_id], |r| {
                Ok(Restriction {
                    id: r.get(0)?,
                    plot_id: r.get(1)?,
                    kind: r.get(2)?,
                    text: r.get(3)?,
                    valid_from: r.get(4)?,
                    valid_to: r.get(5)?,
                    voided_at: r.get(6)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let monuments = self.monuments_for_plot(plot_id)?;
        let notices = {
            let mut stmt =
                self.conn.prepare("SELECT id, plot_id, lease_id, kind, notice_date, result, voided_at FROM notices WHERE plot_id=?1 ORDER BY notice_date")?;
            let rows = stmt.query_map([plot_id], |r| {
                Ok(Notice {
                    id: r.get(0)?,
                    plot_id: r.get(1)?,
                    lease_id: r.get(2)?,
                    kind: r.get(3)?,
                    notice_date: r.get(4)?,
                    result: r.get(5)?,
                    voided_at: r.get(6)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let crypt_slots = self.crypt_slots(plot_id)?;
        let grace = settings::get_i64(&self.conn, "rules.grace_period_days", Some(&plot.cemetery_id), 180)?;
        let resale_allowed_from = match (inputs.lease_expiry, status) {
            (Some(exp), PlotStatus::Notified | PlotStatus::Expired) => resale_allowed_from(exp, inputs.last_notice, grace).map(fmt_date),
            _ => None,
        };
        Ok(PlotDetail {
            rest_period_years: plot.rest_period_years.unwrap_or(ptype.rest_period_years),
            plot,
            plot_type: ptype,
            cemetery_name,
            section_code,
            row_number,
            status,
            occupants,
            events,
            leases,
            current_lease,
            holders,
            restrictions,
            monuments,
            notices,
            crypt_slots,
            resale_allowed_from,
        })
    }

    pub fn occupants(&self, plot_id: &str) -> Result<Vec<Occupant>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT e.id, e.folio, e.event_type, e.container, e.event_date, d.id, trim(d.name_prefix||' '||d.family_name||' '||d.given_name),
                    d.birth_date, d.death_date, s.slot_no
             FROM interment_events e JOIN deceased d ON d.id=e.deceased_id LEFT JOIN crypt_slots s ON s.id=e.crypt_slot_id
             WHERE e.plot_id=?1 AND {ACTIVE_PLACEMENT_SQL} ORDER BY e.event_date, e.folio"
        ))?;
        let rows = stmt.query_map([plot_id], |r| {
            Ok(Occupant {
                event_id: r.get(0)?,
                folio: r.get(1)?,
                event_type: r.get(2)?,
                container: r.get(3)?,
                event_date: r.get(4)?,
                deceased_id: r.get(5)?,
                deceased_name: r.get(6)?,
                birth_date: r.get(7)?,
                death_date: r.get(8)?,
                crypt_slot_no: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn monuments_for_plot(&self, plot_id: &str) -> Result<Vec<Monument>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, plot_id, entry_date, description, contractor, material, size, permit_date, protected, voided_at FROM monuments WHERE plot_id=?1 ORDER BY entry_date",
        )?;
        let rows = stmt.query_map([plot_id], |r| {
            Ok(Monument {
                id: r.get(0)?,
                plot_id: r.get(1)?,
                entry_date: r.get(2)?,
                description: r.get(3)?,
                contractor: r.get(4)?,
                material: r.get(5)?,
                size: r.get(6)?,
                permit_date: r.get(7)?,
                protected: r.get(8)?,
                voided_at: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// FR-SIR-02: per-slot occupancy of a crypt.
    pub fn crypt_slots(&self, plot_id: &str) -> Result<Vec<CryptSlot>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT s.id, s.plot_id, s.slot_no, s.label,
                    (SELECT trim(d.family_name||' '||d.given_name) FROM interment_events e JOIN deceased d ON d.id=e.deceased_id
                      WHERE e.crypt_slot_id=s.id AND {ACTIVE_PLACEMENT_SQL} ORDER BY e.event_date DESC LIMIT 1),
                    (SELECT e.id FROM interment_events e WHERE e.crypt_slot_id=s.id AND {ACTIVE_PLACEMENT_SQL} ORDER BY e.event_date DESC LIMIT 1),
                    (SELECT e.folio FROM interment_events e WHERE e.crypt_slot_id=s.id AND {ACTIVE_PLACEMENT_SQL} ORDER BY e.event_date DESC LIMIT 1),
                    (SELECT e.event_date FROM interment_events e WHERE e.crypt_slot_id=s.id AND {ACTIVE_PLACEMENT_SQL} ORDER BY e.event_date DESC LIMIT 1)
             FROM crypt_slots s WHERE s.plot_id=?1 AND s.voided_at IS NULL ORDER BY s.slot_no"
        ))?;
        let rows = stmt.query_map([plot_id], |r| {
            Ok(CryptSlot {
                id: r.get(0)?,
                plot_id: r.get(1)?,
                slot_no: r.get(2)?,
                label: r.get(3)?,
                occupant_name: r.get(4)?,
                occupant_event_id: r.get(5)?,
                occupant_folio: r.get(6)?,
                occupant_date: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

fn create_slots(tx: &crate::sql::Connection, s: &Session, plot_id: &str, n: i32) -> Result<()> {
    for i in 1..=n {
        insert_slot(tx, s, plot_id, i)?;
    }
    Ok(())
}

fn insert_slot(tx: &crate::sql::Connection, s: &Session, plot_id: &str, no: i32) -> Result<()> {
    let ts = now_ts();
    tx.execute(
        "INSERT INTO crypt_slots (id, plot_id, slot_no, label, created_at, created_by, updated_at, updated_by) VALUES (?1,?2,?3,'',?4,?5,?4,?5)",
        params![new_id(), plot_id, no, ts, s.user_name],
    )?;
    Ok(())
}

fn update_section_bounds(tx: &crate::sql::Connection, section_id: &str) -> Result<()> {
    let (x0, y0, x1, y1): (Option<f64>, Option<f64>, Option<f64>, Option<f64>) = tx.query_row(
        "SELECT min(map_x), min(map_y), max(map_x+map_w), max(map_y+map_h) FROM plots WHERE section_id=?1 AND voided_at IS NULL",
        [section_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    if let (Some(x0), Some(y0), Some(x1), Some(y1)) = (x0, y0, x1, y1) {
        tx.execute(
            "UPDATE sections SET map_x=?2, map_y=?3, map_w=?4, map_h=?5 WHERE id=?1",
            params![section_id, x0 - SECTION_PADDING, y0 - SECTION_PADDING, x1 - x0 + 2.0 * SECTION_PADDING, y1 - y0 + 2.0 * SECTION_PADDING],
        )?;
    }
    Ok(())
}

pub(crate) fn like_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Splits "A/12/3" into comparable parts: numbers numerically, text by HU collation.
pub fn natural_key(s: &str) -> Vec<(u8, i64, String)> {
    s.split('/')
        .map(|part| match part.trim().parse::<i64>() {
            Ok(n) => (0, n, String::new()),
            Err(_) => (1, 0, crate::text::normalize(part)),
        })
        .collect()
}
