//! Daily operation of a cemetery (W8–W11, docs/FRS-UZEMELTETES.md):
//! funeral calendar with bookable facilities, contractors and their work
//! notices, work orders (grave digging, recurring maintenance), case files
//! with gap-free filing numbers and attachments, and inspection walks with
//! the neglected-plot procedure.
//!
//! Lists are returned as JSON objects built by Postgres (`row_to_json`) with
//! camelCase keys, so the UI gets exactly the joined columns it shows.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::finance::ChargeInput;
use super::*;
use crate::audit;
use crate::auth::{Permission, Session};
use crate::sql::{params, OptionalExtension, Value as SqlValue};
use crate::util::{fmt_date, new_id, now_ts, parse_date, sha256_hex, today};

/// Largest attachment accepted (a scanned application or a phone photo).
/// Base64 adds a third, and Vercel functions take at most 4.5 MB per request.
pub const MAX_ATTACHMENT_BYTES: usize = 3 * 1024 * 1024;
/// Days the holder gets to tidy a neglected plot after the notice (FR-UZE-03).
pub const NEGLECT_DEADLINE_DAYS: i64 = 60;

// ---------------------------------------------------------------- inputs

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FacilityInput {
    pub id: Option<String>,
    pub cemetery_id: String,
    pub name: String,
    pub kind: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ContractorInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub tax_number: String,
    pub postcode: String,
    pub city: String,
    pub address: String,
    pub contact_name: String,
    pub phone: String,
    pub email: String,
    pub permit_number: String,
    pub permit_valid_until: Option<String>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BookingInput {
    pub id: Option<String>,
    pub cemetery_id: String,
    pub title: String,
    /// local time, `YYYY-MM-DDTHH:MM`
    pub starts_at: String,
    pub ends_at: String,
    pub facility_id: Option<String>,
    pub plot_id: Option<String>,
    pub contractor_id: Option<String>,
    pub deceased_name: String,
    /// "" | coffin | urn
    pub container: String,
    pub notes: String,
    /// also create the grave-digging work order for the plot
    pub dig_order: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkNoticeInput {
    pub cemetery_id: String,
    pub contractor_id: String,
    pub plot_id: Option<String>,
    pub kind: String,
    pub work_date: String,
    pub description: String,
    /// raise the maintenance contribution from the fee decree
    pub charge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WorkOrderInput {
    pub id: Option<String>,
    pub cemetery_id: String,
    pub kind: String,
    pub title: String,
    pub plot_id: Option<String>,
    pub booking_id: Option<String>,
    pub due_date: String,
    pub assignee: String,
    pub details: String,
    pub recurrence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CaseInput {
    pub cemetery_id: String,
    pub kind: String,
    pub subject: String,
    pub applicant_id: Option<String>,
    pub applicant_name: String,
    pub plot_id: Option<String>,
    pub contractor_id: Option<String>,
    pub received_on: Option<String>,
    pub deadline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CaseUpdate {
    pub status: Option<String>,
    pub decision: Option<String>,
    pub deadline: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ConditionInput {
    pub plot_id: String,
    pub condition: String,
    pub note: String,
    pub observed_on: Option<String>,
    /// for neglected / dangerous plots: open a neglect case right away
    pub open_case: bool,
}

const CASE_KINDS: &[&str] = &[
    "lease_contract",
    "renewal",
    "monument_permit",
    "monument_removal",
    "exhumation",
    "relocation",
    "ash_release",
    "holder_transfer",
    "neglect",
    "complaint",
    "other",
];

// ---------------------------------------------------------------- helpers

fn camel(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut up = false;
    for ch in key.chars() {
        if ch == '_' {
            up = true;
        } else if up {
            out.extend(ch.to_uppercase());
            up = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn camelize(v: Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (camel(&k), camelize(v))).collect::<Map<_, _>>()),
        Value::Array(a) => Value::Array(a.into_iter().map(camelize).collect()),
        other => other,
    }
}

/// Runs `sql` and returns each row as a camelCase JSON object.
pub(crate) fn json_rows(conn: &crate::sql::Connection, sql: &str, p: Vec<SqlValue>) -> Result<Vec<Value>> {
    let mut st = conn.prepare(&format!("SELECT row_to_json(q)::text FROM ({sql}) q"))?;
    let rows = st.query_map(crate::sql::params_from_iter(p), |r| r.get::<_, String>(0))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(camelize(serde_json::from_str(&r?)?));
    }
    Ok(out)
}

fn json_row(conn: &crate::sql::Connection, sql: &str, p: Vec<SqlValue>, entity: &str, id: &str) -> Result<Value> {
    json_rows(conn, sql, p)?.into_iter().next().ok_or_else(|| CoreError::not_found(entity, id))
}

fn s(v: impl Into<String>) -> SqlValue {
    SqlValue::Text(v.into())
}

fn parse_local(field: &str, v: &str) -> Result<NaiveDateTime> {
    let v = v.trim();
    NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M")
        .or_else(|_| NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M:%S"))
        .map_err(|_| CoreError::validation(field, format!("invalid time: {v}")).with("code", "invalid_time"))
}

fn fmt_local(t: NaiveDateTime) -> String {
    t.format("%Y-%m-%dT%H:%M").to_string()
}

fn date_or_today(v: &Option<String>) -> Result<String> {
    match opt_s(v) {
        Some(d) => Ok(fmt_date(parse_date(&d)?)),
        None => Ok(fmt_date(today())),
    }
}

fn one_of(field: &str, v: &str, allowed: &[&str]) -> Result<String> {
    if allowed.contains(&v) {
        Ok(v.to_string())
    } else {
        Err(CoreError::validation(field, format!("unknown value {v}")).with("code", "invalid_choice"))
    }
}

/// Next date of a recurring work order.
pub fn next_due(d: NaiveDate, recurrence: &str) -> Option<NaiveDate> {
    match recurrence {
        "weekly" => Some(d + Duration::days(7)),
        "monthly" => d.checked_add_months(chrono::Months::new(1)),
        "yearly" => d.with_year(d.year() + 1).or_else(|| d.checked_add_months(chrono::Months::new(12))),
        _ => None,
    }
}

const BOOKING_SELECT: &str = "SELECT b.id, b.cemetery_id, b.title, b.starts_at, b.ends_at, b.facility_id, f.name AS facility_name,
        b.plot_id, p.identifier AS plot_identifier, b.contractor_id, c.name AS contractor_name, b.deceased_name, b.container,
        b.event_id, b.status, b.notes, b.created_at, b.created_by
    FROM bookings b
    LEFT JOIN facilities f ON f.id = b.facility_id
    LEFT JOIN plots p ON p.id = b.plot_id
    LEFT JOIN contractors c ON c.id = b.contractor_id";

const ORDER_SELECT: &str = "SELECT w.id, w.cemetery_id, w.kind, w.title, w.plot_id, p.identifier AS plot_identifier, w.booking_id,
        b.starts_at AS booking_starts_at, w.due_date, w.assignee, w.details, w.recurrence, w.status, w.done_at, w.done_by, w.done_note,
        w.created_at, w.created_by
    FROM work_orders w
    LEFT JOIN plots p ON p.id = w.plot_id
    LEFT JOIN bookings b ON b.id = w.booking_id";

const CASE_SELECT: &str = "SELECT k.id, k.cemetery_id, k.case_number, k.kind, k.subject, k.applicant_id, k.applicant_name, k.plot_id,
        p.identifier AS plot_identifier, k.contractor_id, c.name AS contractor_name, k.received_on, k.deadline, k.status, k.decision,
        k.created_at, k.created_by, k.updated_at, k.voided_at, k.void_reason,
        (SELECT count(*) FROM attachments a WHERE a.entity = 'case' AND a.entity_id = k.id AND a.voided_at IS NULL) AS attachments
    FROM cases k
    LEFT JOIN plots p ON p.id = k.plot_id
    LEFT JOIN contractors c ON c.id = k.contractor_id";

impl Store {
    // ---------------------------------------------------------------- facilities (FR-SZE-01)

    pub fn save_facility(&self, se: &Session, f: &FacilityInput) -> Result<Value> {
        se.require(Permission::Operations)?;
        non_empty("cemeteryId", &f.cemetery_id)?;
        non_empty("name", &f.name)?;
        let kind = one_of("kind", if f.kind.is_empty() { "chapel" } else { &f.kind }, &["chapel", "cooler", "hall", "other"])?;
        let ts = now_ts();
        let tx = crate::db::tx(&self.conn)?;
        let id = match &f.id {
            Some(id) => {
                let n = tx.execute(
                    "UPDATE facilities SET name=?2, kind=?3, notes=?4, updated_at=?5, updated_by=?6 WHERE id=?1 AND voided_at IS NULL",
                    params![id, f.name.trim(), kind, f.notes.trim(), ts, se.user_name],
                )?;
                if n == 0 {
                    return Err(CoreError::not_found("facility", id));
                }
                id.clone()
            }
            None => {
                let id = new_id();
                tx.execute(
                    "INSERT INTO facilities (id, cemetery_id, name, kind, notes, created_at, created_by, updated_at, updated_by)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?6,?7)",
                    params![id, f.cemetery_id, f.name.trim(), kind, f.notes.trim(), ts, se.user_name],
                )?;
                id
            }
        };
        audit::record(&tx, se, "facility", &id, if f.id.is_some() { "update" } else { "create" }, None::<&()>, Some(f), None)?;
        tx.commit()?;
        json_row(&self.conn, "SELECT * FROM facilities WHERE id = ?1", vec![s(&id)], "facility", &id)
    }

    pub fn list_facilities(&self, cemetery_id: &str) -> Result<Vec<Value>> {
        json_rows(
            &self.conn,
            "SELECT id, cemetery_id, name, kind, notes FROM facilities WHERE cemetery_id = ?1 AND voided_at IS NULL ORDER BY name COLLATE \"hu-x-icu\"",
            vec![s(cemetery_id)],
        )
    }

    // ---------------------------------------------------------------- contractors (FR-VAL-01)

    pub fn save_contractor(&self, se: &Session, c: &ContractorInput) -> Result<Value> {
        se.require(Permission::Operations)?;
        non_empty("name", &c.name)?;
        let kind = one_of("kind", if c.kind.is_empty() { "funeral" } else { &c.kind }, &["funeral", "stonemason", "gardener", "other"])?;
        let valid = match opt_s(&c.permit_valid_until) {
            Some(d) => Some(fmt_date(parse_date(&d)?)),
            None => None,
        };
        let ts = now_ts();
        let tx = crate::db::tx(&self.conn)?;
        let id = c.id.clone().unwrap_or_else(new_id);
        if c.id.is_some() {
            let n = tx.execute(
                "UPDATE contractors SET name=?2, kind=?3, tax_number=?4, postcode=?5, city=?6, address=?7, contact_name=?8, phone=?9, email=?10,
                        permit_number=?11, permit_valid_until=?12, notes=?13, updated_at=?14, updated_by=?15
                 WHERE id=?1 AND voided_at IS NULL",
                params![
                    id,
                    c.name.trim(),
                    kind,
                    c.tax_number.trim(),
                    c.postcode.trim(),
                    c.city.trim(),
                    c.address.trim(),
                    c.contact_name.trim(),
                    c.phone.trim(),
                    c.email.trim(),
                    c.permit_number.trim(),
                    valid,
                    c.notes.trim(),
                    ts,
                    se.user_name
                ],
            )?;
            if n == 0 {
                return Err(CoreError::not_found("contractor", &id));
            }
        } else {
            tx.execute(
                "INSERT INTO contractors (id, name, kind, tax_number, postcode, city, address, contact_name, phone, email, permit_number,
                                          permit_valid_until, notes, created_at, created_by, updated_at, updated_by)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?14,?15)",
                params![
                    id,
                    c.name.trim(),
                    kind,
                    c.tax_number.trim(),
                    c.postcode.trim(),
                    c.city.trim(),
                    c.address.trim(),
                    c.contact_name.trim(),
                    c.phone.trim(),
                    c.email.trim(),
                    c.permit_number.trim(),
                    valid,
                    c.notes.trim(),
                    ts,
                    se.user_name
                ],
            )?;
        }
        audit::record(&tx, se, "contractor", &id, if c.id.is_some() { "update" } else { "create" }, None::<&()>, Some(c), None)?;
        tx.commit()?;
        Ok(self.list_contractors(true)?.into_iter().find(|v| v["id"] == id).unwrap_or(Value::Null))
    }

    /// Contractors with their permit state and the number of work notices.
    pub fn list_contractors(&self, include_voided: bool) -> Result<Vec<Value>> {
        json_rows(
            &self.conn,
            &format!(
                "SELECT c.id, c.name, c.kind, c.tax_number, c.postcode, c.city, c.address, c.contact_name, c.phone, c.email,
                    c.permit_number, c.permit_valid_until, c.notes, c.voided_at,
                    (c.permit_valid_until IS NOT NULL AND c.permit_valid_until < ?1) AS permit_expired,
                    (SELECT count(*) FROM work_notices n WHERE n.contractor_id = c.id AND n.voided_at IS NULL) AS notices
             FROM contractors c
             WHERE ({include_voided} OR c.voided_at IS NULL)
             ORDER BY c.name COLLATE \"hu-x-icu\""
            ),
            vec![s(fmt_date(today()))],
        )
    }

    pub fn void_contractor(&self, se: &Session, id: &str, reason: Option<&str>) -> Result<()> {
        se.require(Permission::Operations)?;
        let reason = require_reason(reason)?;
        let tx = crate::db::tx(&self.conn)?;
        let n = tx.execute(
            "UPDATE contractors SET voided_at=?2, voided_by=?3, void_reason=?4, updated_at=?2, updated_by=?3 WHERE id=?1 AND voided_at IS NULL",
            params![id, now_ts(), se.user_name, reason],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("contractor", id));
        }
        audit::record_simple(&tx, se, "contractor", id, "void", Some(&reason))?;
        tx.commit()?;
        Ok(())
    }

    pub fn void_facility(&self, se: &Session, id: &str, reason: Option<&str>) -> Result<()> {
        se.require(Permission::Operations)?;
        let reason = require_reason(reason)?;
        let tx = crate::db::tx(&self.conn)?;
        let n = tx.execute(
            "UPDATE facilities SET voided_at=?2, voided_by=?3, void_reason=?4, updated_at=?2, updated_by=?3 WHERE id=?1 AND voided_at IS NULL",
            params![id, now_ts(), se.user_name, reason],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("facility", id));
        }
        audit::record_simple(&tx, se, "facility", id, "void", Some(&reason))?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- bookings (FR-SZE-01/02)

    /// Creates or moves a booking. A facility or a plot can hold one planned
    /// booking at a time; an overlap is refused with `booking_conflict`.
    pub fn save_booking(&self, se: &Session, b: &BookingInput) -> Result<Value> {
        let manager = se.can(Permission::Operations);
        if !manager {
            se.require(Permission::Calendar)?;
        }
        // an invited office changes only its own bookings
        if let (Some(id), false) = (&b.id, manager) {
            self.require_own_booking(se, id)?;
        }
        non_empty("cemeteryId", &b.cemetery_id)?;
        non_empty("title", &b.title)?;
        let start = parse_local("startsAt", &b.starts_at)?;
        let end = parse_local("endsAt", &b.ends_at)?;
        if end <= start {
            return Err(CoreError::validation("endsAt", "the end must be after the start").with("code", "ends_before_start"));
        }
        let container = one_of("container", &b.container, &["", "coffin", "urn"])?;
        let (start, end) = (fmt_local(start), fmt_local(end));
        let self_id = b.id.clone().unwrap_or_default();
        let facility = opt_s(&b.facility_id);
        let plot = opt_s(&b.plot_id);
        let clash = |col: &str, val: &Option<String>| -> Result<Option<(String, String)>> {
            let Some(v) = val else { return Ok(None) };
            Ok(self
                .conn
                .query_row(
                    &format!(
                        "SELECT title, starts_at FROM bookings WHERE {col} = ?1 AND status = 'planned' AND id <> ?2
                           AND starts_at < ?4 AND ends_at > ?3 ORDER BY starts_at LIMIT 1"
                    ),
                    params![v, self_id, start, end],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?)
        };
        for (col, val, what) in [("facility_id", &facility, "facility"), ("plot_id", &plot, "plot")] {
            if let Some((title, at)) = clash(col, val)? {
                return Err(CoreError::new("booking_conflict", format!("the {what} is already booked: {title} ({at})"))
                    .with("with", title)
                    .with("at", at.replace('T', " "))
                    .with("what", what));
            }
        }
        let ts = now_ts();
        let contractor = if manager { opt_s(&b.contractor_id) } else { None };
        let tx = crate::db::tx(&self.conn)?;
        let id = b.id.clone().unwrap_or_else(new_id);
        if b.id.is_some() {
            let n = tx.execute(
                "UPDATE bookings SET title=?2, starts_at=?3, ends_at=?4, facility_id=?5, plot_id=?6, contractor_id=?7, deceased_name=?8,
                        container=?9, notes=?10, updated_at=?11, updated_by=?12
                 WHERE id=?1 AND status = 'planned'",
                params![id, b.title.trim(), start, end, facility, plot, contractor, b.deceased_name.trim(), container, b.notes.trim(), ts, se.user_name],
            )?;
            if n == 0 {
                return Err(CoreError::new("booking_closed", "only planned bookings can be changed"));
            }
        } else {
            tx.execute(
                "INSERT INTO bookings (id, cemetery_id, title, starts_at, ends_at, facility_id, plot_id, contractor_id, deceased_name, container, notes,
                                       created_at, created_by, updated_at, updated_by)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?12,?13)",
                params![
                    id,
                    b.cemetery_id,
                    b.title.trim(),
                    start,
                    end,
                    facility,
                    plot,
                    contractor,
                    b.deceased_name.trim(),
                    container,
                    b.notes.trim(),
                    ts,
                    se.user_name
                ],
            )?;
            if b.dig_order && manager {
                if let Some(p) = &plot {
                    let details = match container.as_str() {
                        "urn" => "Urna",
                        "coffin" => "Koporsó",
                        _ => "",
                    };
                    self.insert_work_order(
                        &tx,
                        se,
                        &WorkOrderInput {
                            cemetery_id: b.cemetery_id.clone(),
                            kind: "grave_digging".into(),
                            title: format!("Sírásás: {}", b.title.trim()),
                            plot_id: Some(p.clone()),
                            booking_id: Some(id.clone()),
                            due_date: start[..10].to_string(),
                            details: details.into(),
                            ..Default::default()
                        },
                    )?;
                }
            }
        }
        audit::record(&tx, se, "booking", &id, if b.id.is_some() { "update" } else { "create" }, None::<&()>, Some(b), None)?;
        tx.commit()?;
        self.get_booking(&id)
    }

    fn require_own_booking(&self, se: &Session, id: &str) -> Result<()> {
        let owner: Option<Option<String>> = self.conn.query_row("SELECT created_by FROM bookings WHERE id = ?1", params![id], |r| r.get(0)).optional()?;
        match owner {
            None => Err(CoreError::not_found("booking", id)),
            Some(o) if o.as_deref() == Some(se.user_name.as_str()) => Ok(()),
            Some(_) => Err(CoreError::new("booking_not_own", "only the office that booked it can change this booking")),
        }
    }

    pub fn get_booking(&self, id: &str) -> Result<Value> {
        json_row(&self.conn, &format!("{BOOKING_SELECT} WHERE b.id = ?1"), vec![s(id)], "booking", id)
    }

    /// Bookings that touch the `from`..`to` day range (inclusive).
    pub fn list_bookings(&self, cemetery_id: &str, from: &str, to: &str) -> Result<Vec<Value>> {
        let from = fmt_date(parse_date(from)?);
        let to = fmt_date(parse_date(to)? + Duration::days(1));
        json_rows(
            &self.conn,
            &format!("{BOOKING_SELECT} WHERE b.cemetery_id = ?1 AND b.starts_at < ?3 AND b.ends_at > ?2 ORDER BY b.starts_at"),
            vec![s(cemetery_id), s(from), s(to)],
        )
    }

    /// done | cancelled (cancelling needs a reason); linking the recorded burial.
    pub fn set_booking_status(&self, se: &Session, id: &str, status: &str, reason: Option<&str>, event_id: Option<&str>) -> Result<Value> {
        let status = one_of("status", status, &["done", "cancelled"])?;
        let manager = se.can(Permission::Operations);
        if !manager && !(status == "cancelled" && se.can(Permission::Calendar)) && !(status == "done" && se.can(Permission::RecordEvents)) {
            return Err(CoreError::forbidden("operations"));
        }
        if !manager && status == "cancelled" {
            self.require_own_booking(se, id)?;
        }
        let reason = if status == "cancelled" { Some(require_reason(reason)?) } else { None };
        let tx = crate::db::tx(&self.conn)?;
        let n = tx.execute(
            "UPDATE bookings SET status=?2, event_id=COALESCE(?3, event_id), notes = CASE WHEN ?4::text IS NULL THEN notes ELSE trim(notes || ' Lemondva: ' || ?4::text) END,
                    updated_at=?5, updated_by=?6
             WHERE id=?1 AND status = 'planned'",
            params![id, status, event_id, reason, now_ts(), se.user_name],
        )?;
        if n == 0 {
            return Err(CoreError::new("booking_closed", "only planned bookings can be changed"));
        }
        if status == "cancelled" {
            tx.execute(
                "UPDATE work_orders SET status='cancelled', done_note='a szertartás elmaradt', updated_at=?2, updated_by=?3 WHERE booking_id=?1 AND status='open'",
                params![id, now_ts(), se.user_name],
            )?;
        }
        audit::record_simple(&tx, se, "booking", id, &status, reason.as_deref())?;
        tx.commit()?;
        self.get_booking(id)
    }

    // ---------------------------------------------------------------- work notices (FR-VAL-02)

    pub fn add_work_notice(&self, se: &Session, n: &WorkNoticeInput) -> Result<Value> {
        se.require(Permission::Operations)?;
        non_empty("cemeteryId", &n.cemetery_id)?;
        non_empty("contractorId", &n.contractor_id)?;
        let kind = one_of("kind", &n.kind, &["burial", "monument", "renovation", "demolition", "other"])?;
        let date = fmt_date(parse_date(&n.work_date)?);
        let (cname, expired): (String, bool) = self
            .conn
            .query_row(
                "SELECT name, (permit_valid_until IS NOT NULL AND permit_valid_until < ?2) FROM contractors WHERE id = ?1 AND voided_at IS NULL",
                params![n.contractor_id, date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
            .ok_or_else(|| CoreError::not_found("contractor", &n.contractor_id))?;
        if expired {
            return Err(CoreError::new("contractor_permit_expired", "the contractor's permit has expired").with("name", cname));
        }
        let id = new_id();
        let ident = match &opt_s(&n.plot_id) {
            Some(p) => self.conn.query_row("SELECT identifier FROM plots WHERE id = ?1", params![p], |r| r.get::<_, String>(0)).optional()?,
            None => None,
        };
        let tx = crate::db::tx(&self.conn)?;
        let charge_id = if n.charge {
            let rate: Option<String> = tx
                .query_row(
                    "SELECT id FROM fee_rates WHERE cemetery_id = ?1 AND fee_kind = 'maintenance' AND valid_from <= ?2
                       AND (valid_to IS NULL OR valid_to >= ?2) AND voided_at IS NULL ORDER BY valid_from DESC LIMIT 1",
                    params![n.cemetery_id, date],
                    |r| r.get(0),
                )
                .optional()?;
            let Some(rate) = rate else {
                return Err(CoreError::new("fee_rate_missing", "no maintenance contribution in the fee decree").with("feeKind", "maintenance"));
            };
            let desc = match &ident {
                Some(i) => format!("Temetőfenntartási hozzájárulás: {i}, {date}"),
                None => format!("Temetőfenntartási hozzájárulás, {date}"),
            };
            let c = self.add_charge(
                se,
                &ChargeInput {
                    cemetery_id: n.cemetery_id.clone(),
                    payer_name: Some(cname.clone()),
                    fee_rate_id: Some(rate),
                    description: Some(desc),
                    entity: Some("work".into()),
                    entity_id: Some(id.clone()),
                    ..Default::default()
                },
            )?;
            Some(c.id)
        } else {
            None
        };
        let ts = now_ts();
        tx.execute(
            "INSERT INTO work_notices (id, cemetery_id, contractor_id, plot_id, kind, work_date, description, charge_id, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?9,?10)",
            params![id, n.cemetery_id, n.contractor_id, opt_s(&n.plot_id), kind, date, n.description.trim(), charge_id, ts, se.user_name],
        )?;
        audit::record(&tx, se, "work_notice", &id, "create", None::<&()>, Some(n), None)?;
        tx.commit()?;
        Ok(self.list_work_notices(&n.cemetery_id, &date, &date)?.into_iter().find(|v| v["id"] == id).unwrap_or(Value::Null))
    }

    pub fn list_work_notices(&self, cemetery_id: &str, from: &str, to: &str) -> Result<Vec<Value>> {
        json_rows(
            &self.conn,
            "SELECT n.id, n.contractor_id, c.name AS contractor_name, n.plot_id, p.identifier AS plot_identifier, n.kind, n.work_date,
                    n.description, n.charge_id, ch.gross_huf AS charge_gross_huf, ch.status AS charge_status, n.created_at, n.created_by
             FROM work_notices n
             JOIN contractors c ON c.id = n.contractor_id
             LEFT JOIN plots p ON p.id = n.plot_id
             LEFT JOIN charges ch ON ch.id = n.charge_id
             WHERE n.cemetery_id = ?1 AND n.work_date BETWEEN ?2 AND ?3 AND n.voided_at IS NULL
             ORDER BY n.work_date DESC, n.created_at DESC",
            vec![s(cemetery_id), s(fmt_date(parse_date(from)?)), s(fmt_date(parse_date(to)?))],
        )
    }

    /// FR-VAL-03: a contractor's turnover by month, the open items that can go
    /// on one summary invoice, and what is invoiced but not yet paid.
    pub fn contractor_account(&self, se: &Session, contractor_id: &str) -> Result<Value> {
        se.require(Permission::Operations)?;
        let p = || vec![s(contractor_id)];
        let months = json_rows(
            &self.conn,
            "SELECT substr(n.work_date, 1, 7) AS month, count(*) AS notices, coalesce(sum(ch.gross_huf) FILTER (WHERE ch.status <> 'void'), 0) AS gross_huf
             FROM work_notices n LEFT JOIN charges ch ON ch.id = n.charge_id
             WHERE n.contractor_id = ?1 AND n.voided_at IS NULL AND n.work_date >= to_char(current_date - interval '12 months', 'YYYY-MM')
             GROUP BY 1 ORDER BY 1 DESC",
            p(),
        )?;
        let open = json_rows(
            &self.conn,
            "SELECT ch.id, ch.cemetery_id, ch.payer_name, ch.person_id, ch.description, ch.quantity, ch.unit, ch.unit_net_huf, ch.vat_rate,
                    ch.net_huf, ch.gross_huf, ch.status, ch.created_at
             FROM charges ch JOIN work_notices n ON n.charge_id = ch.id
             WHERE n.contractor_id = ?1 AND ch.status = 'open' ORDER BY ch.created_at",
            p(),
        )?;
        let (invoiced, unpaid): (i64, i64) = self.conn.query_row(
            "SELECT coalesce(sum(i.gross_huf), 0), coalesce(sum(i.gross_huf - i.paid_huf) FILTER (WHERE i.status IN ('issued', 'partially_paid', 'pending_asp')), 0)
             FROM invoices i WHERE i.kind = 'invoice' AND i.status <> 'storno'
               AND i.id IN (SELECT ch.invoice_id FROM charges ch JOIN work_notices n ON n.charge_id = ch.id WHERE n.contractor_id = ?1)",
            params![contractor_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let open_huf: i64 = open.iter().map(|c| c["grossHuf"].as_i64().unwrap_or(0)).sum();
        Ok(json!({ "months": months, "openCharges": open, "openHuf": open_huf, "invoicedHuf": invoiced, "unpaidHuf": unpaid }))
    }

    // ---------------------------------------------------------------- work orders (FR-SZE-03, FR-UZE-01)

    fn insert_work_order(&self, conn: &crate::sql::Connection, se: &Session, w: &WorkOrderInput) -> Result<String> {
        non_empty("cemeteryId", &w.cemetery_id)?;
        non_empty("title", &w.title)?;
        let kind = one_of("kind", &w.kind, &["grave_digging", "preparation", "maintenance", "cleaning", "tree", "repair", "other"])?;
        let rec = one_of("recurrence", if w.recurrence.is_empty() { "none" } else { &w.recurrence }, &["none", "weekly", "monthly", "yearly"])?;
        let due = fmt_date(parse_date(&w.due_date)?);
        let id = new_id();
        let ts = now_ts();
        conn.execute(
            "INSERT INTO work_orders (id, cemetery_id, kind, title, plot_id, booking_id, due_date, assignee, details, recurrence,
                                      created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?11,?12)",
            params![
                id,
                w.cemetery_id,
                kind,
                w.title.trim(),
                opt_s(&w.plot_id),
                opt_s(&w.booking_id),
                due,
                w.assignee.trim(),
                w.details.trim(),
                rec,
                ts,
                se.user_name
            ],
        )?;
        audit::record(conn, se, "work_order", &id, "create", None::<&()>, Some(w), None)?;
        Ok(id)
    }

    pub fn save_work_order(&self, se: &Session, w: &WorkOrderInput) -> Result<Value> {
        se.require(Permission::Operations)?;
        let tx = crate::db::tx(&self.conn)?;
        let id = match &w.id {
            None => self.insert_work_order(&tx, se, w)?,
            Some(id) => {
                non_empty("title", &w.title)?;
                let due = fmt_date(parse_date(&w.due_date)?);
                let rec = one_of("recurrence", if w.recurrence.is_empty() { "none" } else { &w.recurrence }, &["none", "weekly", "monthly", "yearly"])?;
                let n = tx.execute(
                    "UPDATE work_orders SET title=?2, plot_id=?3, due_date=?4, assignee=?5, details=?6, recurrence=?7, updated_at=?8, updated_by=?9
                     WHERE id=?1 AND status='open'",
                    params![id, w.title.trim(), opt_s(&w.plot_id), due, w.assignee.trim(), w.details.trim(), rec, now_ts(), se.user_name],
                )?;
                if n == 0 {
                    return Err(CoreError::new("work_order_closed", "only open work orders can be changed"));
                }
                audit::record(&tx, se, "work_order", id, "update", None::<&()>, Some(w), None)?;
                id.clone()
            }
        };
        tx.commit()?;
        self.get_work_order(&id)
    }

    pub fn get_work_order(&self, id: &str) -> Result<Value> {
        json_row(&self.conn, &format!("{ORDER_SELECT} WHERE w.id = ?1"), vec![s(id)], "work_order", id)
    }

    /// Open orders first (overdue on top), then the closed ones of the range.
    pub fn list_work_orders(&self, cemetery_id: &str, status: Option<&str>, assignee: Option<&str>) -> Result<Vec<Value>> {
        json_rows(
            &self.conn,
            &format!(
                "{ORDER_SELECT} WHERE w.cemetery_id = ?1 AND (?2::text IS NULL OR w.status = ?2::text) AND (?3::text IS NULL OR w.assignee = ?3::text)
                 ORDER BY (w.status = 'open') DESC, CASE WHEN w.status = 'open' THEN w.due_date END ASC, w.updated_at DESC LIMIT 500"
            ),
            vec![s(cemetery_id), status.map(s).unwrap_or(SqlValue::Null), assignee.map(s).unwrap_or(SqlValue::Null)],
        )
    }

    /// Marks the order done; a recurring order gets its next occurrence.
    pub fn complete_work_order(&self, se: &Session, id: &str, note: &str) -> Result<Value> {
        se.require(Permission::CompleteWork)?;
        let w = self.get_work_order(id)?;
        if w["status"] != "open" {
            return Err(CoreError::new("work_order_closed", "this work order is already closed"));
        }
        let ts = now_ts();
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE work_orders SET status='done', done_at=?2, done_by=?3, done_note=?4, updated_at=?2, updated_by=?3 WHERE id=?1",
            params![id, ts, se.user_name, note.trim()],
        )?;
        audit::record_simple(&tx, se, "work_order", id, "done", Some(note.trim()).filter(|n| !n.is_empty()))?;
        let mut next_id = None;
        let rec = w["recurrence"].as_str().unwrap_or("none");
        if let Some(next) = next_due(parse_date(w["dueDate"].as_str().unwrap_or_default())?, rec) {
            let nid = new_id();
            tx.execute(
                "INSERT INTO work_orders (id, cemetery_id, kind, title, plot_id, due_date, assignee, details, recurrence, created_at, created_by, updated_at, updated_by)
                 SELECT ?2, cemetery_id, kind, title, plot_id, ?3, assignee, details, recurrence, ?4, ?5, ?4, ?5 FROM work_orders WHERE id = ?1",
                params![id, nid, fmt_date(next), ts, se.user_name],
            )?;
            next_id = Some(nid);
        }
        tx.commit()?;
        Ok(json!({ "order": self.get_work_order(id)?, "nextId": next_id }))
    }

    pub fn cancel_work_order(&self, se: &Session, id: &str, reason: Option<&str>) -> Result<Value> {
        se.require(Permission::Operations)?;
        let reason = require_reason(reason)?;
        let tx = crate::db::tx(&self.conn)?;
        let n = tx.execute(
            "UPDATE work_orders SET status='cancelled', done_note=?2, updated_at=?3, updated_by=?4 WHERE id=?1 AND status='open'",
            params![id, reason, now_ts(), se.user_name],
        )?;
        if n == 0 {
            return Err(CoreError::new("work_order_closed", "this work order is already closed"));
        }
        audit::record_simple(&tx, se, "work_order", id, "cancel", Some(&reason))?;
        tx.commit()?;
        self.get_work_order(id)
    }

    // ---------------------------------------------------------------- cases (FR-ENG-01/02)

    fn insert_case(&self, conn: &crate::sql::Connection, se: &Session, c: &CaseInput) -> Result<String> {
        non_empty("cemeteryId", &c.cemetery_id)?;
        non_empty("subject", &c.subject)?;
        let kind = one_of("kind", &c.kind, CASE_KINDS)?;
        let received = date_or_today(&c.received_on)?;
        let deadline = match opt_s(&c.deadline) {
            Some(d) => Some(fmt_date(parse_date(&d)?)),
            None => None,
        };
        let applicant = match (&opt_s(&c.applicant_id), c.applicant_name.trim()) {
            (_, n) if !n.is_empty() => n.to_string(),
            (Some(p), _) => {
                conn.query_row("SELECT trim(name_prefix || ' ' || family_name || ' ' || given_name) FROM persons WHERE id = ?1", params![p], |r| r.get(0))?
            }
            _ => String::new(),
        };
        let year = &received[..4];
        // gap-free filing number: the sequence row lock serialises concurrent clerks
        conn.execute("INSERT INTO case_sequences (cemetery_id, year, next_value) VALUES (?1, ?2, 1) ON CONFLICT DO NOTHING", params![c.cemetery_id, year])?;
        let n: i64 = conn.query_row(
            "UPDATE case_sequences SET next_value = next_value + 1 WHERE cemetery_id = ?1 AND year = ?2 RETURNING next_value - 1",
            params![c.cemetery_id, year],
            |r| r.get(0),
        )?;
        let number = format!("{n}/{year}");
        let id = new_id();
        let ts = now_ts();
        conn.execute(
            "INSERT INTO cases (id, cemetery_id, case_number, kind, subject, applicant_id, applicant_name, plot_id, contractor_id, received_on, deadline,
                                created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?12,?13)",
            params![
                id,
                c.cemetery_id,
                number,
                kind,
                c.subject.trim(),
                opt_s(&c.applicant_id),
                applicant,
                opt_s(&c.plot_id),
                opt_s(&c.contractor_id),
                received,
                deadline,
                ts,
                se.user_name
            ],
        )?;
        audit::record(conn, se, "case", &id, "create", None::<&()>, Some(&json!({ "number": number, "input": c })), None)?;
        Ok(id)
    }

    pub fn create_case(&self, se: &Session, c: &CaseInput) -> Result<Value> {
        se.require(Permission::Operations)?;
        let tx = crate::db::tx(&self.conn)?;
        let id = self.insert_case(&tx, se, c)?;
        tx.commit()?;
        self.get_case(&id)
    }

    pub fn get_case(&self, id: &str) -> Result<Value> {
        json_row(&self.conn, &format!("{CASE_SELECT} WHERE k.id = ?1"), vec![s(id)], "case", id)
    }

    /// The case with its attachments and its history from the audit log.
    pub fn case_detail(&self, id: &str) -> Result<Value> {
        let case = self.get_case(id)?;
        let history = json_rows(
            &self.conn,
            "SELECT ts, user_name, action, new_value, reason FROM audit_log WHERE entity = 'case' AND entity_id = ?1 ORDER BY seq",
            vec![s(id)],
        )?;
        Ok(json!({ "case": case, "attachments": self.list_attachments("case", id)?, "history": history }))
    }

    pub fn list_cases(&self, cemetery_id: &str, status: Option<&str>, plot_id: Option<&str>) -> Result<Vec<Value>> {
        json_rows(
            &self.conn,
            &format!(
                "{CASE_SELECT} WHERE k.cemetery_id = ?1 AND k.voided_at IS NULL AND (?2::text IS NULL OR k.status = ?2::text)
                   AND (?3::text IS NULL OR k.plot_id = ?3::text)
                 ORDER BY (k.status IN ('received', 'missing_docs')) DESC, k.deadline ASC NULLS LAST, k.received_on DESC, k.created_at DESC LIMIT 500"
            ),
            vec![s(cemetery_id), status.map(s).unwrap_or(SqlValue::Null), plot_id.map(s).unwrap_or(SqlValue::Null)],
        )
    }

    /// Status, decision text or deadline. Approving or rejecting needs the decision text.
    pub fn update_case(&self, se: &Session, id: &str, u: &CaseUpdate) -> Result<Value> {
        se.require(Permission::Operations)?;
        let old = self.get_case(id)?;
        if old["status"] == "closed" {
            return Err(CoreError::new("case_closed", "this case is closed"));
        }
        let status = match &u.status {
            Some(st) => one_of("status", st, &["received", "missing_docs", "approved", "rejected", "closed"])?,
            None => old["status"].as_str().unwrap_or("received").to_string(),
        };
        let decision = u.decision.clone().map(|d| d.trim().to_string()).unwrap_or_else(|| old["decision"].as_str().unwrap_or_default().to_string());
        if matches!(status.as_str(), "approved" | "rejected") && decision.is_empty() {
            return Err(CoreError::validation("decision", "write the decision").with("code", "decision_required"));
        }
        let deadline = match &u.deadline {
            Some(d) if d.trim().is_empty() => None,
            Some(d) => Some(fmt_date(parse_date(d)?)),
            None => old["deadline"].as_str().map(String::from),
        };
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE cases SET status=?2, decision=?3, deadline=?4, updated_at=?5, updated_by=?6 WHERE id=?1",
            params![id, status, decision, deadline, now_ts(), se.user_name],
        )?;
        let new = json!({ "status": status, "decision": decision, "deadline": deadline });
        let before = json!({ "status": old["status"], "decision": old["decision"], "deadline": old["deadline"] });
        audit::record(&tx, se, "case", id, "update", Some(&before), Some(&new), None)?;
        tx.commit()?;
        self.get_case(id)
    }

    pub fn void_case(&self, se: &Session, id: &str, reason: Option<&str>) -> Result<()> {
        se.require(Permission::VoidEntries)?;
        let reason = require_reason(reason)?;
        let tx = crate::db::tx(&self.conn)?;
        let n = tx.execute(
            "UPDATE cases SET voided_at=?2, voided_by=?3, void_reason=?4, updated_at=?2, updated_by=?3 WHERE id=?1 AND voided_at IS NULL",
            params![id, now_ts(), se.user_name, reason],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("case", id));
        }
        audit::record_simple(&tx, se, "case", id, "void", Some(&reason))?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- attachments

    /// Stores a file for a case, a plot, a contractor or a work order.
    pub fn add_attachment(&self, se: &Session, entity: &str, entity_id: &str, file_name: &str, mime: &str, content_b64: &str) -> Result<Value> {
        se.require(if entity == "work_order" { Permission::CompleteWork } else { Permission::Operations })?;
        let entity = one_of("entity", entity, &["case", "plot", "contractor", "work_order", "booking"])?;
        non_empty("fileName", file_name)?;
        use base64::Engine;
        let data = base64::engine::general_purpose::STANDARD
            .decode(content_b64.trim())
            .map_err(|_| CoreError::validation("content", "not base64").with("code", "bad_file"))?;
        if data.is_empty() {
            return Err(CoreError::validation("content", "empty file").with("code", "bad_file"));
        }
        if data.len() > MAX_ATTACHMENT_BYTES {
            return Err(CoreError::new("file_too_large", "the file is too large").with("maxMb", MAX_ATTACHMENT_BYTES / 1024 / 1024));
        }
        let id = new_id();
        let ts = now_ts();
        let name: String = file_name.trim().chars().filter(|c| !c.is_control() && *c != '/' && *c != '\\').take(200).collect();
        let mime = if mime.trim().is_empty() { "application/octet-stream" } else { mime.trim() };
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "INSERT INTO attachments (id, entity, entity_id, file_name, mime_type, sha256, content, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?8,?9)",
            params![id, entity, entity_id, name, mime, sha256_hex(&data), data, ts, se.user_name],
        )?;
        audit::record_simple(&tx, se, &entity, entity_id, "attach", Some(&name))?;
        tx.commit()?;
        Ok(json!({ "id": id, "fileName": name, "mimeType": mime, "size": data.len() }))
    }

    pub fn list_attachments(&self, entity: &str, entity_id: &str) -> Result<Vec<Value>> {
        json_rows(
            &self.conn,
            "SELECT id, file_name, mime_type, sha256, length(content) AS size, created_at, created_by FROM attachments
             WHERE entity = ?1 AND entity_id = ?2 AND voided_at IS NULL ORDER BY created_at",
            vec![s(entity), s(entity_id)],
        )
    }

    pub fn get_attachment(&self, id: &str) -> Result<(String, String, Vec<u8>)> {
        self.conn
            .query_row("SELECT file_name, mime_type, content FROM attachments WHERE id = ?1 AND voided_at IS NULL", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .optional()?
            .ok_or_else(|| CoreError::not_found("attachment", id))
    }

    pub fn void_attachment(&self, se: &Session, id: &str, reason: Option<&str>) -> Result<()> {
        se.require(Permission::Operations)?;
        let reason = require_reason(reason)?;
        let tx = crate::db::tx(&self.conn)?;
        let n = tx.execute(
            "UPDATE attachments SET voided_at=?2, voided_by=?3, void_reason=?4, updated_at=?2, updated_by=?3 WHERE id=?1 AND voided_at IS NULL",
            params![id, now_ts(), se.user_name, reason],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("attachment", id));
        }
        audit::record_simple(&tx, se, "attachment", id, "void", Some(&reason))?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- inspections (FR-UZE-02/03)

    /// Records what was seen at a plot. A neglected or dangerous plot can open
    /// a neglect case for the holder with a deadline to put it right.
    pub fn record_condition(&self, se: &Session, c: &ConditionInput) -> Result<Value> {
        se.require(Permission::Operations)?;
        non_empty("plotId", &c.plot_id)?;
        let condition = one_of("condition", &c.condition, &["ok", "neglected", "dangerous", "to_liquidate"])?;
        let date = date_or_today(&c.observed_on)?;
        let (cem, ident): (String, String) = self
            .conn
            .query_row("SELECT cemetery_id, identifier FROM plots WHERE id = ?1", params![c.plot_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?
            .ok_or_else(|| CoreError::not_found("plot", &c.plot_id))?;
        let id = new_id();
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "INSERT INTO plot_conditions (id, plot_id, observed_on, condition, note, created_at, created_by) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![id, c.plot_id, date, condition, c.note.trim(), now_ts(), se.user_name],
        )?;
        audit::record_simple(&tx, se, "plot", &c.plot_id, "condition", Some(format!("{condition} {}", c.note.trim()).trim()))?;
        let mut case_id = None;
        if c.open_case && matches!(condition.as_str(), "neglected" | "dangerous") {
            let open: Option<String> = tx
                .query_row(
                    "SELECT id FROM cases WHERE plot_id = ?1 AND kind = 'neglect' AND status IN ('received', 'missing_docs') AND voided_at IS NULL LIMIT 1",
                    params![c.plot_id],
                    |r| r.get(0),
                )
                .optional()?;
            if open.is_none() {
                // the holder of the current lease is the addressee
                let holder: Option<(Option<String>, Option<String>)> = tx
                    .query_row(
                        "SELECT l.person_id, trim(p.name_prefix || ' ' || p.family_name || ' ' || p.given_name)
                         FROM leases l LEFT JOIN persons p ON p.id = l.person_id
                         WHERE l.plot_id = ?1 AND l.voided_at IS NULL AND l.status = 'active' ORDER BY l.expiry_date DESC LIMIT 1",
                        params![c.plot_id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                let (pid, pname) = holder.unwrap_or((None, None));
                let what = if condition == "dangerous" { "Balesetveszélyes sírhely" } else { "Gondozatlan sírhely" };
                let deadline = parse_date(&date)? + Duration::days(NEGLECT_DEADLINE_DAYS);
                case_id = Some(self.insert_case(
                    &tx,
                    se,
                    &CaseInput {
                        cemetery_id: cem,
                        kind: "neglect".into(),
                        subject: format!("{what}: {ident}"),
                        applicant_id: pid,
                        applicant_name: pname.unwrap_or_default(),
                        plot_id: Some(c.plot_id.clone()),
                        received_on: Some(date.clone()),
                        deadline: Some(fmt_date(deadline)),
                        ..Default::default()
                    },
                )?);
            } else {
                case_id = open;
            }
        }
        tx.commit()?;
        Ok(json!({ "id": id, "caseId": case_id }))
    }

    /// The latest observation per plot (optionally only problem plots).
    pub fn list_conditions(&self, cemetery_id: &str, problems_only: bool) -> Result<Vec<Value>> {
        json_rows(
            &self.conn,
            &format!(
                "SELECT * FROM (
                SELECT DISTINCT ON (pc.plot_id) pc.id, pc.plot_id, p.identifier AS plot_identifier, pc.observed_on, pc.condition, pc.note, pc.created_by,
                       (SELECT k.case_number FROM cases k WHERE k.plot_id = pc.plot_id AND k.kind = 'neglect' AND k.voided_at IS NULL
                         ORDER BY k.created_at DESC LIMIT 1) AS case_number
                FROM plot_conditions pc JOIN plots p ON p.id = pc.plot_id
                WHERE p.cemetery_id = ?1
                ORDER BY pc.plot_id, pc.observed_on DESC, pc.created_at DESC
             ) x WHERE (NOT {problems_only} OR x.condition <> 'ok')
             ORDER BY x.observed_on DESC, x.plot_identifier COLLATE \"hu-x-icu\""
            ),
            vec![s(cemetery_id)],
        )
    }

    pub fn plot_conditions(&self, plot_id: &str) -> Result<Vec<Value>> {
        json_rows(
            &self.conn,
            "SELECT id, observed_on, condition, note, created_by FROM plot_conditions WHERE plot_id = ?1 ORDER BY observed_on DESC, created_at DESC",
            vec![s(plot_id)],
        )
    }

    // ---------------------------------------------------------------- agenda

    /// Everything due in a day range: bookings, open work orders and case deadlines.
    pub fn agenda(&self, se: &Session, cemetery_id: &str, from: &str, to: &str) -> Result<Value> {
        let f = fmt_date(parse_date(from)?);
        let t = fmt_date(parse_date(to)?);
        if !se.can(Permission::Operations) {
            se.require(Permission::Calendar)?;
            return Ok(json!({ "bookings": self.list_bookings(cemetery_id, &f, &t)?, "workOrders": [], "caseDeadlines": [], "today": fmt_date(today()) }));
        }
        let orders = json_rows(
            &self.conn,
            &format!("{ORDER_SELECT} WHERE w.cemetery_id = ?1 AND w.status = 'open' AND w.due_date <= ?3 ORDER BY w.due_date"),
            vec![s(cemetery_id), s(&f), s(&t)],
        )?;
        let deadlines = json_rows(
            &self.conn,
            &format!(
                "{CASE_SELECT} WHERE k.cemetery_id = ?1 AND k.voided_at IS NULL AND k.status IN ('received', 'missing_docs')
                   AND k.deadline IS NOT NULL AND k.deadline <= ?3 ORDER BY k.deadline"
            ),
            vec![s(cemetery_id), s(&f), s(&t)],
        )?;
        Ok(json!({
            "bookings": self.list_bookings(cemetery_id, &f, &t)?,
            "workOrders": orders,
            "caseDeadlines": deadlines,
            "today": fmt_date(today()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::Role;

    fn setup() -> (Store, Session, String, String) {
        let store = Store::in_memory().unwrap();
        let se = Session::system();
        crate::sample::load_sample(&store, &se).unwrap();
        let cem: String = store.conn.query_row("SELECT id FROM cemeteries LIMIT 1", [], |r| r.get(0)).unwrap();
        let plot: String = store.conn.query_row("SELECT id FROM plots WHERE cemetery_id = ?1 LIMIT 1", params![cem], |r| r.get(0)).unwrap();
        (store, se, cem, plot)
    }

    #[test]
    fn booking_conflicts_and_dig_order() {
        let (store, se, cem, plot) = setup();
        let chapel = store.save_facility(&se, &FacilityInput { cemetery_id: cem.clone(), name: "Ravatalozó".into(), ..Default::default() }).unwrap();
        let chapel_id = chapel["id"].as_str().unwrap().to_string();
        let b = store
            .save_booking(
                &se,
                &BookingInput {
                    cemetery_id: cem.clone(),
                    title: "Kiss Péter temetése".into(),
                    starts_at: "2026-11-10T10:00".into(),
                    ends_at: "2026-11-10T11:00".into(),
                    facility_id: Some(chapel_id.clone()),
                    plot_id: Some(plot.clone()),
                    container: "coffin".into(),
                    dig_order: true,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(b["facilityName"], "Ravatalozó");
        // the same chapel at an overlapping time is refused
        let err = store
            .save_booking(
                &se,
                &BookingInput {
                    cemetery_id: cem.clone(),
                    title: "Másik".into(),
                    starts_at: "2026-11-10T10:30".into(),
                    ends_at: "2026-11-10T11:30".into(),
                    facility_id: Some(chapel_id.clone()),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert_eq!(err.code, "booking_conflict");
        // back to back is fine
        store
            .save_booking(
                &se,
                &BookingInput {
                    cemetery_id: cem.clone(),
                    title: "Következő".into(),
                    starts_at: "2026-11-10T11:00".into(),
                    ends_at: "2026-11-10T12:00".into(),
                    facility_id: Some(chapel_id),
                    ..Default::default()
                },
            )
            .unwrap();
        let orders = store.list_work_orders(&cem, Some("open"), None).unwrap();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0]["kind"], "grave_digging");
        assert_eq!(orders[0]["dueDate"], "2026-11-10");
        assert_eq!(store.list_bookings(&cem, "2026-11-10", "2026-11-10").unwrap().len(), 2);
        // cancelling the funeral cancels the digging
        let id = b["id"].as_str().unwrap();
        assert_eq!(store.set_booking_status(&se, id, "cancelled", None, None).unwrap_err().code, "reason_required");
        store.set_booking_status(&se, id, "cancelled", Some("a család kérésére"), None).unwrap();
        assert!(store.list_work_orders(&cem, Some("open"), None).unwrap().is_empty());
    }

    #[test]
    fn invited_office_books_and_changes_only_its_own() {
        let (store, se, cem, _) = setup();
        let chapel = store.save_facility(&se, &FacilityInput { cemetery_id: cem.clone(), name: "Ravatalozó".into(), ..Default::default() }).unwrap();
        let fid = chapel["id"].as_str().unwrap().to_string();
        let office = |name: &str| Session::for_role(None, name, name, Role::Undertaker, &[]);
        let (a, b) = (office("kegyelet@pelda.hu"), office("vegtisztesseg@pelda.hu"));
        let mk = |s: &Session, title: &str, h: u32| {
            store.save_booking(
                s,
                &BookingInput {
                    cemetery_id: cem.clone(),
                    title: title.into(),
                    starts_at: format!("2026-12-01T{h:02}:00"),
                    ends_at: format!("2026-12-01T{:02}:00", h + 1),
                    facility_id: Some(fid.clone()),
                    dig_order: true,
                    ..Default::default()
                },
            )
        };
        let own = mk(&a, "Kegyelet: Szabó Béla", 9).unwrap();
        // the other office sees it and cannot book over it
        assert_eq!(mk(&b, "Végtisztesség: Kiss Anna", 9).unwrap_err().code, "booking_conflict");
        let other = mk(&b, "Végtisztesség: Kiss Anna", 11).unwrap();
        let agenda = store.agenda(&b, &cem, "2026-12-01", "2026-12-01").unwrap();
        assert_eq!(agenda["bookings"].as_array().unwrap().len(), 2);
        assert!(agenda["workOrders"].as_array().unwrap().is_empty());
        // offices do not create work orders, and change only their own bookings
        assert!(store.list_work_orders(&cem, None, None).unwrap().is_empty());
        let id = own["id"].as_str().unwrap().to_string();
        assert_eq!(store.set_booking_status(&b, &id, "cancelled", Some("tévedés"), None).unwrap_err().code, "booking_not_own");
        store.set_booking_status(&a, &id, "cancelled", Some("a család kérte"), None).unwrap();
        assert_eq!(store.set_booking_status(&a, other["id"].as_str().unwrap(), "done", None, None).unwrap_err().code, "forbidden");
        assert!(store.save_facility(&a, &FacilityInput { cemetery_id: cem.clone(), name: "X".into(), ..Default::default() }).is_err());
    }

    #[test]
    fn recurring_work_order_and_worker_role() {
        let (store, se, cem, _) = setup();
        let w = store
            .save_work_order(
                &se,
                &WorkOrderInput {
                    cemetery_id: cem.clone(),
                    kind: "maintenance".into(),
                    title: "Fűnyírás az A parcellában".into(),
                    due_date: "2026-01-31".into(),
                    recurrence: "monthly".into(),
                    assignee: "Nagy Béla".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let worker = Session::for_role(None, "bela", "Nagy Béla", Role::Worker, &[]);
        assert!(store.save_work_order(&worker, &WorkOrderInput::default()).is_err());
        let r = store.complete_work_order(&worker, w["id"].as_str().unwrap(), "kész").unwrap();
        assert_eq!(r["order"]["status"], "done");
        let next = store.get_work_order(r["nextId"].as_str().unwrap()).unwrap();
        assert_eq!(next["dueDate"], "2026-02-28");
        assert_eq!(next["status"], "open");
        assert_eq!(store.list_work_orders(&cem, None, Some("Nagy Béla")).unwrap().len(), 2);
        assert_eq!(store.complete_work_order(&worker, w["id"].as_str().unwrap(), "").unwrap_err().code, "work_order_closed");
    }

    #[test]
    fn cases_numbers_attachments_and_decisions() {
        let (store, se, cem, plot) = setup();
        let mk = |subject: &str| {
            store
                .create_case(
                    &se,
                    &CaseInput {
                        cemetery_id: cem.clone(),
                        kind: "monument_permit".into(),
                        subject: subject.into(),
                        applicant_name: "Szabó Anna".into(),
                        plot_id: Some(plot.clone()),
                        received_on: Some("2026-03-02".into()),
                        ..Default::default()
                    },
                )
                .unwrap()
        };
        let a = mk("Síremlék állítása");
        let b = mk("Síremlék felújítása");
        assert_eq!(a["caseNumber"], "1/2026");
        assert_eq!(b["caseNumber"], "2/2026");
        let id = a["id"].as_str().unwrap();
        use base64::Engine;
        let content = base64::engine::general_purpose::STANDARD.encode(b"%PDF-1.4 kerelem");
        store.add_attachment(&se, "case", id, "kérelem.pdf", "application/pdf", &content).unwrap();
        let too_big = base64::engine::general_purpose::STANDARD.encode(vec![0u8; MAX_ATTACHMENT_BYTES + 1]);
        assert_eq!(store.add_attachment(&se, "case", id, "nagy.bin", "", &too_big).unwrap_err().code, "file_too_large");
        let err = store.update_case(&se, id, &CaseUpdate { status: Some("approved".into()), ..Default::default() }).unwrap_err();
        assert_eq!(err.code, "validation");
        let c = store
            .update_case(&se, id, &CaseUpdate { status: Some("approved".into()), decision: Some("Engedélyezve a terv szerint.".into()), ..Default::default() })
            .unwrap();
        assert_eq!(c["status"], "approved");
        assert_eq!(c["attachments"], 1);
        let d = store.case_detail(id).unwrap();
        assert_eq!(d["attachments"][0]["fileName"], "kérelem.pdf");
        assert_eq!(d["history"].as_array().unwrap().len(), 3); // create, attach, decision
        let (name, _, bytes) = store.get_attachment(d["attachments"][0]["id"].as_str().unwrap()).unwrap();
        assert_eq!(name, "kérelem.pdf");
        assert_eq!(bytes, b"%PDF-1.4 kerelem");
        // FR-ENG-04: the issued decision is kept with the case
        let dir = std::env::temp_dir().join(format!("case-doc-{}", new_id()));
        let f = crate::pdf::generate(&store, &se, "case_document", &json!({ "caseId": id }), &dir).unwrap();
        assert!(f.file_name.starts_with("hatarozat"), "{}", f.file_name);
        let atts = store.list_attachments("case", id).unwrap();
        assert_eq!(atts.len(), 2);
        let (_, mime, pdf) = store.get_attachment(atts[1]["id"].as_str().unwrap()).unwrap();
        assert_eq!(mime, "application/pdf");
        assert!(pdf.starts_with(b"%PDF"));
    }

    #[test]
    fn contractor_notice_charges_maintenance_fee() {
        let (store, se, cem, plot) = setup();
        let c = store
            .save_contractor(
                &se,
                &ContractorInput { name: "Kő Kft.".into(), kind: "stonemason".into(), permit_valid_until: Some("2030-12-31".into()), ..Default::default() },
            )
            .unwrap();
        let cid = c["id"].as_str().unwrap().to_string();
        let notice = WorkNoticeInput {
            cemetery_id: cem.clone(),
            contractor_id: cid.clone(),
            plot_id: Some(plot),
            kind: "monument".into(),
            work_date: "2026-04-15".into(),
            charge: true,
            ..Default::default()
        };
        assert_eq!(store.add_work_notice(&se, &notice).unwrap_err().code, "fee_rate_missing");
        store
            .add_fee_rate(
                &se,
                &FeeRate {
                    cemetery_id: cem.clone(),
                    fee_kind: "maintenance".into(),
                    name: "Temetőfenntartási hozzájárulás".into(),
                    amount_huf: 8_000,
                    vat_rate: "27".into(),
                    valid_from: "2020-01-01".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let n = store.add_work_notice(&se, &notice).unwrap();
        assert_eq!(n["contractorName"], "Kő Kft.");
        assert_eq!(n["chargeGrossHuf"], 10_160);
        let charges = store.list_charges(&cem, Some("open"), None).unwrap();
        assert!(charges.iter().any(|c| c.payer_name == "Kő Kft." && c.entity.as_deref() == Some("work")));
        // an expired permit stops new notices
        store
            .save_contractor(
                &se,
                &ContractorInput { id: Some(cid.clone()), name: "Kő Kft.".into(), permit_valid_until: Some("2026-01-01".into()), ..Default::default() },
            )
            .unwrap();
        assert_eq!(store.add_work_notice(&se, &WorkNoticeInput { charge: false, ..notice }).unwrap_err().code, "contractor_permit_expired");
        assert_eq!(store.list_contractors(false).unwrap()[0]["permitExpired"], true);
        let acc = store.contractor_account(&se, &cid).unwrap();
        assert_eq!(acc["openHuf"], 10_160);
        assert_eq!(acc["openCharges"].as_array().unwrap().len(), 1);
        assert_eq!(acc["months"][0]["month"], "2026-04");
    }

    #[test]
    fn inspection_opens_neglect_case_once() {
        let (store, se, cem, plot) = setup();
        let input = ConditionInput { plot_id: plot.clone(), condition: "neglected".into(), note: "gaz".into(), open_case: true, ..Default::default() };
        let r1 = store.record_condition(&se, &input).unwrap();
        let r2 = store.record_condition(&se, &input).unwrap();
        assert!(r1["caseId"].is_string());
        assert_eq!(r1["caseId"], r2["caseId"]);
        let case = store.get_case(r1["caseId"].as_str().unwrap()).unwrap();
        assert_eq!(case["kind"], "neglect");
        assert!(case["deadline"].is_string());
        let list = store.list_conditions(&cem, true).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["caseNumber"], case["caseNumber"]);
        store.record_condition(&se, &ConditionInput { plot_id: plot.clone(), condition: "ok".into(), ..Default::default() }).unwrap();
        assert!(store.list_conditions(&cem, true).unwrap().is_empty());
        assert_eq!(store.plot_conditions(&plot).unwrap().len(), 3);
        let agenda = store.agenda(&se, &cem, &fmt_date(today()), "2099-12-31").unwrap();
        assert_eq!(agenda["caseDeadlines"].as_array().unwrap().len(), 1);
    }
}
