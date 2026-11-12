//! Subscription and billing (W3): 30-day trial, per-cemetery price,
//! bank transfer (proforma → invoice, statement matching), read-only after expiry.

#![allow(clippy::type_complexity)]

use chrono::{Datelike, Duration, Months, NaiveDate, Utc};
use lunda_core::auth::{Permission, Session};
use lunda_core::sql::{params, Connection, OptionalExtension};
use lunda_core::util::new_id;
use lunda_core::{CoreError, Result, Store};
use serde_json::{json, Value};

use crate::mail;
use lunda_core::szamlazz::{self, Document, Line, Party};

pub const MONTHLY_NET: i64 = 45_000;
pub const YEARLY_NET: i64 = 450_000;
pub const VAT_PERCENT: i64 = 27;
pub const TRANSFER_DUE_DAYS: i64 = 8;
/// Days after the due date before an unpaid account becomes read-only.
pub const OVERDUE_GRACE_DAYS: i64 = 15;
/// Renewal proforma this many days before the period ends.
pub const RENEWAL_NOTICE_DAYS: i64 = 10;

pub fn unit_net(plan: &str) -> i64 {
    if plan == "yearly" {
        YEARLY_NET
    } else {
        MONTHLY_NET
    }
}

pub fn gross(net: i64) -> i64 {
    net + (net as f64 * VAT_PERCENT as f64 / 100.0).round() as i64
}

pub(crate) fn today() -> NaiveDate {
    Utc::now().with_timezone(&chrono_tz_budapest()).date_naive()
}

/// Europe/Budapest without a time-zone database: CET/CEST by the EU rule.
fn chrono_tz_budapest() -> chrono::FixedOffset {
    let now = Utc::now();
    let y = now.year();
    let last_sunday = |m: u32| {
        let mut d = NaiveDate::from_ymd_opt(y, m, 31).unwrap();
        while d.weekday() != chrono::Weekday::Sun {
            d -= Duration::days(1);
        }
        d.and_hms_opt(1, 0, 0).unwrap().and_utc()
    };
    let summer = now >= last_sunday(3) && now < last_sunday(10);
    chrono::FixedOffset::east_opt(if summer { 7200 } else { 3600 }).unwrap()
}

pub fn period_end(start: NaiveDate, plan: &str) -> NaiveDate {
    start.checked_add_months(Months::new(if plan == "yearly" { 12 } else { 1 })).unwrap_or(start)
}

pub(crate) fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub(crate) fn hu_date(d: NaiveDate) -> String {
    d.format("%Y. %m. %d.").to_string()
}

pub(crate) fn parse(d: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(&d[..10.min(d.len())], "%Y-%m-%d")?)
}

// ---------------------------------------------------------------- status

/// The organisation's state: active, read_only (older accounts) or closed.
/// Subscriptions are per cemetery (`subscriptions`).
pub fn effective_status(conn: &Connection, tenant_id: &str) -> Result<String> {
    Ok(conn.query_row("SELECT status FROM platform.tenants WHERE id = ?1", params![tenant_id], |r| r.get(0))?)
}

/// API methods that only read; everything else is refused when the account is read-only.
pub fn is_read_method(m: &str) -> bool {
    const SUFFIXES: &[&str] = &[
        ".list",
        ".get",
        ".find",
        ".similar",
        ".events",
        ".plots",
        ".preview",
        ".raw",
        ".data",
        ".permissions",
        ".all",
        ".utilization",
        ".yearly",
        ".verify",
        ".propose",
    ];
    SUFFIXES.iter().any(|s| m.ends_with(s))
        || matches!(
            m,
            "search.quick"
                | "dashboard"
                | "print.document"
                | "export.file"
                | "import.check"
                | "cash.day"
                | "finance.status"
                | "finance.revenue"
                | "invoices.pdf"
                | "operations.agenda"
                | "contractors.account"
                | "reminders.status"
                | "conditions.plot"
                | "persons.dataExport"
                | "stock.moves"
                | "print.burialItems"
                | "fees.current"
                | "stock.stocktakes"
        )
}

/// Session limited to reading (keeps viewing, printing, exporting).
pub fn read_only_session(s: &Session) -> Session {
    let mut r = s.clone();
    r.permissions.retain(|p| matches!(p, Permission::ViewPersonal | Permission::Print | Permission::Export));
    r
}

// ---------------------------------------------------------------- overview

pub fn overview(store: &Store, tenant_id: &str) -> Result<Value> {
    let conn = &store.conn;
    let details: Option<Value> = conn
        .query_row("SELECT name, tax_number, postcode, city, address, email FROM platform.billing_details WHERE tenant_id = ?1", params![tenant_id], |r| {
            Ok(json!({ "name": r.get::<_, String>(0)?, "taxNumber": r.get::<_, String>(1)?, "postcode": r.get::<_, String>(2)?,
                           "city": r.get::<_, String>(3)?, "address": r.get::<_, String>(4)?, "email": r.get::<_, String>(5)? }))
        })
        .optional()?;
    let invoices: Vec<Value> = conn
        .prepare(
            "SELECT id, kind, number, description, gross_huf, to_char(created_at, 'YYYY-MM-DD'), to_char(due_date, 'YYYY-MM-DD'), status, pdf IS NOT NULL
             FROM platform.invoices WHERE tenant_id = ?1 ORDER BY created_at DESC LIMIT 50",
        )?
        .query_map(params![tenant_id], |r| {
            Ok(json!({ "id": r.get::<_, String>(0)?, "kind": r.get::<_, String>(1)?, "number": r.get::<_, String>(2)?, "description": r.get::<_, String>(3)?,
                       "grossHuf": r.get::<_, i64>(4)?, "date": r.get::<_, String>(5)?, "dueDate": r.get::<_, Option<String>>(6)?,
                       "status": r.get::<_, String>(7)?, "hasPdf": r.get::<_, bool>(8)? }))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let plan = |p: &str| json!({ "unitNet": unit_net(p), "net": unit_net(p), "gross": gross(unit_net(p)) });
    let subs = crate::subscriptions::overview(store, tenant_id)?;
    Ok(json!({
        "cardPayments": crate::barion::config().is_some(),
        "vatPercent": VAT_PERCENT,
        "prices": { "monthly": plan("monthly"), "yearly": plan("yearly") },
        "details": details,
        "cemeteries": subs["cemeteries"],
        "trialDays": subs["trialDays"],
        "demoDays": subs["demoDays"],
        "verifyHuf": subs["verifyHuf"],
        "cardSubscriptions": subs["cardSubscriptions"],
        "trialUsed": subs["trialUsed"],
        "invoices": invoices,
    }))
}

pub fn save_details(conn: &Connection, tenant_id: &str, a: &Value) -> Result<()> {
    let f = |k: &str| a[k].as_str().unwrap_or("").trim().to_string();
    for (k, field) in [("name", "name"), ("postcode", "postcode"), ("city", "city"), ("address", "address"), ("email", "email")] {
        if f(k).is_empty() {
            return Err(CoreError::validation(field, format!("{field} is required")).with("code", "required"));
        }
    }
    if !crate::valid_email(&f("email")) {
        return Err(CoreError::validation("email", "invalid e-mail address").with("code", "email_invalid"));
    }
    let tax = f("taxNumber");
    if !tax.is_empty() && !tax.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return Err(CoreError::validation("taxNumber", "invalid tax number").with("code", "tax_number_invalid"));
    }
    conn.execute(
        "INSERT INTO platform.billing_details (tenant_id, name, tax_number, postcode, city, address, email) VALUES (?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT (tenant_id) DO UPDATE SET name=excluded.name, tax_number=excluded.tax_number, postcode=excluded.postcode,
             city=excluded.city, address=excluded.address, email=excluded.email, updated_at=now()",
        params![tenant_id, f("name"), tax, f("postcode"), f("city"), f("address"), f("email")],
    )?;
    conn.execute("UPDATE platform.tenants SET billing_email = ?2, tax_number = ?3 WHERE id = ?1", params![tenant_id, f("email"), tax])?;
    Ok(())
}

pub(crate) fn buyer(conn: &Connection, tenant_id: &str) -> Result<Party> {
    conn.query_row("SELECT name, tax_number, postcode, city, address, email FROM platform.billing_details WHERE tenant_id = ?1", params![tenant_id], |r| {
        Ok(Party { name: r.get(0)?, tax_number: r.get(1)?, postcode: r.get(2)?, city: r.get(3)?, address: r.get(4)?, email: r.get(5)? })
    })
    .optional()?
    .ok_or_else(|| CoreError::new("billing_details_missing", "fill in the billing details first"))
}

// ---------------------------------------------------------------- documents

#[allow(clippy::too_many_arguments)]
pub(crate) fn issue(
    conn: &Connection,
    tenant_id: &str,
    proforma: bool,
    description: &str,
    quantity: i64,
    unit_net: i64,
    period: (NaiveDate, NaiveDate),
    method: &str,
    proforma_of: Option<(&str, &str)>,
) -> Result<String> {
    let party = buyer(conn, tenant_id)?;
    let seq: i64 = conn.query_row("SELECT count(*) + 1 FROM platform.invoices", [], |r| r.get(0))?;
    let today = today();
    let due = today + Duration::days(if proforma { TRANSFER_DUE_DAYS } else { 0 });
    let comment = format!("Időszak: {} – {}", hu_date(period.0), hu_date(period.1 - Duration::days(1)));
    let doc = Document {
        proforma,
        issue_date: ymd(today),
        fulfilment_date: ymd(if proforma { today } else { period.0.min(today) }),
        due_date: ymd(due),
        payment_method: if method == "card" { "Bankkártya".into() } else { "Átutalás".into() },
        note: format!("TEMISZ előfizetés. Közlemény: {}", &tenant_id[..8]),
        order_number: tenant_id[..8].to_string(),
        proforma_number: proforma_of.map(|(_, n)| n.to_string()),
        paid: !proforma,
        buyer: party,
        lines: vec![Line {
            name: description.into(),
            quantity,
            unit: if unit_net == YEARLY_NET { "temető/év".into() } else { "temető/hó".into() },
            unit_net,
            vat_percent: VAT_PERCENT,
            comment,
        }],
    };
    let (net, vat, gross) = szamlazz::amounts(&doc.lines[0]);
    let issued = szamlazz::issue(&doc, seq)?;
    let id = new_id();
    conn.execute(
        "INSERT INTO platform.invoices (id, tenant_id, kind, number, proforma_id, description, quantity, unit_net_huf, net_huf, vat_huf, gross_huf,
                                        period_start, period_end, due_date, payment_method, status, pdf, provider_log, paid_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,CAST(?12 AS DATE),CAST(?13 AS DATE),CAST(?14 AS DATE),?15,?16,?17,?18, CASE WHEN ?16 = 'paid' THEN now() END)",
        params![
            id,
            tenant_id,
            if proforma { "proforma" } else { "invoice" },
            issued.number,
            proforma_of.map(|(i, _)| i.to_string()),
            description,
            quantity,
            unit_net,
            net,
            vat,
            gross,
            ymd(period.0),
            ymd(period.1),
            if proforma { Some(ymd(due)) } else { None },
            method,
            if proforma { "open" } else { "paid" },
            issued.pdf,
            issued.log
        ],
    )?;
    if issued.pdf.is_none() {
        // test mode or PDF not returned: tell the customer by e-mail (Számlázz.hu e-mails its own PDFs)
        let to = doc.buyer.email.clone();
        let what = if proforma { "díjbekérő" } else { "számla" };
        mail::send(
            conn,
            &to,
            &format!("TEMISZ {what}: {}", issued.number),
            &format!(
                "Jó napot kívánunk!\n\nElkészült a TEMISZ előfizetés {what}e ({}).\n\n{description}\n{quantity} × {} Ft + {VAT_PERCENT}% ÁFA\nFizetendő: {} Ft{}{}",
                issued.number,
                fmt_huf(unit_net),
                fmt_huf(gross),
                if proforma { format!("\nFizetési határidő: {}\nKözlemény: {}", hu_date(due), &tenant_id[..8]) } else { String::new() },
                mail::FOOTER
            ),
            if proforma { "proforma" } else { "invoice" },
        )?;
    }
    Ok(id)
}

pub fn fmt_huf(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{a0}');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Platform admin: the transfer arrived. Issues the final invoice and starts/extends the period.
pub fn mark_paid(conn: &Connection, invoice_id: &str) -> Result<()> {
    mark_paid_by(conn, invoice_id, "transfer")
}

/// A proforma was paid (`transfer` or `card`): final invoice, period started or extended.
pub fn mark_paid_by(conn: &Connection, invoice_id: &str, method: &str) -> Result<()> {
    let row: Option<(String, String, String, i64, i64, String, String, String)> = conn
        .query_row(
            "SELECT tenant_id, number, description, quantity, unit_net_huf, to_char(period_start, 'YYYY-MM-DD'), to_char(period_end, 'YYYY-MM-DD'), status
             FROM platform.invoices WHERE id = ?1 AND kind = 'proforma'",
            params![invoice_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )
        .optional()?;
    let Some((tenant, number, desc, qty, unit, ps, pe, status)) = row else { return Err(CoreError::not_found("invoice", invoice_id)) };
    if status != "open" {
        return Err(CoreError::new("invoice_not_open", "this proforma is not open"));
    }
    let (ps, pe) = (parse(&ps)?, parse(&pe)?);
    let t = lunda_core::db::tx(conn)?;
    t.execute("UPDATE platform.invoices SET status = 'paid', paid_at = now() WHERE id = ?1", params![invoice_id])?;
    issue(&t, &tenant, false, &desc, qty, unit, (ps, pe), method, Some((invoice_id, &number)))?;
    // a full-period proforma moves the period; a top-up for extra cemeteries does not
    t.execute(
        "UPDATE platform.subscriptions SET status = 'active', updated_at = now(),
             current_period_start = CASE WHEN CAST(?3 AS DATE) > coalesce(current_period_end, CAST(?2 AS DATE)) OR status = 'pending' THEN CAST(?2 AS DATE) ELSE current_period_start END,
             current_period_end = GREATEST(coalesce(current_period_end, CAST(?3 AS DATE)), CAST(?3 AS DATE))
         WHERE tenant_id = ?1",
        params![tenant, ymd(ps), ymd(pe)],
    )?;
    t.execute("UPDATE platform.tenants SET status = 'active', read_only_since = NULL WHERE id = ?1 AND status <> 'closed'", params![tenant])?;
    // the cemetery the proforma was for (older proformas: every transfer cemetery)
    let cem: Option<String> = t.query_row("SELECT cemetery_id FROM platform.invoices WHERE id = ?1", params![invoice_id], |r| r.get(0))?;
    crate::subscriptions::proforma_paid(&t, &tenant, cem.as_deref(), &ymd(ps), &ymd(pe))?;
    t.commit()?;
    Ok(())
}

// ---------------------------------------------------------------- daily job

pub(crate) fn notify_once(conn: &Connection, tenant_id: &str, key: &str) -> Result<bool> {
    Ok(conn.execute("INSERT INTO platform.notifications (tenant_id, key) VALUES (?1, ?2) ON CONFLICT DO NOTHING", params![tenant_id, key])? == 1)
}

pub(crate) fn tenant_email(conn: &Connection, tenant_id: &str) -> Result<(String, String)> {
    Ok(conn.query_row("SELECT billing_email, name FROM platform.tenants WHERE id = ?1", params![tenant_id], |r| Ok((r.get(0)?, r.get(1)?)))?)
}

// ---------------------------------------------------------------- bank statement matching

fn alnum(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

struct OpenProforma {
    id: String,
    tenant: String,
    number: String,
    gross: i64,
    buyer: String,
}

fn open_proformas(conn: &Connection) -> Result<Vec<OpenProforma>> {
    let mut st = conn.prepare(
        "SELECT i.id, i.tenant_id, i.number, i.gross_huf, coalesce(b.name, t.name)
         FROM platform.invoices i JOIN platform.tenants t ON t.id = i.tenant_id LEFT JOIN platform.billing_details b ON b.tenant_id = i.tenant_id
         WHERE i.kind = 'proforma' AND i.status = 'open' ORDER BY i.created_at",
    )?;
    let rows = st.query_map([], |r| Ok(OpenProforma { id: r.get(0)?, tenant: r.get(1)?, number: r.get(2)?, gross: r.get(3)?, buyer: r.get(4)? }))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// The proforma a transfer pays, and whether the match is certain.
/// Certain: the reference names the proforma or the customer (the proforma
/// asks for the customer code as reference) and the amount covers it.
/// Otherwise a suggestion: the same amount from a payer with a similar name.
fn find_proforma<'a>(open: &'a [OpenProforma], amount: i64, partner: &str, reference: &str) -> Option<(&'a OpenProforma, bool)> {
    let r = alnum(reference);
    let named: Vec<&OpenProforma> = open
        .iter()
        .filter(|p| {
            let num = alnum(&p.number);
            (!num.is_empty() && r.contains(&num)) || r.contains(&p.tenant[..8.min(p.tenant.len())].to_lowercase())
        })
        .collect();
    if let Some(p) = named.iter().find(|p| p.gross == amount).or(if named.len() == 1 { named.first() } else { None }) {
        return Some((p, amount >= p.gross));
    }
    let partner = lunda_core::text::normalize(partner);
    let same: Vec<&OpenProforma> = open
        .iter()
        .filter(|p| p.gross == amount)
        .filter(|p| {
            let first = lunda_core::text::normalize(&p.buyer);
            let word = first.split(' ').find(|w| w.len() >= 4).unwrap_or("");
            !word.is_empty() && partner.contains(word)
        })
        .collect();
    if same.len() == 1 {
        Some((same[0], false))
    } else {
        None
    }
}

/// Platform admin: imports TEMISZ's bank statement and settles the proformas
/// it certainly pays (final invoice, period started). Returns the counts.
pub fn import_statement(conn: &Connection, file_name: &str, content_b64: &str, by: &str) -> Result<Value> {
    let (lines, mut skipped) = lunda_core::store::finance::parse_bank_statement(file_name, content_b64)?;
    let mut added = 0;
    for l in lines.iter().filter(|l| l.amount_huf > 0) {
        let n = conn.execute(
            "INSERT INTO platform.bank_lines (id, booked_on, amount_huf, partner, reference, fingerprint) VALUES (?1, CAST(?2 AS DATE), ?3, ?4, ?5, ?6)
             ON CONFLICT (fingerprint) DO NOTHING",
            params![new_id(), l.booked_on, l.amount_huf, l.partner, l.reference, l.fingerprint],
        )?;
        if n == 1 {
            added += 1;
        } else {
            skipped += 1;
        }
    }
    let matched = auto_match(conn, by)?;
    Ok(json!({ "added": added, "skipped": skipped, "matched": matched }))
}

fn auto_match(conn: &Connection, by: &str) -> Result<usize> {
    let mut st = conn.prepare("SELECT id, amount_huf, partner, reference FROM platform.bank_lines WHERE status = 'unmatched' ORDER BY booked_on")?;
    let lines: Vec<(String, i64, String, String)> =
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<std::result::Result<_, _>>()?;
    let mut n = 0;
    for (id, amount, partner, reference) in lines {
        let open = open_proformas(conn)?;
        if let Some((p, true)) = find_proforma(&open, amount, &partner, &reference) {
            let t = lunda_core::db::tx(conn)?;
            settle(&t, &id, &p.id, by)?;
            t.commit()?;
            n += 1;
        }
    }
    Ok(n)
}

fn settle(conn: &Connection, line_id: &str, invoice_id: &str, by: &str) -> Result<()> {
    mark_paid(conn, invoice_id)?;
    conn.execute(
        "UPDATE platform.bank_lines SET status = 'matched', invoice_id = ?2, matched_at = now(), matched_by = ?3 WHERE id = ?1",
        params![line_id, invoice_id, by],
    )?;
    Ok(())
}

/// Lines not matched yet, each with a suggested proforma when there is one.
pub fn bank_lines(conn: &Connection) -> Result<Vec<Value>> {
    let open = open_proformas(conn)?;
    let mut st = conn.prepare(
        "SELECT id, to_char(booked_on, 'YYYY-MM-DD'), amount_huf, partner, reference FROM platform.bank_lines WHERE status = 'unmatched' ORDER BY booked_on DESC LIMIT 200",
    )?;
    let rows: Vec<(String, String, i64, String, String)> =
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?.collect::<std::result::Result<_, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, date, amount, partner, reference)| {
            let s = find_proforma(&open, amount, &partner, &reference).map(|(p, _)| p);
            json!({ "id": id, "bookedOn": date, "amountHuf": amount, "partner": partner, "reference": reference,
                    "suggestedId": s.map(|p| p.id.clone()), "suggestedNumber": s.map(|p| p.number.clone()), "suggestedBuyer": s.map(|p| p.buyer.clone()),
                    "suggestedGross": s.map(|p| p.gross) })
        })
        .collect())
}

/// Platform admin: pays a proforma with a bank line by hand, or sets a line aside.
pub fn resolve_line(conn: &Connection, line_id: &str, invoice_id: Option<&str>, by: &str) -> Result<()> {
    let status: Option<String> = conn.query_row("SELECT status FROM platform.bank_lines WHERE id = ?1", params![line_id], |r| r.get(0)).optional()?;
    match status.as_deref() {
        None => return Err(CoreError::not_found("bank_line", line_id)),
        Some("unmatched") => {}
        Some(_) => return Err(CoreError::new("bank_already_matched", "this line is already settled")),
    }
    let t = lunda_core::db::tx(conn)?;
    match invoice_id {
        Some(inv) => settle(&t, line_id, inv, by)?,
        None => {
            t.execute("UPDATE platform.bank_lines SET status = 'ignored', matched_at = now(), matched_by = ?2 WHERE id = ?1", params![line_id, by])?;
        }
    }
    t.commit()?;
    Ok(())
}

pub fn invoice_pdf(conn: &Connection, tenant_id: &str, invoice_id: &str) -> Result<Value> {
    let row: Option<(String, Option<Vec<u8>>)> = conn
        .query_row("SELECT number, pdf FROM platform.invoices WHERE id = ?1 AND tenant_id = ?2", params![invoice_id, tenant_id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    let Some((number, Some(pdf))) = row else { return Err(CoreError::new("pdf_unavailable", "no PDF for this document")) };
    use base64::Engine;
    Ok(
        json!({ "fileName": format!("{}.pdf", number.replace('/', "-")), "path": "", "pages": 1, "sha256": "", "data": base64::engine::general_purpose::STANDARD.encode(pdf) }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prices_and_periods() {
        assert_eq!(gross(MONTHLY_NET), 57_150);
        assert_eq!(gross(YEARLY_NET * 2), 1_143_000);
        let d = NaiveDate::from_ymd_opt(2026, 1, 31).unwrap();
        assert_eq!(period_end(d, "monthly"), NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(period_end(d, "yearly"), NaiveDate::from_ymd_opt(2027, 1, 31).unwrap());
        assert_eq!(fmt_huf(450_000), "450\u{a0}000");
    }

    #[test]
    fn read_methods() {
        assert!(is_read_method("plots.list") && is_read_method("search.quick") && is_read_method("export.file") && is_read_method("burial.preview"));
        assert!(!is_read_method("burial.record") && !is_read_method("settings.set") && !is_read_method("plots.ensure") && !is_read_method("sample.load"));
    }
}
