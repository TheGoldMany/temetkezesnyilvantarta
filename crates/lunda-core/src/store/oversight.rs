//! Reports for the maintainer and the public (W12, docs/FRS-UZEMELTETES.md):
//! the annual report (FR-FEN-01), the capacity forecast (FR-FEN-02), e-mail
//! notices to holders and debtors (FR-UGY-01) and the public grave search
//! (FR-UGY-02). Sending the e-mails is the server's job; the core prepares
//! the messages and records them once sent.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::operations::json_rows;
use super::*;
use crate::auth::{Permission, Session};
use crate::settings;
use crate::sql::{params, OptionalExtension, Value as SqlValue};
use crate::util::{fmt_date, parse_date, today};

/// Years of burials the capacity forecast averages over.
pub const FORECAST_YEARS: i32 = 5;
pub const PUBLIC_SEARCH_LIMIT: i64 = 20;

/// A message for the server to send.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutMail {
    pub to: String,
    pub reply_to: Option<String>,
    pub subject: String,
    pub body: String,
    /// plot for expiry notices, invoice for reminders
    pub ref_id: String,
}

fn txt(v: impl Into<String>) -> SqlValue {
    SqlValue::Text(v.into())
}

fn cem_arg(c: Option<&str>) -> SqlValue {
    c.map(txt).unwrap_or(SqlValue::Null)
}

impl Store {
    // ---------------------------------------------------------------- annual report (FR-FEN-01)

    /// Everything the maintainer asks for at the end of the year, for one
    /// cemetery or for all of them.
    pub fn annual_report(&self, s: &Session, cemetery_id: Option<&str>, year: i32) -> Result<Value> {
        s.require(Permission::Print)?;
        if !(1900..=2200).contains(&year) {
            return Err(CoreError::validation("year", "invalid year"));
        }
        let y = year.to_string();
        let c = || vec![cem_arg(cemetery_id), txt(&y)];
        let burials = json_rows(
            &self.conn,
            "SELECT event_type, count(*) AS n FROM interment_events
             WHERE voided_at IS NULL AND (?1::text IS NULL OR cemetery_id = ?1::text) AND substr(event_date, 1, 4) = ?2
             GROUP BY event_type ORDER BY n DESC",
            c(),
        )?;
        let leases = json_rows(
            &self.conn,
            "SELECT l.kind, count(*) AS n, coalesce(sum(l.fee_huf), 0) AS fee_huf FROM leases l JOIN plots p ON p.id = l.plot_id
             WHERE l.voided_at IS NULL AND l.status <> 'cancelled' AND (?1::text IS NULL OR p.cemetery_id = ?1::text) AND substr(l.start_date, 1, 4) = ?2
             GROUP BY l.kind ORDER BY n DESC",
            c(),
        )?;
        // revenue by fee kind from the invoices of the year (storno invoices carry negative amounts)
        let revenue = json_rows(
            &self.conn,
            "SELECT coalesce(f.fee_kind, 'other') AS fee_kind, coalesce(nullif(il.ledger_code, ''), '') AS ledger_code,
                    sum(il.net_huf) AS net_huf, sum(il.vat_huf) AS vat_huf, sum(il.gross_huf) AS gross_huf
             FROM invoice_lines il
             JOIN invoices i ON i.id = il.invoice_id
             LEFT JOIN charges ch ON ch.id = il.charge_id
             LEFT JOIN fee_rates f ON f.id = ch.fee_rate_id
             WHERE i.kind IN ('invoice', 'storno') AND (?1::text IS NULL OR i.cemetery_id = ?1::text) AND substr(i.issue_date, 1, 4) = ?2
             GROUP BY 1, 2 ORDER BY gross_huf DESC",
            c(),
        )?;
        let reversed = self.conn.query_row(
            "SELECT coalesce(sum(gross_huf), 0) FROM invoices WHERE kind = 'storno' AND (?1::text IS NULL OR cemetery_id = ?1::text) AND substr(issue_date, 1, 4) = ?2",
            params![cemetery_id, y],
            |r| r.get::<_, i64>(0),
        )?;
        let paid = self.conn.query_row(
            "SELECT coalesce(sum(amount_huf), 0) FROM payments WHERE voided_at IS NULL AND (?1::text IS NULL OR cemetery_id = ?1::text) AND substr(paid_on, 1, 4) = ?2",
            params![cemetery_id, y],
            |r| r.get::<_, i64>(0),
        )?;
        let (open_n, open_huf, overdue_huf): (i64, i64, i64) = self.conn.query_row(
            "SELECT count(*), coalesce(sum(gross_huf - paid_huf), 0), coalesce(sum(gross_huf - paid_huf) FILTER (WHERE due_date < ?2), 0)
             FROM invoices WHERE kind = 'invoice' AND status IN ('issued', 'partially_paid', 'pending_asp') AND (?1::text IS NULL OR cemetery_id = ?1::text)",
            params![cemetery_id, fmt_date(today())],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let (cash_in, cash_out): (i64, i64) = self.conn.query_row(
            "SELECT coalesce(sum(amount_huf) FILTER (WHERE kind = 'in'), 0), coalesce(sum(amount_huf) FILTER (WHERE kind = 'out'), 0)
             FROM cash_entries WHERE voided_at IS NULL AND (?1::text IS NULL OR cemetery_id = ?1::text) AND substr(entry_date, 1, 4) = ?2",
            params![cemetery_id, y],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let work = json_rows(
            &self.conn,
            "SELECT kind, count(*) AS n FROM work_orders
             WHERE status = 'done' AND (?1::text IS NULL OR cemetery_id = ?1::text) AND substr(done_at, 1, 4) = ?2 GROUP BY kind ORDER BY n DESC",
            c(),
        )?;
        let cases = json_rows(
            &self.conn,
            "SELECT kind, count(*) AS n, count(*) FILTER (WHERE status = 'approved') AS approved, count(*) FILTER (WHERE status = 'rejected') AS rejected,
                    count(*) FILTER (WHERE status IN ('received', 'missing_docs')) AS pending
             FROM cases WHERE voided_at IS NULL AND (?1::text IS NULL OR cemetery_id = ?1::text) AND substr(received_on, 1, 4) = ?2
             GROUP BY kind ORDER BY n DESC",
            c(),
        )?;
        let bookings = self.conn.query_row(
            "SELECT count(*) FROM bookings WHERE status = 'done' AND (?1::text IS NULL OR cemetery_id = ?1::text) AND substr(starts_at, 1, 4) = ?2",
            params![cemetery_id, y],
            |r| r.get::<_, i64>(0),
        )?;
        let plots = self.plot_summaries(&PlotFilter { cemetery_id: cemetery_id.map(String::from), light: true, ..Default::default() })?;
        let mut by_status: BTreeMap<String, i64> = BTreeMap::new();
        for p in &plots.items {
            *by_status.entry(p.status.as_str().to_string()).or_default() += 1;
        }
        let cemetery = match cemetery_id {
            Some(id) => self.conn.query_row("SELECT name FROM cemeteries WHERE id = ?1", params![id], |r| r.get::<_, String>(0)).optional()?,
            None => None,
        };
        Ok(json!({
            "year": year,
            "cemetery": cemetery,
            "operator": settings::get(&self.conn, "operator.name", None)?.unwrap_or_default(),
            "burials": burials,
            "burialsTotal": burials.iter().map(|b| b["n"].as_i64().unwrap_or(0)).sum::<i64>(),
            "leases": leases,
            "revenue": revenue,
            "revenueGrossHuf": revenue.iter().map(|r| r["grossHuf"].as_i64().unwrap_or(0)).sum::<i64>(),
            "reversedHuf": reversed,
            "paymentsHuf": paid,
            "receivables": { "count": open_n, "openHuf": open_huf, "overdueHuf": overdue_huf },
            "cash": { "inHuf": cash_in, "outHuf": cash_out },
            "plots": { "total": plots.items.len(), "byStatus": by_status },
            "workDone": work,
            "ceremonies": bookings,
            "cases": cases,
            "capacity": self.capacity(cemetery_id)?,
        }))
    }

    // ---------------------------------------------------------------- capacity (FR-FEN-02)

    /// Free plots per section, the yearly burial rate of the last years and
    /// the years left until the section fills up.
    pub fn capacity(&self, cemetery_id: Option<&str>) -> Result<Vec<Value>> {
        let plots = self.plot_summaries(&PlotFilter { cemetery_id: cemetery_id.map(String::from), light: true, ..Default::default() })?;
        #[derive(Default)]
        struct Acc {
            total: i64,
            free: i64,
            resellable: i64,
            expired: i64,
        }
        let mut acc: BTreeMap<String, Acc> = BTreeMap::new();
        for p in &plots.items {
            let a = acc.entry(p.section_id.clone()).or_default();
            a.total += 1;
            match p.status.as_str() {
                "free" => a.free += 1,
                "resellable" => a.resellable += 1,
                "expired" | "notified" => a.expired += 1,
                _ => {}
            }
        }
        let this_year = chrono::Datelike::year(&today());
        let from = (this_year - FORECAST_YEARS).to_string();
        let to = (this_year - 1).to_string();
        // new placements into an empty plot fill capacity; further burials do not
        let mut rate: BTreeMap<String, i64> = BTreeMap::new();
        let mut st = self.conn.prepare(
            "SELECT p.section_id, count(DISTINCT e.plot_id) FROM interment_events e JOIN plots p ON p.id = e.plot_id
             WHERE e.voided_at IS NULL AND e.event_type IN ('coffin_burial', 'urn_placement', 'common_grave')
               AND substr(e.event_date, 1, 4) BETWEEN ?1 AND ?2
               AND NOT EXISTS (SELECT 1 FROM interment_events o WHERE o.plot_id = e.plot_id AND o.voided_at IS NULL AND o.event_date < e.event_date)
               AND (?3::text IS NULL OR p.cemetery_id = ?3::text)
             GROUP BY p.section_id",
        )?;
        for r in st.query_map(params![from, to, cemetery_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))? {
            let (sid, n) = r?;
            rate.insert(sid, n);
        }
        let mut out = vec![];
        let mut st = self.conn.prepare(
            "SELECT s.id, c.name, s.code, s.name FROM sections s JOIN cemeteries c ON c.id = s.cemetery_id
             WHERE s.voided_at IS NULL AND (?1::text IS NULL OR s.cemetery_id = ?1::text) ORDER BY c.name, s.code",
        )?;
        let rows: Vec<(String, String, String, String)> =
            st.query_map(params![cemetery_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<std::result::Result<_, _>>()?;
        for (sid, cname, code, sname) in rows {
            let a = acc.remove(&sid).unwrap_or_default();
            let per_year = rate.get(&sid).copied().unwrap_or(0) as f64 / FORECAST_YEARS as f64;
            let years_left = if per_year > 0.0 { Some(((a.free + a.resellable) as f64 / per_year * 10.0).round() / 10.0) } else { None };
            out.push(json!({
                "sectionId": sid,
                "cemeteryName": cname,
                "sectionCode": code,
                "sectionName": sname,
                "total": a.total,
                "free": a.free,
                "resellable": a.resellable,
                "expired": a.expired,
                "perYear": (per_year * 10.0).round() / 10.0,
                "yearsLeft": years_left,
            }));
        }
        out.sort_by(|a, b| {
            crate::text::hu_compare(a["cemeteryName"].as_str().unwrap_or(""), b["cemeteryName"].as_str().unwrap_or(""))
                .then_with(|| crate::text::hu_compare(a["sectionCode"].as_str().unwrap_or(""), b["sectionCode"].as_str().unwrap_or("")))
        });
        Ok(out)
    }

    // ---------------------------------------------------------------- e-mail notices (FR-UGY-01)

    fn reply_to(&self) -> Result<Option<String>> {
        Ok(settings::get(&self.conn, "operator.email", None)?.filter(|e| e.contains('@')))
    }

    fn signature(&self) -> Result<String> {
        let mut lines = vec![];
        for k in ["operator.name", "operator.address", "operator.phone", "operator.email"] {
            if let Some(v) = settings::get(&self.conn, k, None)?.filter(|v| !v.trim().is_empty()) {
                lines.push(v);
            }
        }
        Ok(lines.join("\n"))
    }

    /// Expiry notices for the holders of the plots. Plots whose holder has no
    /// e-mail address come back in `withoutEmail` (they get a letter).
    pub fn expiry_mails(&self, s: &Session, plot_ids: &[String]) -> Result<Value> {
        s.require(Permission::RecordLeases)?;
        let sig = self.signature()?;
        let reply = self.reply_to()?;
        let mut mails = vec![];
        let mut without = vec![];
        for pid in plot_ids {
            let row: Option<(String, String, Option<String>, Option<String>, Option<String>)> = self
                .conn
                .query_row(
                    "SELECT p.identifier, c.name, l.expiry_date, trim(pe.name_prefix || ' ' || pe.family_name || ' ' || pe.given_name), nullif(trim(pe.email), '')
                     FROM plots p JOIN cemeteries c ON c.id = p.cemetery_id
                     LEFT JOIN leases l ON l.plot_id = p.id AND l.status = 'active' AND l.voided_at IS NULL
                     LEFT JOIN plot_holders h ON h.plot_id = p.id AND h.role = 'primary' AND h.end_date IS NULL AND h.voided_at IS NULL
                     LEFT JOIN persons pe ON pe.id = h.person_id
                     WHERE p.id = ?1 LIMIT 1",
                    params![pid],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
                )
                .optional()?;
            let Some((ident, cem, expiry, holder, email)) = row else { return Err(CoreError::not_found("plot", pid)) };
            match email.filter(|e| e.contains('@')) {
                Some(to) => {
                    let when = expiry.as_deref().map(|d| parse_date(d).map(|d| d.format("%Y. %m. %d.").to_string())).transpose()?.unwrap_or_default();
                    let past = expiry.as_deref().map(|d| d < fmt_date(today()).as_str()).unwrap_or(false);
                    let lead = if past { format!("lejárt {when} napon") } else { format!("{when} napon lejár") };
                    let body = format!(
                        "Tisztelt {name}!\n\nTájékoztatjuk, hogy a(z) {cem} {ident} jelű temetési helyének használati joga {lead}.\n\n\
                         A használati jog meghosszabbítható (újraváltás). Kérjük, keresse fel ügyfélszolgálatunkat, vagy válaszoljon erre a levélre. \
                         Ha a használati jogot nem váltják meg, a temetési hely a türelmi idő után újra értékesíthetővé válik.\n\n\
                         Üdvözlettel:\n{sig}",
                        name = holder.clone().unwrap_or_else(|| "Hozzátartozó".into()),
                    );
                    mails.push(OutMail { to, reply_to: reply.clone(), subject: format!("Temetési hely lejárata – {cem} {ident}"), body, ref_id: pid.clone() });
                }
                None => without.push(json!({ "plotId": pid, "identifier": ident, "holderName": holder })),
            }
        }
        Ok(json!({ "mails": mails, "withoutEmail": without }))
    }

    /// Payment reminder for an overdue invoice, to the buyer's e-mail address.
    pub fn reminder_mail(&self, s: &Session, invoice_id: &str) -> Result<OutMail> {
        s.require(Permission::RecordPayments)?;
        let inv = self.get_invoice(invoice_id)?;
        let to = Some(inv.buyer_email.trim().to_string()).filter(|e| e.contains('@')).ok_or_else(|| CoreError::new("no_email", "the buyer has no e-mail address"))?;
        let due = parse_date(&inv.due_date)?.format("%Y. %m. %d.").to_string();
        let open = inv.gross_huf - inv.paid_huf;
        let body = format!(
            "Tisztelt {buyer}!\n\nNyilvántartásunk szerint a(z) {number} számú, {due} fizetési határidejű számlából {open} Ft még nincs kiegyenlítve.\n\n\
             Kérjük, rendezze a tartozást. Ha időközben már fizetett, kérjük, tekintse levelünket tárgytalannak.\n\nÜdvözlettel:\n{sig}",
            buyer = inv.buyer_name,
            number = inv.number,
            open = crate::text::thousands(open),
            sig = self.signature()?,
        );
        Ok(OutMail { to, reply_to: self.reply_to()?, subject: format!("Fizetési emlékeztető – {} számla", inv.number), body, ref_id: inv.id })
    }

    // ---------------------------------------------------------------- public grave search (FR-UGY-02)

    /// Whether the operator publishes the grave search (opt-in).
    pub fn public_search_enabled(&self) -> Result<bool> {
        Ok(settings::get(&self.conn, "public.search_enabled", None)?.as_deref() == Some("true"))
    }

    /// Name, years and resting place of deceased the family has not asked to
    /// hide. No dates, no personal data beyond what a headstone shows.
    pub fn public_search(&self, query: &str) -> Result<Vec<Value>> {
        if !self.public_search_enabled()? {
            return Err(CoreError::new("public_search_disabled", "the grave search is not published"));
        }
        let norm = crate::text::normalize(query.trim());
        let words: Vec<&str> = norm.split(' ').filter(|w| w.chars().count() >= 2).take(4).collect();
        if words.iter().map(|w| w.chars().count()).sum::<usize>() < 3 {
            return Ok(vec![]);
        }
        let mut args: Vec<SqlValue> = vec![];
        let mut conds = vec![];
        for w in &words {
            args.push(txt(format!("%{w}%")));
            conds.push(format!("si.text LIKE ?{}", args.len()));
        }
        args.push(SqlValue::Integer(PUBLIC_SEARCH_LIMIT));
        let sql = format!(
            "WITH cand AS MATERIALIZED (SELECT si.ref_id FROM search_index si WHERE si.kind = 'deceased' AND {conds})
             SELECT trim(d.name_prefix || ' ' || d.family_name || ' ' || d.given_name) AS name,
                    substr(d.birth_date, 1, 4) AS birth_year, substr(d.death_date, 1, 4) AS death_year,
                    c.name AS cemetery, p.identifier AS plot
             FROM deceased d
             JOIN LATERAL (SELECT e.plot_id FROM interment_events e WHERE e.deceased_id = d.id AND e.voided_at IS NULL AND e.plot_id IS NOT NULL
                           ORDER BY e.event_date DESC LIMIT 1) last ON true
             JOIN plots p ON p.id = last.plot_id
             JOIN cemeteries c ON c.id = p.cemetery_id
             WHERE d.id IN (SELECT ref_id FROM cand) AND d.voided_at IS NULL AND d.hide_public = 0 AND c.voided_at IS NULL
             ORDER BY d.sort_key, d.death_date LIMIT ?{}",
            args.len(),
            conds = conds.join(" AND ")
        );
        json_rows(&self.conn, &sql, args)
    }

    /// FR-UGY-02: the family can ask that a deceased does not appear publicly.
    pub fn set_deceased_public(&self, s: &Session, deceased_id: &str, hidden: bool) -> Result<()> {
        s.require(Permission::RecordEvents)?;
        let tx = crate::db::tx(&self.conn)?;
        let n = tx.execute(
            "UPDATE deceased SET hide_public = ?2, updated_at = ?3, updated_by = ?4 WHERE id = ?1",
            params![deceased_id, hidden as i64, crate::util::now_ts(), s.user_name],
        )?;
        if n == 0 {
            return Err(CoreError::not_found("deceased", deceased_id));
        }
        crate::audit::record_simple(&tx, s, "deceased", deceased_id, if hidden { "hide_public" } else { "show_public" }, None)?;
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Store, Session, String) {
        let store = Store::in_memory().unwrap();
        let s = Session::system();
        crate::sample::load_sample(&store, &s).unwrap();
        let cem: String = store.conn.query_row("SELECT id FROM cemeteries LIMIT 1", [], |r| r.get(0)).unwrap();
        (store, s, cem)
    }

    #[test]
    fn annual_report_and_capacity() {
        let (store, s, cem) = setup();
        let year: i32 = store
            .conn
            .query_row("SELECT CAST(max(substr(event_date, 1, 4)) AS INTEGER) FROM interment_events WHERE cemetery_id = ?1", params![cem], |r| r.get(0))
            .unwrap();
        let r = store.annual_report(&s, Some(&cem), year).unwrap();
        assert!(r["burialsTotal"].as_i64().unwrap() > 0, "{r}");
        assert!(r["plots"]["total"].as_u64().unwrap() > 100);
        let cap = r["capacity"].as_array().unwrap();
        assert!(!cap.is_empty());
        let free: i64 = cap.iter().map(|c| c["free"].as_i64().unwrap()).sum();
        assert_eq!(free, *r["plots"]["byStatus"].get("free").and_then(|v| v.as_i64()).as_ref().unwrap_or(&0));
        assert!(store.annual_report(&s, None, 1800).is_err());
    }

    #[test]
    fn public_search_is_opt_in_and_respects_hiding() {
        let (store, s, _) = setup();
        assert_eq!(store.public_search("kovacs").unwrap_err().code, "public_search_disabled");
        crate::settings::set_many(&store.conn, &s, None, &[("public.search_enabled".to_string(), "true".to_string())].into_iter().collect()).unwrap();
        let hits = store.public_search("kovács").unwrap();
        assert!(!hits.is_empty());
        assert!(hits[0].get("birthDate").is_none(), "no full dates in public");
        assert!(hits[0]["plot"].is_string());
        assert!(store.public_search("ko").unwrap().is_empty(), "too short a query");
        let name = hits[0]["name"].as_str().unwrap().to_string();
        let id: String = store
            .conn
            .query_row("SELECT id FROM deceased WHERE trim(name_prefix || ' ' || family_name || ' ' || given_name) = ?1 LIMIT 1", params![name], |r| r.get(0))
            .unwrap();
        store.set_deceased_public(&s, &id, true).unwrap();
        let after = store.public_search("kovács").unwrap();
        assert_eq!(after.iter().filter(|h| h["name"] == name).count() + 1, hits.iter().filter(|h| h["name"] == name).count());
    }

    #[test]
    fn expiry_mails_split_by_email() {
        let (store, s, cem) = setup();
        let rows = store.expiries(&s, &ExpiryFilter { cemetery_id: Some(cem), within_days: 3650, include_expired: true, ..Default::default() }).unwrap();
        let with_holder: Vec<&ExpiryRow> = rows.iter().filter(|r| r.holder_id.is_some()).take(2).collect();
        assert_eq!(with_holder.len(), 2);
        store.conn.execute("UPDATE persons SET email = 'jogosult@pelda.hu' WHERE id = ?1", params![with_holder[0].holder_id]).unwrap();
        store.conn.execute("UPDATE persons SET email = '' WHERE id = ?1", params![with_holder[1].holder_id]).unwrap();
        let ids = vec![with_holder[0].plot_id.clone(), with_holder[1].plot_id.clone()];
        let r = store.expiry_mails(&s, &ids).unwrap();
        assert_eq!(r["mails"].as_array().unwrap().len(), 1);
        assert_eq!(r["mails"][0]["to"], "jogosult@pelda.hu");
        assert!(r["mails"][0]["body"].as_str().unwrap().contains(&with_holder[0].identifier));
        assert_eq!(r["withoutEmail"].as_array().unwrap().len(), 1);
    }
}
