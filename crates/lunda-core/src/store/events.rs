//! Register events with gap-free, immutable folio numbers (FR-ELH-02..05, JOG-02).

use chrono::{Datelike, NaiveDate};
use rusqlite::{params, params_from_iter, types::Value, OptionalExtension};

use super::*;
use crate::audit;
use crate::auth::{Permission, Session};
use crate::rules::{check_burial, ActiveRestriction, BurialContext, Container, PlotStatus, Verdict};
use crate::settings;
use crate::text::normalize;
use crate::util::{fmt_date, new_id, now_ts, parse_date};

/// Allocates the next folio inside the caller's transaction (FR-ELH-03).
/// Gap-free: the counter update and the event insert commit or roll back together.
/// Historical (paper book) folios are accepted if unused and push the counter forward.
pub(crate) fn allocate_folio(tx: &rusqlite::Connection, cemetery_id: &str, event_date: NaiveDate, historical: Option<i64>) -> Result<(String, i64)> {
    let scope: String = tx
        .query_row("SELECT folio_scope FROM cemeteries WHERE id=?1 AND voided_at IS NULL", [cemetery_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| CoreError::not_found("cemetery", cemetery_id))?;
    let key = if scope == "yearly" { event_date.year().to_string() } else { "all".to_string() };
    tx.execute(
        "INSERT OR IGNORE INTO register_sequences (cemetery_id, scope_key, next_value)
         VALUES (?1, ?2, (SELECT coalesce(max(folio),0)+1 FROM interment_events WHERE cemetery_id=?1 AND scope_key=?2))",
        params![cemetery_id, key],
    )?;
    match historical {
        Some(f) => {
            if f < 1 {
                return Err(CoreError::validation("historicalFolio", "folio must be positive"));
            }
            let used: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM interment_events WHERE cemetery_id=?1 AND scope_key=?2 AND folio=?3)",
                params![cemetery_id, key, f],
                |r| r.get(0),
            )?;
            if used {
                return Err(CoreError::new("folio_taken", "folio already used").with("folio", f));
            }
            tx.execute(
                "UPDATE register_sequences SET next_value = max(next_value, ?3 + 1) WHERE cemetery_id=?1 AND scope_key=?2",
                params![cemetery_id, key, f],
            )?;
            Ok((key, f))
        }
        None => {
            let folio: i64 = tx.query_row(
                "UPDATE register_sequences SET next_value = next_value + 1 WHERE cemetery_id=?1 AND scope_key=?2 RETURNING next_value - 1",
                params![cemetery_id, key],
                |r| r.get(0),
            )?;
            Ok((key, folio))
        }
    }
}

fn check_event_type(t: &str, allowed: &[&str]) -> Result<()> {
    if allowed.contains(&t) {
        Ok(())
    } else {
        Err(CoreError::validation("eventType", "invalid event type").with("value", t))
    }
}

pub(crate) fn verdict_error(v: &Verdict) -> CoreError {
    let codes: Vec<String> = v.blockers.iter().map(|b| b.code.clone()).collect();
    CoreError::new("burial_not_allowed", "burial is not allowed")
        .with("reasons", codes.join(","))
        .with("details", serde_json::to_string(&v.blockers).unwrap_or_default())
}

impl Store {
    /// Builds the burial context for a plot (FR-ELH-04).
    pub(crate) fn burial_context(&self, plot: &Plot, date: NaiveDate, container: Container) -> Result<BurialContext> {
        let ptype = self.get_plot_type(&plot.plot_type_id)?;
        let (coffins, urns): (i32, i32) = self.conn.query_row(
            &format!(
                "SELECT coalesce(sum(e.container='coffin'),0), coalesce(sum(e.container='urn'),0)
                 FROM interment_events e WHERE e.plot_id=?1 AND {ACTIVE_PLACEMENT_SQL}"
            ),
            [&plot.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let last_coffin: Option<String> = self.conn.query_row(
            &format!("SELECT max(e.event_date) FROM interment_events e WHERE e.plot_id=?1 AND e.container='coffin' AND {ACTIVE_PLACEMENT_SQL}"),
            [&plot.id],
            |r| r.get(0),
        )?;
        let dstr = fmt_date(date);
        let restrictions = {
            let mut st = self.conn.prepare(
                "SELECT kind, text FROM plot_restrictions WHERE plot_id=?1 AND voided_at IS NULL
                 AND (valid_from IS NULL OR valid_from <= ?2) AND (valid_to IS NULL OR valid_to >= ?2)",
            )?;
            let rows = st.query_map(params![plot.id, dstr], |r| Ok(ActiveRestriction { kind: r.get(0)?, text: r.get(1)? }))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let lease_expiry: Option<String> = self
            .conn
            .query_row("SELECT expiry_date FROM leases WHERE plot_id=?1 AND status='active' AND voided_at IS NULL", [&plot.id], |r| r.get(0))
            .optional()?;
        let status = self.plot_status_at(plot, date.max(crate::util::today()))?;
        Ok(BurialContext {
            burial_date: date,
            container,
            plot_category: ptype.category.clone(),
            coffin_capacity: plot.coffin_capacity,
            urn_capacity: plot.urn_capacity,
            coffins_present: coffins,
            urns_present: urns,
            last_coffin_burial: last_coffin.as_deref().map(parse_date).transpose()?,
            rest_period_years: plot.rest_period_years.unwrap_or(ptype.rest_period_years),
            restrictions,
            unusable: plot.unusable,
            further_burial_allowed: plot.further_burial_allowed,
            status,
            lease_expiry: lease_expiry.as_deref().map(parse_date).transpose()?,
            protected: plot.protected && plot.protection_kind.as_deref() == Some("national"),
        })
    }

    /// Preview for the "new burial" / "rátemetés" dialogs (UC-01 step 3–5, UC-02 step 2–3).
    pub fn preview_burial(&self, plot_id: &str, event_date: &str, container: &str) -> Result<BurialPreview> {
        let plot = self.get_plot(plot_id)?;
        let date = parse_date(event_date)?;
        let c = Container::parse(container);
        let ctx = self.burial_context(&plot, date, c)?;
        let verdict = check_burial(&ctx);
        let lease = self.lease_proposal_for_burial(&plot, date, c)?;
        let has_primary_holder = self.primary_holder_id(&self.conn, plot_id)?.is_some();
        let crypt_free_slots = self.crypt_slots(plot_id)?.into_iter().filter(|s| s.occupant_event_id.is_none()).collect();
        Ok(BurialPreview { verdict, status: ctx.status, has_primary_holder, lease, crypt_free_slots })
    }

    /// UC-01 / UC-02: records a burial-type event with folio, holder, lease and audit in one transaction.
    pub fn record_burial(&self, s: &Session, input: &BurialInput) -> Result<BurialResult> {
        s.require(Permission::RecordEvents)?;
        check_event_type(&input.event_type, &["coffin_burial", "urn_placement", "reburial", "common_grave", "ash_scattering"])?;
        let date = parse_date(&input.event_date)?;
        if date > crate::util::today() + chrono::Duration::days(30) {
            return Err(CoreError::validation("eventDate", "event date too far in the future").with("code", "date_in_future"));
        }
        let container = match input.event_type.as_str() {
            "coffin_burial" => Container::Coffin,
            "urn_placement" => Container::Urn,
            "ash_scattering" => Container::None,
            _ => Container::parse(&input.container),
        };
        let plot_id = input.plot_id.clone().ok_or_else(|| CoreError::validation("plotId", "plot is required").with("code", "required"))?;
        let plot = self.get_plot(&plot_id)?;
        if plot.voided_at.is_some() {
            return Err(CoreError::new("immutable", "voided plot"));
        }
        let ptype = self.get_plot_type(&plot.plot_type_id)?;
        if input.event_type == "common_grave" && ptype.category != "common" {
            return Err(CoreError::validation("plotId", "common grave event needs a common grave plot").with("code", "type_mismatch"));
        }
        if input.event_type == "ash_scattering" && ptype.category != "scatter" {
            return Err(CoreError::validation("plotId", "ash scattering needs a scatter area").with("code", "type_mismatch"));
        }

        let ctx = self.burial_context(&plot, date, container)?;
        let verdict = check_burial(&ctx);
        let override_reason = if verdict.allowed {
            None
        } else if input.historical_folio.is_some() && s.can(Permission::Import) {
            // QUESTIONS Q17: entries copied from the paper register record
            // past facts; the check result is kept as a note instead of blocking.
            let codes: Vec<String> = verdict.blockers.iter().map(|b| b.code.clone()).collect();
            Some(format!("Papír alapú nyilvántartásból átvett bejegyzés (ellenőrzés: {})", codes.join(", ")))
        } else {
            match input.override_reason.as_deref().map(str::trim).filter(|r| r.len() >= 3) {
                Some(r) if s.can(Permission::OverrideChecks) => Some(r.to_string()),
                _ => return Err(verdict_error(&verdict)),
            }
        };
        let proposal = if input.lease.create { self.lease_proposal_for_burial(&plot, date, container)? } else { None };

        let tx = crate::db::tx(&self.conn)?;
        let deceased_id = match &input.deceased {
            DeceasedRef::Existing { id } => {
                let d = tx.query_row("SELECT * FROM deceased WHERE id=?1", [id], map_deceased).optional()?.ok_or_else(|| CoreError::not_found("deceased", id))?;
                if d.voided_at.is_some() {
                    return Err(CoreError::new("immutable", "voided deceased"));
                }
                // a deceased may only rest in one place at a time
                let placed: i64 = tx.query_row(
                    &format!("SELECT count(*) FROM interment_events e WHERE e.deceased_id=?1 AND {ACTIVE_PLACEMENT_SQL}"),
                    [id],
                    |r| r.get(0),
                )?;
                if placed > 0 {
                    return Err(CoreError::new("already_placed", "the deceased already has an active placement; use relocation"));
                }
                id.clone()
            }
            DeceasedRef::New { deceased } => self.insert_deceased(&tx, s, deceased, true)?.id,
        };

        // crypt slot (FR-SIR-02)
        let crypt_slot_id = if ptype.category == "crypt" && container == Container::Coffin {
            let free: Vec<String> = {
                let mut st = tx.prepare(&format!(
                    "SELECT s.id FROM crypt_slots s WHERE s.plot_id=?1 AND s.voided_at IS NULL
                     AND NOT EXISTS (SELECT 1 FROM interment_events e WHERE e.crypt_slot_id=s.id AND {ACTIVE_PLACEMENT_SQL}) ORDER BY s.slot_no"
                ))?;
                let rows = st.query_map([&plot_id], |r| r.get(0))?;
                rows.collect::<std::result::Result<_, _>>()?
            };
            match &input.crypt_slot_id {
                Some(sid) if free.contains(sid) => Some(sid.clone()),
                Some(_) => return Err(CoreError::new("crypt_slot_taken", "crypt slot is occupied or not part of this crypt")),
                None => free.first().cloned(),
            }
        } else {
            None
        };

        let arranger_id = match &input.arranger {
            Some(r) => Some(self.resolve_person(&tx, s, r)?),
            None => None,
        };

        // rendelkezési jogosult (UC-01 step 4)
        let needs_holder = !matches!(ptype.category.as_str(), "scatter" | "common");
        let mut holder_id = self.primary_holder_id(&tx, &plot_id)?;
        if holder_id.is_none() && needs_holder {
            let pid = match (&input.holder, &arranger_id) {
                (Some(h), _) => self.resolve_person(&tx, s, h)?,
                (None, Some(a)) => a.clone(),
                (None, None) => return Err(CoreError::validation("holder", "the plot has no holder; a holder is required").with("code", "holder_required")),
            };
            self.insert_holder(&tx, s, &plot_id, &pid, "primary", input.holder_relation.as_deref().unwrap_or(""), &input.event_date, "megváltás / eltemettetés", "")?;
            holder_id = Some(pid);
        }

        let (scope_key, folio) = allocate_folio(&tx, &plot.cemetery_id, date, input.historical_folio)?;
        let event_id = new_id();
        let ts = now_ts();
        tx.execute(
            "INSERT INTO interment_events (id, cemetery_id, scope_key, folio, historical, event_type, container, event_date, deceased_id, plot_id,
                crypt_slot_id, arranger_id, override_reason, notes, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?15,?16)",
            params![
                event_id, plot.cemetery_id, scope_key, folio, input.historical_folio.is_some(), input.event_type, container.as_str(),
                input.event_date, deceased_id, plot_id, crypt_slot_id, arranger_id, override_reason, input.notes, ts, s.user_name
            ],
        )?;
        let event = tx.query_row("SELECT * FROM interment_events WHERE id=?1", [&event_id], map_event)?;
        audit::record(&tx, s, "event", &event_id, "create", None::<&()>, Some(&event), override_reason.as_deref())?;

        let lease = match proposal {
            Some(p) => {
                let duration = input.lease.duration_years.unwrap_or(p.duration_years);
                let expiry = if input.lease.duration_years.is_some() {
                    fmt_date(crate::rules::lease_expiry(parse_date(&p.start_date)?, duration))
                } else {
                    p.expiry_date.clone()
                };
                let fee = if p.fee_exempt { 0 } else { input.lease.fee_huf.unwrap_or(p.fee_huf) };
                Some(self.insert_lease(&tx, s, &plot_id, holder_id.as_deref(), &p.kind, &p.start_date, duration, &expiry, fee, p.fee_rate_id.as_deref(), p.fee_exempt, Some(&event_id), &input.lease.notes)?)
            }
            None => None,
        };
        tx.commit()?;
        Ok(BurialResult { folio: event.folio, event, lease, verdict })
    }

    /// Urn release, urn removal, relocation (FR-ELH-02, JOG-15).
    pub fn record_removal(&self, s: &Session, input: &RemovalInput) -> Result<InterEvent> {
        s.require(Permission::RecordEvents)?;
        check_event_type(&input.event_type, &["urn_release", "urn_removal", "relocation"])?;
        let date = parse_date(&input.event_date)?;
        let src = self
            .conn
            .query_row("SELECT * FROM interment_events WHERE id=?1", [&input.source_event_id], map_event)
            .optional()?
            .ok_or_else(|| CoreError::not_found("event", &input.source_event_id))?;
        let active: bool = self.conn.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM interment_events e WHERE e.id=?1 AND {ACTIVE_PLACEMENT_SQL})"),
            [&src.id],
            |r| r.get(0),
        )?;
        if !active {
            return Err(CoreError::new("not_active_placement", "the source is not an active placement"));
        }
        if date < parse_date(&src.event_date)? {
            return Err(CoreError::validation("eventDate", "removal before placement").with("code", "date_before_placement"));
        }
        if matches!(input.event_type.as_str(), "urn_release" | "urn_removal") && src.container != "urn" {
            return Err(CoreError::validation("sourceEventId", "only urns can be released or removed").with("code", "not_an_urn"));
        }
        if input.event_type == "urn_release" {
            non_empty("receiverName", &input.receiver_name)?; // JOG-15
        }
        let mut target: Option<Plot> = None;
        if input.event_type == "relocation" {
            match &input.target_plot_id {
                Some(tid) => {
                    let t = self.get_plot(tid)?;
                    let ctx = self.burial_context(&t, date, Container::parse(&src.container))?;
                    let v = check_burial(&ctx);
                    if !v.allowed {
                        return Err(verdict_error(&v));
                    }
                    target = Some(t);
                }
                None => non_empty("destination", &input.destination)?,
            }
        }
        let tx = crate::db::tx(&self.conn)?;
        let (scope_key, folio) = allocate_folio(&tx, &src.cemetery_id, date, input.historical_folio)?;
        let id = new_id();
        let ts = now_ts();
        let container = if input.event_type == "relocation" { src.container.clone() } else { "urn".to_string() };
        tx.execute(
            "INSERT INTO interment_events (id, cemetery_id, scope_key, folio, historical, event_type, container, event_date, deceased_id, plot_id,
                receiver_name, receiver_address, destination, source_event_id, notes, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?16,?17)",
            params![
                id, src.cemetery_id, scope_key, folio, input.historical_folio.is_some(), input.event_type, container, input.event_date,
                src.deceased_id, target.as_ref().map(|t| t.id.clone()), input.receiver_name.trim(), input.receiver_address.trim(),
                input.destination.trim(), src.id, input.notes, ts, s.user_name
            ],
        )?;
        let ev = tx.query_row("SELECT * FROM interment_events WHERE id=?1", [&id], map_event)?;
        audit::record(&tx, s, "event", &id, "create", None::<&()>, Some(&ev), None)?;
        tx.commit()?;
        Ok(ev)
    }

    /// FR-NAP-02: a register entry is never deleted, only voided with a reason; its folio stays.
    pub fn void_event(&self, s: &Session, event_id: &str, reason: Option<&str>) -> Result<InterEvent> {
        s.require(Permission::VoidEntries)?;
        let reason = require_reason(reason)?;
        let ev = self.get_event(event_id)?;
        if ev.voided_at.is_some() {
            return Err(CoreError::new("already_voided", "already voided"));
        }
        let dependants: i64 = self.conn.query_row(
            "SELECT count(*) FROM interment_events WHERE source_event_id=?1 AND voided_at IS NULL",
            [event_id],
            |r| r.get(0),
        )?;
        if dependants > 0 {
            return Err(CoreError::new("void_in_use", "a later removal refers to this entry; void that first"));
        }
        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        tx.execute(
            "UPDATE interment_events SET voided_at=?2, voided_by=?3, void_reason=?4, updated_at=?2, updated_by=?3 WHERE id=?1",
            params![event_id, ts, s.user_name, reason],
        )?;
        audit::record(&tx, s, "event", event_id, "void", Some(&ev), None::<&()>, Some(&reason))?;
        // a lease created by this event is voided as well
        let leases: Vec<String> = {
            let mut st = tx.prepare("SELECT id FROM leases WHERE related_event_id=?1 AND voided_at IS NULL")?;
            let rows = st.query_map([event_id], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for lid in leases {
            self.void_lease_tx(&tx, s, &lid, &reason)?;
        }
        tx.commit()?;
        self.get_event(event_id)
    }

    /// FR-JOG-03: correction of an entry (date, notes, arranger, receiver, destination)
    /// with mandatory justification; the folio cannot change.
    pub fn correct_event(&self, s: &Session, e: &InterEvent, reason: Option<&str>) -> Result<InterEvent> {
        s.require(Permission::CorrectEntries)?;
        let reason = require_reason(reason)?;
        parse_date(&e.event_date)?;
        let old = self.get_event(&e.id)?;
        if old.voided_at.is_some() {
            return Err(CoreError::new("immutable", "voided record is final"));
        }
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE interment_events SET event_date=?2, notes=?3, arranger_id=?4, receiver_name=?5, receiver_address=?6, destination=?7, updated_at=?8, updated_by=?9 WHERE id=?1",
            params![e.id, e.event_date, e.notes, e.arranger_id, e.receiver_name, e.receiver_address, e.destination, now_ts(), s.user_name],
        )?;
        let new = tx.query_row("SELECT * FROM interment_events WHERE id=?1", [&e.id], map_event)?;
        audit::record(&tx, s, "event", &e.id, "correct", Some(&old), Some(&new), Some(&reason))?;
        tx.commit()?;
        Ok(new)
    }

    pub fn get_event(&self, id: &str) -> Result<InterEvent> {
        self.conn
            .query_row("SELECT * FROM interment_events WHERE id=?1", [id], map_event)
            .optional()?
            .ok_or_else(|| CoreError::not_found("event", id))
    }

    pub fn get_event_row(&self, id: &str) -> Result<EventRow> {
        self.conn
            .query_row(&format!("{EVENT_ROW_SQL} WHERE e.id=?1"), [id], map_event_row)
            .optional()?
            .ok_or_else(|| CoreError::not_found("event", id))
    }

    /// Register book rows (nyilvántartó könyv / sírbolt könyv) with filters.
    pub fn list_events(&self, f: &EventFilter) -> Result<Page<EventRow>> {
        let mut conds: Vec<String> = vec![];
        let mut args: Vec<Value> = vec![];
        macro_rules! cond {
            ($sql:expr, $v:expr) => {{
                args.push($v);
                conds.push($sql.replace('?', &format!("?{}", args.len())));
            }};
        }
        if let Some(v) = &f.cemetery_id {
            cond!("e.cemetery_id = ?", Value::Text(v.clone()));
        }
        if let Some(v) = &f.section_id {
            cond!("p.section_id = ?", Value::Text(v.clone()));
        }
        if let Some(v) = &f.date_from {
            cond!("e.event_date >= ?", Value::Text(v.clone()));
        }
        if let Some(v) = &f.date_to {
            cond!("e.event_date <= ?", Value::Text(v.clone()));
        }
        if let Some(v) = f.folio_from {
            cond!("e.folio >= ?", Value::Integer(v));
        }
        if let Some(v) = f.folio_to {
            cond!("e.folio <= ?", Value::Integer(v));
        }
        if let Some(q) = f.query.as_ref().map(|q| normalize(q)).filter(|q| !q.is_empty()) {
            cond!("d.search_text LIKE '%' || ? || '%'", Value::Text(q));
        }
        if !f.event_types.is_empty() {
            let ph: Vec<String> = f
                .event_types
                .iter()
                .map(|t| {
                    args.push(Value::Text(t.clone()));
                    format!("?{}", args.len())
                })
                .collect();
            conds.push(format!("e.event_type IN ({})", ph.join(",")));
        }
        if f.crypts_only {
            conds.push(
                "(EXISTS (SELECT 1 FROM plots p2 JOIN plot_types t2 ON t2.id=p2.plot_type_id WHERE p2.id=e.plot_id AND t2.category='crypt')
                  OR EXISTS (SELECT 1 FROM interment_events s2 JOIN plots p3 ON p3.id=s2.plot_id JOIN plot_types t3 ON t3.id=p3.plot_type_id WHERE s2.id=e.source_event_id AND t3.category='crypt'))"
                    .into(),
            );
        }
        if !f.include_voided {
            conds.push("e.voided_at IS NULL".into());
        }
        let where_ = if conds.is_empty() { String::new() } else { format!(" WHERE {}", conds.join(" AND ")) };
        let total: i64 = self.conn.query_row(
            &format!(
                "SELECT count(*) FROM interment_events e JOIN deceased d ON d.id=e.deceased_id LEFT JOIN plots p ON p.id=e.plot_id{where_}"
            ),
            params_from_iter(args.iter()),
            |r| r.get(0),
        )?;
        let order = match f.order.as_deref() {
            Some("date_desc") => "e.event_date DESC, e.folio DESC",
            _ => "e.cemetery_id, e.scope_key, e.folio",
        };
        let limit = f.limit.unwrap_or(-1);
        let offset = f.offset.unwrap_or(0);
        let sql = format!("{EVENT_ROW_SQL}{where_} ORDER BY {order} LIMIT {limit} OFFSET {offset}");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args.iter()), map_event_row)?;
        Ok(Page { items: rows.collect::<std::result::Result<_, _>>()?, total })
    }

    /// Lease proposal for a burial (UC-01 step 5 / UC-02 step 3, FR-MEG-03).
    pub(crate) fn lease_proposal_for_burial(&self, plot: &Plot, date: NaiveDate, container: Container) -> Result<Option<LeaseProposal>> {
        let ptype = self.get_plot_type(&plot.plot_type_id)?;
        if ptype.default_lease_years <= 0 || matches!(ptype.category.as_str(), "scatter" | "common") {
            return Ok(None);
        }
        let exempt = plot.protected && plot.protection_kind.as_deref() == Some("national"); // FR-MEG-08
        let rest = match container {
            Container::Coffin => plot.rest_period_years.unwrap_or(ptype.rest_period_years),
            _ => settings::get_i64(&self.conn, "rules.urn_rest_period_years", Some(&plot.cemetery_id), 0)? as i32,
        };
        let active: Option<(String, String)> = self
            .conn
            .query_row("SELECT start_date, expiry_date FROM leases WHERE plot_id=?1 AND status='active' AND voided_at IS NULL", [&plot.id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let occupants: i64 = self.conn.query_row(
            &format!("SELECT count(*) FROM interment_events e WHERE e.plot_id=?1 AND {ACTIVE_PLACEMENT_SQL}"),
            [&plot.id],
            |r| r.get(0),
        )?;
        let resellable = self.plot_status(plot)? == PlotStatus::Resellable;
        if (active.is_none() && occupants == 0) || resellable {
            // first purchase (megváltás)
            let duration = ptype.default_lease_years.max(rest);
            let rate = self.current_rate(&plot.cemetery_id, &plot.plot_type_id, "purchase", date)?;
            let expiry = crate::rules::lease_expiry(date, duration);
            return Ok(Some(LeaseProposal {
                kind: "purchase".into(),
                start_date: fmt_date(date),
                duration_years: duration,
                expiry_date: fmt_date(expiry),
                fee_huf: if exempt { 0 } else { rate.as_ref().map(|r| r.amount_huf).unwrap_or(0) },
                fee_rate_id: rate.map(|r| r.id),
                fee_exempt: exempt,
                extension_days: 0,
            }));
        }
        // rátemetés: extension to burial date + rest period, pro-rata fee (Q4)
        let current_expiry = active.as_ref().map(|a| parse_date(&a.1)).transpose()?;
        let rate = match self.current_rate(&plot.cemetery_id, &plot.plot_type_id, "renewal", date)? {
            Some(r) => Some(r),
            None => self.current_rate(&plot.cemetery_id, &plot.plot_type_id, "purchase", date)?,
        };
        let ext = crate::rules::extension_on_reburial(
            current_expiry,
            date,
            rest,
            ptype.default_lease_years,
            rate.as_ref().map(|r| r.amount_huf).unwrap_or(0),
            exempt,
        );
        if ext.extension_days == 0 {
            return Ok(None);
        }
        Ok(Some(LeaseProposal {
            kind: "renewal".into(),
            start_date: fmt_date(date),
            duration_years: rest,
            expiry_date: fmt_date(ext.new_expiry),
            fee_huf: ext.fee_huf,
            fee_rate_id: rate.map(|r| r.id),
            fee_exempt: exempt,
            extension_days: ext.extension_days,
        }))
    }
}
