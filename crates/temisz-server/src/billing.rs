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

fn today() -> NaiveDate {
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

fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn hu_date(d: NaiveDate) -> String {
    d.format("%Y. %m. %d.").to_string()
}

fn parse(d: &str) -> Result<NaiveDate> {
    Ok(NaiveDate::parse_from_str(&d[..10.min(d.len())], "%Y-%m-%d")?)
}

/// Active (not archived) cemeteries in the tenant schema the connection points at.
pub fn cemetery_count(store: &Store) -> Result<i64> {
    Ok(store.conn.query_row("SELECT count(*) FROM cemeteries WHERE voided_at IS NULL", [], |r| r.get(0))?)
}

// ---------------------------------------------------------------- status

pub struct Sub {
    pub plan: String,
    pub method: String,
    pub count: i64,
    pub status: String,
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
    pub cancel_at_end: bool,
}

pub fn subscription(conn: &Connection, tenant_id: &str) -> Result<Option<Sub>> {
    let row = conn
        .query_row(
            "SELECT plan, payment_method, cemetery_count, status, to_char(current_period_start, 'YYYY-MM-DD'),
                    to_char(current_period_end, 'YYYY-MM-DD'), cancel_at_period_end
             FROM platform.subscriptions WHERE tenant_id = ?1",
            params![tenant_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<String>>(5)?,
                    r.get::<_, bool>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((plan, method, count, status, start, end, cancel)) = row else { return Ok(None) };
    Ok(Some(Sub {
        plan,
        method,
        count,
        status,
        start: start.as_deref().map(parse).transpose()?,
        end: end.as_deref().map(parse).transpose()?,
        cancel_at_end: cancel,
    }))
}

/// trial | active | read_only | closed – updated here when the trial has run out.
pub fn effective_status(conn: &Connection, tenant_id: &str) -> Result<String> {
    let (status, trial_over): (String, bool) =
        conn.query_row("SELECT status, trial_ends_at <= now() FROM platform.tenants WHERE id = ?1", params![tenant_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if status == "trial" && trial_over {
        // ordered during the trial: work goes on while the proforma runs (the
        // daily job makes the account read-only if it is not paid in time)
        let ordered: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM platform.subscriptions WHERE tenant_id = ?1 AND status IN ('active', 'pending'))",
            params![tenant_id],
            |r| r.get(0),
        )?;
        let next = if ordered { "active" } else { "read_only" };
        conn.execute(
            "UPDATE platform.tenants SET status = ?2, read_only_since = CASE WHEN ?2 = 'read_only' THEN now() END WHERE id = ?1 AND status = 'trial'",
            params![tenant_id, next],
        )?;
        return Ok(next.into());
    }
    Ok(status)
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
    let status = effective_status(conn, tenant_id)?;
    let count = cemetery_count(store)?.max(1);
    let (trial_ends, trial_days): (String, i64) = conn.query_row(
        "SELECT to_char(trial_ends_at, 'YYYY-MM-DD'), CAST(CEIL(EXTRACT(EPOCH FROM (trial_ends_at - now())) / 86400) AS BIGINT) FROM platform.tenants WHERE id = ?1",
        params![tenant_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let details: Option<Value> = conn
        .query_row("SELECT name, tax_number, postcode, city, address, email FROM platform.billing_details WHERE tenant_id = ?1", params![tenant_id], |r| {
            Ok(json!({ "name": r.get::<_, String>(0)?, "taxNumber": r.get::<_, String>(1)?, "postcode": r.get::<_, String>(2)?,
                           "city": r.get::<_, String>(3)?, "address": r.get::<_, String>(4)?, "email": r.get::<_, String>(5)? }))
        })
        .optional()?;
    let sub = subscription(conn, tenant_id)?.map(|s| {
        json!({ "plan": s.plan, "paymentMethod": s.method, "cemeteryCount": s.count, "status": s.status,
                "periodStart": s.start.map(ymd), "periodEnd": s.end.map(ymd), "cancelAtPeriodEnd": s.cancel_at_end })
    });
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
    let plan = |p: &str| {
        let net = unit_net(p) * count;
        json!({ "unitNet": unit_net(p), "net": net, "gross": gross(net) })
    };
    Ok(json!({
        "cardPayments": crate::barion::config().is_some(),
        "status": status,
        "trialEndsAt": trial_ends,
        "trialDaysLeft": trial_days.max(0),
        "cemeteryCount": count,
        "vatPercent": VAT_PERCENT,
        "prices": { "monthly": plan("monthly"), "yearly": plan("yearly") },
        "details": details,
        "subscription": sub,
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

fn buyer(conn: &Connection, tenant_id: &str) -> Result<Party> {
    conn.query_row("SELECT name, tax_number, postcode, city, address, email FROM platform.billing_details WHERE tenant_id = ?1", params![tenant_id], |r| {
        Ok(Party { name: r.get(0)?, tax_number: r.get(1)?, postcode: r.get(2)?, city: r.get(3)?, address: r.get(4)?, email: r.get(5)? })
    })
    .optional()?
    .ok_or_else(|| CoreError::new("billing_details_missing", "fill in the billing details first"))
}

// ---------------------------------------------------------------- documents

#[allow(clippy::too_many_arguments)]
fn issue(
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

// ---------------------------------------------------------------- subscribing

pub fn subscribe(store: &Store, s: &Session, tenant_id: &str, plan: &str, method: &str) -> Result<Value> {
    s.require(Permission::ManageSettings)?;
    if !matches!(plan, "monthly" | "yearly") || method != "transfer" {
        return Err(CoreError::new("bad_request", "plan and payment method required"));
    }
    let conn = &store.conn;
    buyer(conn, tenant_id)?; // billing details are required
    if let Some(sub) = subscription(conn, tenant_id)? {
        if matches!(sub.status.as_str(), "active" | "pending") {
            return Err(CoreError::new("already_subscribed", "there is already a subscription"));
        }
    }
    // an unpaid proforma first: otherwise ordering again would reopen a
    // read-only account for another payment period without paying
    let unpaid: Option<String> = conn
        .query_row(
            "SELECT number FROM platform.invoices WHERE tenant_id = ?1 AND kind = 'proforma' AND status = 'open' ORDER BY created_at LIMIT 1",
            params![tenant_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(number) = unpaid {
        return Err(CoreError::new("unpaid_proforma", "an earlier proforma is still unpaid").with("number", number));
    }
    let count = cemetery_count(store)?.max(1);
    // the paid period starts when the free trial ends
    let (trial_end, in_trial): (String, bool) =
        conn.query_row("SELECT to_char(trial_ends_at, 'YYYY-MM-DD'), trial_ends_at > now() FROM platform.tenants WHERE id = ?1", params![tenant_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    let start = if in_trial { parse(&trial_end)? } else { today() };
    let t = lunda_core::db::tx(conn)?;
    t.execute(
        "INSERT INTO platform.subscriptions (tenant_id, plan, payment_method, cemetery_count, status, current_period_start, current_period_end)
         VALUES (?1, ?2, 'transfer', ?3, 'pending', CAST(?4 AS DATE), CAST(?5 AS DATE))
         ON CONFLICT (tenant_id) DO UPDATE SET plan=excluded.plan, payment_method='transfer', cemetery_count=excluded.cemetery_count,
             status='pending', current_period_start=excluded.current_period_start, current_period_end=excluded.current_period_end,
             cancel_at_period_end=false, updated_at=now()",
        params![tenant_id, plan, count, ymd(start), ymd(period_end(start, plan))],
    )?;
    issue(&t, tenant_id, true, "TEMISZ előfizetés", count, unit_net(plan), (start, period_end(start, plan)), "transfer", None)?;
    // working continues while the proforma is open (a read-only account opens up again)
    t.execute(
        "UPDATE platform.tenants SET status = 'active', read_only_since = NULL WHERE id = ?1 AND status IN ('trial', 'read_only') AND NOT ?2",
        params![tenant_id, in_trial],
    )?;
    t.commit()?;
    let _ = lunda_core::audit::record_simple(&store.conn, s, "subscription", tenant_id, "subscribe", Some(&format!("{plan}, transfer, {count}")));
    Ok(json!({ "ok": true }))
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
    t.commit()?;
    Ok(())
}

// ---------------------------------------------------------------- cemetery count changes

/// FR-ELO-01: what one more cemetery costs, shown before it is created.
pub fn quote_new_cemetery(store: &Store, tenant_id: &str) -> Result<Value> {
    let status = effective_status(&store.conn, tenant_id)?;
    let sub = subscription(&store.conn, tenant_id)?.filter(|s| matches!(s.status.as_str(), "active" | "pending"));
    let Some(sub) = sub else {
        return Ok(json!({ "inTrial": status == "trial", "monthlyNet": MONTHLY_NET, "yearlyNet": YEARLY_NET, "nowNet": 0 }));
    };
    let (now_net, _) = prorate(&sub, 1);
    Ok(json!({ "inTrial": false, "plan": sub.plan, "unitNet": unit_net(&sub.plan), "nowNet": now_net, "nowGross": gross(now_net),
               "periodEnd": sub.end.map(ymd), "newCount": sub.count + 1 }))
}

/// Net amount for `extra` cemeteries for the rest of the current period.
fn prorate(sub: &Sub, extra: i64) -> (i64, Option<(NaiveDate, NaiveDate)>) {
    let (Some(start), Some(end)) = (sub.start, sub.end) else { return (0, None) };
    let from = today().max(start);
    if from >= end {
        return (0, None);
    }
    let total = (end - start).num_days().max(1) as f64;
    let left = (end - from).num_days() as f64;
    ((unit_net(&sub.plan) as f64 * extra as f64 * left / total).round() as i64, Some((from, end)))
}

/// After cemeteries were created or archived: bill the extra ones for the rest of
/// the period with a proforma; fewer apply at renewal.
pub fn sync_cemetery_count(store: &Store, tenant_id: &str) -> Result<()> {
    let Some(sub) = subscription(&store.conn, tenant_id)?.filter(|s| matches!(s.status.as_str(), "active" | "pending")) else { return Ok(()) };
    let count = cemetery_count(store)?.max(1);
    if count == sub.count {
        return Ok(());
    }
    let extra = count - sub.count;
    if extra > 0 {
        let (net, period) = prorate(&sub, extra);
        if let (true, Some(p)) = (net > 0, period) {
            // one line: N extra cemeteries for the remaining days, as a single priced item
            issue(&store.conn, tenant_id, true, &format!("TEMISZ előfizetés – {extra} további temető, időarányosan"), 1, net, p, "transfer", None)?;
        }
    }
    if extra > 0 {
        store.conn.execute("UPDATE platform.subscriptions SET cemetery_count = ?2, updated_at = now() WHERE tenant_id = ?1", params![tenant_id, count])?;
    }
    Ok(())
}

// ---------------------------------------------------------------- daily job

fn notify_once(conn: &Connection, tenant_id: &str, key: &str) -> Result<bool> {
    Ok(conn.execute("INSERT INTO platform.notifications (tenant_id, key) VALUES (?1, ?2) ON CONFLICT DO NOTHING", params![tenant_id, key])? == 1)
}

fn tenant_email(conn: &Connection, tenant_id: &str) -> Result<(String, String)> {
    Ok(conn.query_row("SELECT billing_email, name FROM platform.tenants WHERE id = ?1", params![tenant_id], |r| Ok((r.get(0)?, r.get(1)?)))?)
}

/// Runs once a day (Vercel Cron): trial reminders, expiry, renewals, overdue proformas.
pub fn daily(store: &Store, base_url: &str) -> Result<Value> {
    let conn = &store.conn;
    let mut done = serde_json::Map::new();
    // trial reminders 7 and 2 days before the end
    let ending: Vec<(String, i64)> = conn
        .prepare(
            "SELECT t.id, CAST(CEIL(EXTRACT(EPOCH FROM (t.trial_ends_at - now())) / 86400) AS BIGINT) FROM platform.tenants t
             WHERE t.status = 'trial' AND t.trial_ends_at > now() AND t.trial_ends_at < now() + interval '8 days'
               AND NOT EXISTS (SELECT 1 FROM platform.subscriptions s WHERE s.tenant_id = t.id AND s.status IN ('active','pending'))",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut reminders = 0;
    for (tenant, days) in ending {
        let key = if days <= 2 {
            "trial_2"
        } else if days <= 7 {
            "trial_7"
        } else {
            continue;
        };
        if notify_once(conn, &tenant, key)? {
            let (to, org) = tenant_email(conn, &tenant)?;
            mail::send(
                conn,
                &to,
                &format!("A TEMISZ próbaidő {days} nap múlva lejár"),
                &format!(
                    "Jó napot kívánunk!\n\nA(z) {org} TEMISZ-próbaideje {days} nap múlva lejár. Ezután a nyilvántartás csak olvasható, amíg nem fizet elő.\n\nAz előfizetés temetőnként havi {} Ft + ÁFA, vagy évi {} Ft + ÁFA. Előfizetni a programban, a Beállítások › Előfizetés oldalon lehet:\n{base_url}/{}",
                    fmt_huf(MONTHLY_NET),
                    fmt_huf(YEARLY_NET),
                    mail::FOOTER
                ),
                "trial_reminder",
            )?;
            reminders += 1;
        }
    }
    done.insert("trialReminders".into(), json!(reminders));
    // trials that ran out
    let expired: Vec<String> = conn
        .prepare("SELECT id FROM platform.tenants WHERE status = 'trial' AND trial_ends_at <= now()")?
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    for tenant in &expired {
        if effective_status(conn, tenant)? != "read_only" {
            continue;
        }
        if notify_once(conn, tenant, "trial_ended")? {
            let (to, org) = tenant_email(conn, tenant)?;
            mail::send(
                conn,
                &to,
                "Lejárt a TEMISZ próbaidő",
                &format!("Jó napot kívánunk!\n\nA(z) {org} TEMISZ-próbaideje lejárt. Az adatok megmaradtak, olvashatók és letölthetők. A további munkához fizessen elő a Beállítások › Előfizetés oldalon:\n{base_url}/{}", mail::FOOTER),
                "trial_ended",
            )?;
        }
    }
    done.insert("trialsEnded".into(), json!(expired.len()));
    // renewal proformas for transfer subscriptions
    let due: Vec<(String, String, i64, String)> = conn
        .prepare(
            "SELECT s.tenant_id, s.plan, s.cemetery_count, to_char(s.current_period_end, 'YYYY-MM-DD') FROM platform.subscriptions s
             WHERE s.payment_method = 'transfer' AND s.status = 'active' AND NOT s.cancel_at_period_end
               AND s.current_period_end <= CAST(?1 AS DATE)
               AND NOT EXISTS (SELECT 1 FROM platform.invoices i WHERE i.tenant_id = s.tenant_id AND i.kind = 'proforma'
                               AND i.period_start = s.current_period_end AND i.status <> 'void')",
        )?
        .query_map(params![ymd(today() + Duration::days(RENEWAL_NOTICE_DAYS))], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<std::result::Result<_, _>>()?;
    for (tenant, plan, count, end) in &due {
        // bill the cemeteries in use now (archived ones drop off at renewal)
        let schema: String = conn.query_row("SELECT schema_name FROM platform.tenants WHERE id = ?1", params![tenant], |r| r.get(0))?;
        conn.use_schema(&schema)?;
        let now_count = cemetery_count(store)?.max(1);
        let start = parse(end)?;
        issue(conn, tenant, true, "TEMISZ előfizetés – következő időszak", now_count, unit_net(plan), (start, period_end(start, plan)), "transfer", None)?;
        conn.execute("UPDATE platform.subscriptions SET cemetery_count = ?2 WHERE tenant_id = ?1", params![tenant, now_count])?;
        let _ = count;
    }
    done.insert("renewalProformas".into(), json!(due.len()));
    // unpaid proformas past the grace period → read-only
    let overdue: Vec<String> = conn
        .prepare(
            "SELECT DISTINCT i.tenant_id FROM platform.invoices i JOIN platform.tenants t ON t.id = i.tenant_id
             WHERE i.kind = 'proforma' AND i.status = 'open' AND i.due_date + CAST(?1 AS INTEGER) < CAST(?2 AS DATE) AND t.status = 'active'",
        )?
        .query_map(params![OVERDUE_GRACE_DAYS as i32, ymd(today())], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    for tenant in &overdue {
        conn.execute("UPDATE platform.tenants SET status = 'read_only', read_only_since = now() WHERE id = ?1", params![tenant])?;
        conn.execute("UPDATE platform.subscriptions SET status = 'past_due' WHERE tenant_id = ?1", params![tenant])?;
        let (to, org) = tenant_email(conn, tenant)?;
        mail::send(
            conn,
            &to,
            "A TEMISZ-fiók csak olvasható lett",
            &format!("Jó napot kívánunk!\n\nA(z) {org} TEMISZ-díjbekérője lejárt, ezért a nyilvántartás csak olvasható. A befizetés beérkezése után azonnal újra szerkeszthető.{}", mail::FOOTER),
            "overdue",
        )?;
    }
    done.insert("overdue".into(), json!(overdue.len()));
    // cancelled subscriptions whose period is over
    let n = conn.execute(
        "UPDATE platform.tenants t SET status = 'read_only', read_only_since = now() FROM platform.subscriptions s
         WHERE s.tenant_id = t.id AND s.status = 'canceled' AND s.current_period_end <= CAST(?1 AS DATE) AND t.status = 'active'",
        params![ymd(today())],
    )?;
    done.insert("canceledEnded".into(), json!(n));
    Ok(Value::Object(done))
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
    fn proration_for_extra_cemetery() {
        let start = today() - Duration::days(10);
        let sub = Sub {
            plan: "yearly".into(),
            method: "transfer".into(),
            count: 1,
            status: "active".into(),
            start: Some(start),
            end: Some(period_end(start, "yearly")),
            cancel_at_end: false,
        };
        let (net, p) = prorate(&sub, 1);
        let total = (period_end(start, "yearly") - start).num_days() as f64;
        assert_eq!(net, (YEARLY_NET as f64 * (total - 10.0) / total).round() as i64);
        assert_eq!(p.unwrap().0, today());
    }

    #[test]
    fn read_methods() {
        assert!(is_read_method("plots.list") && is_read_method("search.quick") && is_read_method("export.file") && is_read_method("burial.preview"));
        assert!(!is_read_method("burial.record") && !is_read_method("settings.set") && !is_read_method("plots.ensure") && !is_read_method("sample.load"));
    }
}
