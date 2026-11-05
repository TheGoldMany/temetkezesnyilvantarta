//! Fee rates (FR-DIJ-01 minimal), leases (FR-MEG-01..05, 07, 08), notices.

use chrono::NaiveDate;
use rusqlite::{params, OptionalExtension};

use super::*;
use crate::audit;
use crate::auth::{Permission, Session};
use crate::rules::{lease_expiry, pro_rata_refund, resale_allowed_from, PlotStatus};
use crate::settings;
use crate::util::{fmt_date, new_id, now_ts, parse_date, today};

impl Store {
    // ---------------------------------------------------------------- fee rates

    pub fn list_fee_rates(&self, cemetery_id: &str) -> Result<Vec<FeeRate>> {
        let mut st = self.conn.prepare(
            "SELECT id, cemetery_id, plot_type_id, fee_kind, amount_huf, valid_from, valid_to, voided_at FROM fee_rates
             WHERE cemetery_id=?1 ORDER BY fee_kind, plot_type_id, valid_from DESC",
        )?;
        let rows = st.query_map([cemetery_id], |r| {
            Ok(FeeRate {
                id: r.get(0)?,
                cemetery_id: r.get(1)?,
                plot_type_id: r.get(2)?,
                fee_kind: r.get(3)?,
                amount_huf: r.get(4)?,
                valid_from: r.get(5)?,
                valid_to: r.get(6)?,
                voided_at: r.get(7)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Adds a rate. Old rates are never changed so historic fees stay traceable;
    /// the previous open-ended rate of the same kind is closed the day before.
    pub fn add_fee_rate(&self, s: &Session, r: &FeeRate) -> Result<FeeRate> {
        s.require(Permission::ManageSettings)?;
        let from = parse_date(&r.valid_from)?;
        if r.amount_huf < 0 {
            return Err(CoreError::validation("amountHuf", "amount must not be negative"));
        }
        non_empty("feeKind", &r.fee_kind)?;
        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        tx.execute(
            "UPDATE fee_rates SET valid_to=?5, updated_at=?6, updated_by=?7
             WHERE cemetery_id=?1 AND fee_kind=?2 AND coalesce(plot_type_id,'')=coalesce(?3,'') AND valid_to IS NULL AND voided_at IS NULL AND valid_from < ?4",
            params![r.cemetery_id, r.fee_kind, r.plot_type_id, r.valid_from, fmt_date(from - chrono::Duration::days(1)), ts, s.user_name],
        )?;
        let id = new_id();
        tx.execute(
            "INSERT INTO fee_rates (id, cemetery_id, plot_type_id, fee_kind, amount_huf, valid_from, valid_to, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?8,?9)",
            params![id, r.cemetery_id, r.plot_type_id, r.fee_kind, r.amount_huf, r.valid_from, opt_s(&r.valid_to), ts, s.user_name],
        )?;
        let out = FeeRate { id: id.clone(), ..r.clone() };
        audit::record(&tx, s, "fee_rate", &id, "create", None::<&()>, Some(&out), None)?;
        tx.commit()?;
        Ok(out)
    }

    /// Rate valid on `date` for a plot type (falls back to a type-independent rate).
    pub fn current_rate(&self, cemetery_id: &str, plot_type_id: &str, kind: &str, date: NaiveDate) -> Result<Option<FeeRate>> {
        let d = fmt_date(date);
        Ok(self
            .conn
            .query_row(
                "SELECT id, cemetery_id, plot_type_id, fee_kind, amount_huf, valid_from, valid_to, voided_at FROM fee_rates
                 WHERE cemetery_id=?1 AND fee_kind=?3 AND (plot_type_id=?2 OR plot_type_id IS NULL) AND voided_at IS NULL
                   AND valid_from <= ?4 AND (valid_to IS NULL OR valid_to >= ?4)
                 ORDER BY plot_type_id IS NULL, valid_from DESC LIMIT 1",
                params![cemetery_id, plot_type_id, kind, d],
                |r| {
                    Ok(FeeRate {
                        id: r.get(0)?,
                        cemetery_id: r.get(1)?,
                        plot_type_id: r.get(2)?,
                        fee_kind: r.get(3)?,
                        amount_huf: r.get(4)?,
                        valid_from: r.get(5)?,
                        valid_to: r.get(6)?,
                        voided_at: r.get(7)?,
                    })
                },
            )
            .optional()?)
    }

    // ---------------------------------------------------------------- leases

    pub fn leases_for_plot(&self, plot_id: &str) -> Result<Vec<Lease>> {
        let mut st = self.conn.prepare(
            "SELECT l.*, CASE WHEN p.id IS NULL THEN NULL ELSE trim(p.name_prefix||' '||p.family_name||' '||p.given_name) END AS person_name
             FROM leases l LEFT JOIN persons p ON p.id=l.person_id WHERE l.plot_id=?1 ORDER BY l.start_date, l.created_at",
        )?;
        let rows = st.query_map([plot_id], map_lease)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn get_lease(&self, id: &str) -> Result<Lease> {
        self.conn
            .query_row(
                "SELECT l.*, CASE WHEN p.id IS NULL THEN NULL ELSE trim(p.name_prefix||' '||p.family_name||' '||p.given_name) END AS person_name
                 FROM leases l LEFT JOIN persons p ON p.id=l.person_id WHERE l.id=?1",
                [id],
                map_lease,
            )
            .optional()?
            .ok_or_else(|| CoreError::not_found("lease", id))
    }

    pub(crate) fn insert_lease_pub(
        &self,
        tx: &rusqlite::Connection,
        s: &Session,
        plot_id: &str,
        person_id: Option<&str>,
        kind: &str,
        start: &str,
        duration: i32,
        expiry: &str,
        fee: i64,
        fee_rate_id: Option<&str>,
        exempt: bool,
        related_event_id: Option<&str>,
        notes: &str,
    ) -> Result<Lease> {
        self.insert_lease(tx, s, plot_id, person_id, kind, start, duration, expiry, fee, fee_rate_id, exempt, related_event_id, notes)
    }

    /// Inserts a lease and supersedes the currently active one (invariant: one active lease per plot).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert_lease(
        &self,
        tx: &rusqlite::Connection,
        s: &Session,
        plot_id: &str,
        person_id: Option<&str>,
        kind: &str,
        start: &str,
        duration: i32,
        expiry: &str,
        fee: i64,
        fee_rate_id: Option<&str>,
        exempt: bool,
        related_event_id: Option<&str>,
        notes: &str,
    ) -> Result<Lease> {
        let ts = now_ts();
        let prev: Vec<String> = {
            let mut st = tx.prepare("SELECT id FROM leases WHERE plot_id=?1 AND status='active' AND voided_at IS NULL")?;
            let rows = st.query_map([plot_id], |r| r.get(0))?;
            rows.collect::<std::result::Result<_, _>>()?
        };
        for pid in &prev {
            tx.execute("UPDATE leases SET status='superseded', updated_at=?2, updated_by=?3 WHERE id=?1", params![pid, ts, s.user_name])?;
        }
        let id = new_id();
        tx.execute(
            "INSERT INTO leases (id, plot_id, person_id, kind, start_date, duration_years, expiry_date, fee_huf, fee_rate_id, fee_exempt, status, related_event_id, notes, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11,?12,?13,?14,?13,?14)",
            params![id, plot_id, person_id, kind, start, duration, expiry, fee, fee_rate_id, exempt, related_event_id, notes, ts, s.user_name],
        )?;
        let lease = tx.query_row(
            "SELECT l.*, NULL AS person_name FROM leases l WHERE l.id=?1",
            [&id],
            map_lease,
        )?;
        audit::record(tx, s, "lease", &id, "create", None::<&()>, Some(&serde_json::json!({ "lease": lease, "superseded": prev })), None)?;
        Ok(lease)
    }

    /// Proposal for a stand-alone purchase, pre-purchase or renewal (FR-MEG-01..03).
    pub fn propose_lease(&self, plot_id: &str, kind: &str, start: Option<&str>) -> Result<LeaseProposal> {
        let plot = self.get_plot(plot_id)?;
        let ptype = self.get_plot_type(&plot.plot_type_id)?;
        let exempt = plot.protected && plot.protection_kind.as_deref() == Some("national");
        let active = self.leases_for_plot(plot_id)?.into_iter().find(|l| l.status == "active" && l.voided_at.is_none());
        let start = match (start, kind, &active) {
            (Some(s), _, _) => parse_date(s)?,
            (None, "renewal", Some(a)) => {
                let exp = parse_date(&a.expiry_date)?;
                if exp >= today() { exp } else { today() }
            }
            _ => today(),
        };
        let rate_kind = if kind == "renewal" { "renewal" } else { "purchase" };
        let rate = match self.current_rate(&plot.cemetery_id, &plot.plot_type_id, rate_kind, start)? {
            Some(r) => Some(r),
            None if kind == "renewal" => self.current_rate(&plot.cemetery_id, &plot.plot_type_id, "purchase", start)?,
            None => None,
        };
        let duration = ptype.default_lease_years.max(1);
        Ok(LeaseProposal {
            kind: kind.into(),
            start_date: fmt_date(start),
            duration_years: duration,
            expiry_date: fmt_date(lease_expiry(start, duration)),
            fee_huf: if exempt { 0 } else { rate.as_ref().map(|r| r.amount_huf).unwrap_or(0) },
            fee_rate_id: rate.map(|r| r.id),
            fee_exempt: exempt,
            extension_days: 0,
        })
    }

    /// Stand-alone purchase (megváltás), pre-purchase (előre megváltás, FR-MEG-02) or renewal (újraváltás).
    pub fn create_lease(&self, s: &Session, input: &NewLeaseInput) -> Result<Lease> {
        s.require(Permission::RecordLeases)?;
        let kind = input.kind.as_str();
        if !["purchase", "prepurchase", "renewal"].contains(&kind) {
            return Err(CoreError::validation("kind", "invalid lease kind"));
        }
        let plot = self.get_plot(&input.plot_id)?;
        let status = self.plot_status(&plot)?;
        let active = self.leases_for_plot(&plot.id)?.into_iter().find(|l| l.status == "active" && l.voided_at.is_none());
        match kind {
            "purchase" | "prepurchase" => {
                if !matches!(status, PlotStatus::Free | PlotStatus::Resellable) {
                    return Err(CoreError::new("plot_not_free", "only free or resellable plots can be purchased").with("status", status.as_str()));
                }
                if kind == "prepurchase" {
                    let occ = self.occupants(&plot.id)?;
                    if !occ.is_empty() && status != PlotStatus::Resellable {
                        return Err(CoreError::new("plot_not_free", "pre-purchase needs an empty plot"));
                    }
                }
                if input.person.is_none() {
                    return Err(CoreError::validation("person", "holder is required").with("code", "holder_required"));
                }
            }
            _ => {
                if active.is_none() {
                    return Err(CoreError::new("no_active_lease", "renewal needs an existing lease"));
                }
                if status == PlotStatus::Resellable {
                    return Err(CoreError::new("plot_not_available", "plot was declared resellable"));
                }
            }
        }
        let proposal = self.propose_lease(&plot.id, kind, if input.start_date.is_empty() { None } else { Some(&input.start_date) })?;
        let duration = input.duration_years.unwrap_or(proposal.duration_years);
        if duration < 1 || duration > 200 {
            return Err(CoreError::validation("durationYears", "invalid duration"));
        }
        let start = parse_date(&proposal.start_date)?;
        let expiry = fmt_date(lease_expiry(start, duration));
        let fee = if proposal.fee_exempt { 0 } else { input.fee_huf.unwrap_or(proposal.fee_huf) };
        if fee < 0 {
            return Err(CoreError::validation("feeHuf", "fee must not be negative"));
        }

        let tx = crate::db::tx(&self.conn)?;
        let person_id = match &input.person {
            Some(p) => Some(self.resolve_person(&tx, s, p)?),
            None => self.primary_holder_id(&tx, &plot.id)?,
        };
        if kind != "renewal" {
            // resale or first purchase: the buyer becomes the primary holder
            let current = self.primary_holder_id(&tx, &plot.id)?;
            if current.as_deref() != person_id.as_deref() {
                if current.is_some() {
                    tx.execute(
                        "UPDATE plot_holders SET end_date=?2, updated_at=?3, updated_by=?4 WHERE plot_id=?1 AND role='primary' AND end_date IS NULL AND voided_at IS NULL",
                        params![plot.id, proposal.start_date, now_ts(), s.user_name],
                    )?;
                }
                if let Some(pid) = &person_id {
                    self.insert_holder(&tx, s, &plot.id, pid, "primary", "", &proposal.start_date, if status == PlotStatus::Resellable { "újraértékesítés" } else { "megváltás" }, "")?;
                }
            }
        }
        let lease = self.insert_lease(&tx, s, &plot.id, person_id.as_deref(), kind, &proposal.start_date, duration, &expiry, fee, proposal.fee_rate_id.as_deref(), proposal.fee_exempt, None, &input.notes)?;
        if status == PlotStatus::Resellable {
            // resale completed: the plot leaves the "újraértékesíthető" state
            tx.execute("UPDATE plots SET resale_ready_at=NULL, updated_at=?2, updated_by=?3 WHERE id=?1", params![plot.id, now_ts(), s.user_name])?;
        }
        tx.commit()?;
        self.get_lease(&lease.id)
    }

    /// Renunciation with pro-rata refund (FR-MEG-04).
    pub fn cancel_lease(&self, s: &Session, lease_id: &str, cancel_date: &str, reason: Option<&str>) -> Result<Lease> {
        s.require(Permission::RecordLeases)?;
        let reason = require_reason(reason)?;
        let l = self.get_lease(lease_id)?;
        if l.status != "active" || l.voided_at.is_some() {
            return Err(CoreError::new("lease_not_active", "lease is not active"));
        }
        let occ = self.occupants(&l.plot_id)?;
        let cd = parse_date(cancel_date)?;
        // the original purchase fee is the refund base
        let refund = if occ.is_empty() { pro_rata_refund(parse_date(&l.start_date)?, parse_date(&l.expiry_date)?, cd, l.fee_huf) } else { 0 };
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE leases SET status='cancelled', cancel_date=?2, refund_huf=?3, updated_at=?4, updated_by=?5 WHERE id=?1",
            params![lease_id, cancel_date, refund, now_ts(), s.user_name],
        )?;
        tx.execute(
            "UPDATE plot_holders SET end_date=?2, updated_at=?3, updated_by=?4 WHERE plot_id=?1 AND role='primary' AND end_date IS NULL AND voided_at IS NULL",
            params![l.plot_id, cancel_date, now_ts(), s.user_name],
        )?;
        audit::record(&tx, s, "lease", lease_id, "cancel", Some(&l), Some(&serde_json::json!({"cancelDate": cancel_date, "refundHuf": refund})), Some(&reason))?;
        tx.commit()?;
        self.get_lease(lease_id)
    }

    pub fn void_lease(&self, s: &Session, lease_id: &str, reason: Option<&str>) -> Result<()> {
        s.require(Permission::VoidEntries)?;
        let reason = require_reason(reason)?;
        let tx = crate::db::tx(&self.conn)?;
        self.void_lease_tx(&tx, s, lease_id, &reason)?;
        tx.commit()?;
        Ok(())
    }

    /// Voids a lease; if it was the active one, the latest superseded lease becomes active again.
    pub(crate) fn void_lease_tx(&self, tx: &rusqlite::Connection, s: &Session, lease_id: &str, reason: &str) -> Result<()> {
        let l = tx
            .query_row("SELECT l.*, NULL AS person_name FROM leases l WHERE l.id=?1", [lease_id], map_lease)
            .optional()?
            .ok_or_else(|| CoreError::not_found("lease", lease_id))?;
        if l.voided_at.is_some() {
            return Ok(());
        }
        let ts = now_ts();
        tx.execute(
            "UPDATE leases SET voided_at=?2, voided_by=?3, void_reason=?4, updated_at=?2 WHERE id=?1",
            params![lease_id, ts, s.user_name, reason],
        )?;
        if l.status == "active" {
            tx.execute(
                "UPDATE leases SET status='active', updated_at=?2 WHERE id = (
                    SELECT id FROM leases WHERE plot_id=?1 AND status='superseded' AND voided_at IS NULL ORDER BY expiry_date DESC, created_at DESC LIMIT 1)",
                params![l.plot_id, ts],
            )?;
        }
        audit::record(tx, s, "lease", lease_id, "void", Some(&l), None::<&()>, Some(reason))?;
        Ok(())
    }

    // ---------------------------------------------------------------- expiries (FR-MEG-05..07)

    pub fn expiries(&self, s: &Session, f: &ExpiryFilter) -> Result<Vec<ExpiryRow>> {
        let t = today();
        let until = fmt_date(t + chrono::Duration::days(f.within_days.max(0)));
        let from = if f.include_expired { "0000-01-01".to_string() } else { fmt_date(t) };
        let mut st = self.conn.prepare(&format!(
            "SELECT p.id, p.identifier, p.cemetery_id, c.name, sec.code, ty.name, l.id, l.start_date, l.expiry_date,
                    h.person_id, trim(pe.name_prefix||' '||pe.family_name||' '||pe.given_name), pe.address,
                    (SELECT group_concat(trim(d.family_name||' '||d.given_name), ', ') FROM interment_events e JOIN deceased d ON d.id=e.deceased_id
                       WHERE e.plot_id=p.id AND {ACTIVE_PLACEMENT_SQL}) AS occ_names,
                    (SELECT count(*) FROM interment_events e WHERE e.plot_id=p.id AND {ACTIVE_PLACEMENT_SQL}) AS occ,
                    (SELECT max(notice_date) FROM notices n WHERE n.plot_id=p.id AND n.voided_at IS NULL) AS last_notice,
                    p.status_override, p.unusable, p.resale_ready_at
             FROM leases l
             JOIN plots p ON p.id=l.plot_id
             JOIN cemeteries c ON c.id=p.cemetery_id
             JOIN sections sec ON sec.id=p.section_id
             JOIN plot_types ty ON ty.id=p.plot_type_id
             LEFT JOIN plot_holders h ON h.plot_id=p.id AND h.role='primary' AND h.end_date IS NULL AND h.voided_at IS NULL
             LEFT JOIN persons pe ON pe.id=h.person_id
             WHERE l.status='active' AND l.voided_at IS NULL AND p.voided_at IS NULL
               AND NOT (p.protected=1 AND p.protection_kind='national')
               AND l.expiry_date <= ?1 AND l.expiry_date >= ?2
               AND (?3 IS NULL OR p.cemetery_id=?3) AND (?4 IS NULL OR p.section_id=?4)
             ORDER BY l.expiry_date, p.identifier"
        ))?;
        let grace_default = settings::get_i64(&self.conn, "rules.grace_period_days", None, 180)?;
        let can_personal = s.can(Permission::ViewPersonal);
        let mut out = vec![];
        let mut rows = st.query(params![until, from, f.cemetery_id, f.section_id])?;
        while let Some(r) = rows.next()? {
            let start = parse_date(&r.get::<_, String>(7)?)?;
            let exp = parse_date(&r.get::<_, String>(8)?)?;
            let last_notice: Option<String> = r.get(14)?;
            let ln = last_notice.as_deref().map(parse_date).transpose()?;
            let inputs = crate::rules::StatusInputs {
                status_override: r.get::<_, Option<String>>(15)?.as_deref().and_then(PlotStatus::parse),
                unusable: r.get(16)?,
                protected: false,
                occupants: r.get(13)?,
                lease_start: Some(start),
                lease_expiry: Some(exp),
                last_notice: ln,
                resale_ready_at: r.get::<_, Option<String>>(17)?.map(|s| parse_date(&s[..10.min(s.len())])).transpose()?,
            };
            let status = crate::rules::compute_status(&inputs, t);
            let notified_for_lease = ln.map(|n| n >= start).unwrap_or(false);
            if f.only_not_notified && notified_for_lease {
                continue;
            }
            out.push(ExpiryRow {
                plot_id: r.get(0)?,
                identifier: r.get(1)?,
                cemetery_id: r.get(2)?,
                cemetery_name: r.get(3)?,
                section_code: r.get(4)?,
                type_name: r.get(5)?,
                status,
                lease_id: r.get(6)?,
                lease_expiry: fmt_date(exp),
                days_left: (exp - t).num_days(),
                holder_id: r.get(9)?,
                holder_name: r.get(10)?,
                holder_address: if can_personal { r.get(11)? } else { None },
                occupant_names: r.get(12)?,
                last_notice: if notified_for_lease { last_notice } else { None },
                resale_allowed_from: if notified_for_lease { resale_allowed_from(exp, ln, grace_default).map(fmt_date) } else { None },
            });
        }
        Ok(out)
    }

    /// UC-03 step 3: records that notices were sent (letter, label, board, e-mail).
    pub fn record_notices(&self, s: &Session, plot_ids: &[String], kind: &str, notice_date: &str, result: &str) -> Result<usize> {
        s.require(Permission::RecordLeases)?;
        if !["letter", "email", "label", "board"].contains(&kind) {
            return Err(CoreError::validation("kind", "invalid notice kind"));
        }
        parse_date(notice_date)?;
        let tx = crate::db::tx(&self.conn)?;
        let ts = now_ts();
        let mut n = 0;
        for pid in plot_ids {
            let lease_id: Option<String> = tx
                .query_row("SELECT id FROM leases WHERE plot_id=?1 AND status='active' AND voided_at IS NULL", [pid], |r| r.get(0))
                .optional()?;
            let id = new_id();
            tx.execute(
                "INSERT INTO notices (id, plot_id, lease_id, kind, notice_date, result, created_at, created_by, updated_at, updated_by)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?7,?8)",
                params![id, pid, lease_id, kind, notice_date, result, ts, s.user_name],
            )?;
            audit::record(&tx, s, "notice", &id, "create", None::<&()>, Some(&serde_json::json!({"plotId": pid, "kind": kind, "date": notice_date})), None)?;
            n += 1;
        }
        tx.commit()?;
        Ok(n)
    }

    /// FR-MEG-07 / UC-03 step 4: after the grace period the plot may be declared resellable,
    /// with explicit confirmation (the reason doubles as the confirmation text).
    pub fn mark_resellable(&self, s: &Session, plot_id: &str, reason: Option<&str>) -> Result<PlotStatus> {
        s.require(Permission::RecordLeases)?;
        let reason = require_reason(reason)?;
        let plot = self.get_plot(plot_id)?;
        let inputs = self.status_inputs(&plot)?;
        let status = crate::rules::compute_status(&inputs, today());
        if status != PlotStatus::Notified {
            return Err(CoreError::new("not_notified", "the plot must be expired and notified").with("status", status.as_str()));
        }
        let exp = inputs.lease_expiry.ok_or_else(|| CoreError::new("no_active_lease", "no lease"))?;
        let grace = settings::get_i64(&self.conn, "rules.grace_period_days", Some(&plot.cemetery_id), 180)?;
        let allowed = resale_allowed_from(exp, inputs.last_notice, grace).ok_or_else(|| CoreError::new("not_notified", "no notice"))?;
        if today() < allowed || today() <= exp {
            return Err(CoreError::new("grace_period_running", "grace period has not elapsed").with("from", fmt_date(allowed.max(exp + chrono::Duration::days(1)))));
        }
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE plots SET resale_ready_at=?2, updated_at=?3, updated_by=?4 WHERE id=?1",
            params![plot_id, fmt_date(today()), now_ts(), s.user_name],
        )?;
        audit::record(&tx, s, "plot", plot_id, "mark_resellable", None::<&()>, Some(&serde_json::json!({"date": fmt_date(today())})), Some(&reason))?;
        tx.commit()?;
        self.plot_status(&self.get_plot(plot_id)?)
    }
}
