//! Subscriptions per cemetery (migration 0012). The organisation is free;
//! every cemetery needs its own subscription to be written:
//!
//! * card: Barion checks the card first (a small amount is reserved and
//!   released at once, and the card is saved as a token), then 15 days of
//!   trial, then the period fee is charged with the token every month or
//!   year until the subscription is cancelled;
//! * transfer: no trial; a proforma at once, the cemetery works while it is
//!   open (the daily job locks it when unpaid after the grace period);
//! * demo: the sample cemetery, free.
//!
//! During a trial (and in the demo) data cannot be exported and prints carry
//! a "PRÓBAIDŐ" mark, so trials are not a way to get the register out free.

use chrono::Duration;
use lunda_core::auth::{Permission, Session};
use lunda_core::sql::{params, Connection, OptionalExtension};
use lunda_core::util::new_id;
use lunda_core::{CoreError, Result, Store};
use serde_json::{json, Value};

use crate::barion;
use crate::billing::{self, gross, hu_date, parse, period_end, today, unit_net, ymd};
use crate::mail;

pub const TRIAL_DAYS: i64 = 15;
/// Reserved on the card for the check, released right after (HUF).
pub fn verify_amount() -> i64 {
    std::env::var("BARION_VERIFY_HUF").ok().and_then(|v| v.trim().parse().ok()).filter(|v: &i64| *v > 0).unwrap_or(100)
}

#[derive(Debug, Clone)]
pub struct CemSub {
    pub cemetery_id: String,
    pub cemetery_name: String,
    pub plan: String,
    pub method: String,
    pub status: String,
    pub trial_ends: Option<String>,
    pub trial_over: bool,
    pub period_start: Option<String>,
    pub period_end: Option<String>,
    pub cancel_at_end: bool,
}

const SELECT: &str = "SELECT cemetery_id, cemetery_name, plan, payment_method, status, to_char(trial_ends_at AT TIME ZONE 'Europe/Budapest', 'YYYY-MM-DD'),
            coalesce(trial_ends_at <= now(), false), to_char(current_period_start, 'YYYY-MM-DD'), to_char(current_period_end, 'YYYY-MM-DD'), cancel_at_period_end
     FROM platform.cemetery_subscriptions";

fn map(r: &lunda_core::sql::Row) -> lunda_core::sql::Result<CemSub> {
    Ok(CemSub {
        cemetery_id: r.get(0)?,
        cemetery_name: r.get(1)?,
        plan: r.get(2)?,
        method: r.get(3)?,
        status: r.get(4)?,
        trial_ends: r.get(5)?,
        trial_over: r.get(6)?,
        period_start: r.get(7)?,
        period_end: r.get(8)?,
        cancel_at_end: r.get(9)?,
    })
}

pub fn list(conn: &Connection, tenant_id: &str) -> Result<Vec<CemSub>> {
    let mut st = conn.prepare(&format!("{SELECT} WHERE tenant_id = ?1 ORDER BY cemetery_name"))?;
    let rows = st.query_map(params![tenant_id], map)?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

pub fn get(conn: &Connection, tenant_id: &str, cemetery_id: &str) -> Result<Option<CemSub>> {
    Ok(conn.query_row(&format!("{SELECT} WHERE tenant_id = ?1 AND cemetery_id = ?2"), params![tenant_id, cemetery_id], map).optional()?)
}

/// The cemetery can be written: trial, paid, a running grace period, a
/// cancelled subscription until its end, the demo.
pub fn usable(sub: &CemSub) -> bool {
    match sub.status.as_str() {
        "trial" => sub.method == "card" || !sub.trial_over,
        "active" | "past_due" | "demo" => true,
        "canceled" => match (&sub.period_end, &sub.trial_ends) {
            (Some(end), _) => parse(end).map(|e| e > today()).unwrap_or(false),
            (None, Some(_)) => !sub.trial_over,
            _ => false,
        },
        _ => false,
    }
}

/// No export, watermarked prints: before anything was paid for the cemetery.
pub fn trial_like(sub: &CemSub) -> bool {
    matches!(sub.status.as_str(), "trial" | "demo" | "pending_card") || (sub.status == "canceled" && sub.period_end.is_none())
}

/// The tenant's cemeteries (in the schema the connection points at) that
/// cannot be written now: no subscription, or a lapsed one.
pub fn locked_cemeteries(store: &Store, tenant_id: &str) -> Result<Vec<String>> {
    let subs = list(&store.conn, tenant_id)?;
    let mut st = store.conn.prepare("SELECT id FROM cemeteries WHERE voided_at IS NULL")?;
    let ids: Vec<String> = st.query_map([], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?;
    Ok(ids.into_iter().filter(|id| !subs.iter().any(|s| &s.cemetery_id == id && usable(s))).collect())
}

/// Cemeteries in trial or demo (exports refused, prints marked).
pub fn trial_cemeteries(conn: &Connection, tenant_id: &str) -> Result<Vec<String>> {
    Ok(list(conn, tenant_id)?.into_iter().filter(trial_like).map(|s| s.cemetery_id).collect())
}

fn cemetery_name(store: &Store, cemetery_id: &str) -> Result<String> {
    store
        .conn
        .query_row("SELECT name FROM cemeteries WHERE id = ?1 AND voided_at IS NULL", params![cemetery_id], |r| r.get(0))
        .optional()?
        .ok_or_else(|| CoreError::not_found("cemetery", cemetery_id))
}

fn check_start(store: &Store, s: &Session, tenant_id: &str, cemetery_id: &str, plan: &str) -> Result<(String, Option<CemSub>)> {
    s.require(Permission::ManageSettings)?;
    if !matches!(plan, "monthly" | "yearly") {
        return Err(CoreError::new("bad_request", "plan required"));
    }
    billing::buyer(&store.conn, tenant_id)?; // billing details are required for the invoices
    let name = cemetery_name(store, cemetery_id)?;
    let old = get(&store.conn, tenant_id, cemetery_id)?;
    if let Some(o) = &old {
        if usable(o) && o.status != "demo" && o.status != "canceled" {
            return Err(CoreError::new("already_subscribed", "this cemetery already has a subscription"));
        }
    }
    Ok((name, old))
}

fn upsert(conn: &Connection, tenant_id: &str, cemetery_id: &str, name: &str, plan: &str, method: &str, status: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO platform.cemetery_subscriptions (tenant_id, cemetery_id, cemetery_name, plan, payment_method, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (tenant_id, cemetery_id) DO UPDATE SET cemetery_name = excluded.cemetery_name, plan = excluded.plan,
             payment_method = excluded.payment_method, status = excluded.status, cancel_at_period_end = false, failed_charges = 0,
             past_due_since = NULL, updated_at = now()",
        params![tenant_id, cemetery_id, name, plan, method, status],
    )?;
    Ok(())
}

/// The sample cemetery: free, as a demonstration.
pub fn register_demo(conn: &Connection, tenant_id: &str, cemetery_id: &str, name: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO platform.cemetery_subscriptions (tenant_id, cemetery_id, cemetery_name, plan, payment_method, status)
         VALUES (?1, ?2, ?3, 'monthly', 'demo', 'demo') ON CONFLICT DO NOTHING",
        params![tenant_id, cemetery_id, name],
    )?;
    Ok(())
}

/// Transfer: no trial, a proforma for the first period now; the cemetery works at once.
pub fn start_transfer(store: &Store, s: &Session, tenant_id: &str, cemetery_id: &str, plan: &str) -> Result<Value> {
    let (name, _) = check_start(store, s, tenant_id, cemetery_id, plan)?;
    // an unpaid proforma first: ordering again must not reopen a locked cemetery
    let unpaid: Option<String> = store
        .conn
        .query_row(
            "SELECT number FROM platform.invoices WHERE tenant_id = ?1 AND cemetery_id = ?2 AND kind = 'proforma' AND status = 'open' LIMIT 1",
            params![tenant_id, cemetery_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(number) = unpaid {
        return Err(CoreError::new("unpaid_proforma", "an earlier proforma is still unpaid").with("number", number));
    }
    let start = today();
    let end = period_end(start, plan);
    let t = lunda_core::db::tx(&store.conn)?;
    upsert(&t, tenant_id, cemetery_id, &name, plan, "transfer", "active")?;
    t.execute(
        "UPDATE platform.cemetery_subscriptions SET current_period_start = CAST(?3 AS DATE), current_period_end = CAST(?4 AS DATE)
         WHERE tenant_id = ?1 AND cemetery_id = ?2",
        params![tenant_id, cemetery_id, ymd(start), ymd(end)],
    )?;
    let inv = billing::issue(&t, tenant_id, true, &format!("TEMISZ előfizetés – {name}"), 1, unit_net(plan), (start, end), "transfer", None)?;
    t.execute("UPDATE platform.invoices SET cemetery_id = ?2 WHERE id = ?1", params![inv, cemetery_id])?;
    t.commit()?;
    let _ = lunda_core::audit::record_simple(&store.conn, s, "subscription", cemetery_id, "subscribe", Some(&format!("{plan}, transfer")));
    Ok(json!({ "ok": true, "invoiceId": inv }))
}

/// Card: Barion checks the card (reserves a small amount, saves the card as a
/// token). Returns the gateway address; the trial starts when the check succeeds.
pub fn start_card(store: &Store, s: &Session, tenant_id: &str, cemetery_id: &str, plan: &str, app_url: &str, site_url: &str) -> Result<Value> {
    let (name, _) = check_start(store, s, tenant_id, cemetery_id, plan)?;
    let cfg = barion::config().ok_or_else(|| CoreError::new("card_unavailable", "card payment is not set up"))?;
    let email: String = store.conn.query_row("SELECT email FROM platform.billing_details WHERE tenant_id = ?1", params![tenant_id], |r| r.get(0))?;
    let request_id = new_id();
    let recurrence_id = new_id();
    let amount = verify_amount();
    let body = json!({
        "POSKey": cfg.pos_key,
        "PaymentType": "Reservation",
        "ReservationPeriod": "1.00:00:00",
        "GuestCheckOut": true,
        "FundingSources": ["BankCard"],
        "PaymentRequestId": request_id,
        "PayerHint": email,
        "RedirectUrl": format!("{app_url}/#kartya={request_id}"),
        "CallbackUrl": format!("{site_url}/api/barion/callback"),
        "Locale": "hu-HU",
        "Currency": "HUF",
        "InitiateRecurrence": true,
        "RecurrenceId": recurrence_id,
        "RecurrenceType": "MerchantInitiatedPayment",
        "OrderNumber": format!("T-{}", &cemetery_id[..8.min(cemetery_id.len())]),
        "Transactions": [{
            "POSTransactionId": format!("check-{request_id}"),
            "Payee": cfg.payee,
            "Total": amount,
            "Comment": "Kártyaellenőrzés – a zárolt összeg azonnal felszabadul",
            "Items": [{
                "Name": "Kártyaellenőrzés",
                "Description": format!("TEMISZ előfizetés – {name}: a kártya ellenőrzése, a zárolás azonnal feloldódik"),
                "Quantity": 1, "Unit": "db", "UnitPrice": amount, "ItemTotal": amount, "SKU": "TEMISZ-CHECK",
            }],
        }],
    });
    let (payment_id, gateway, state) = barion::start_payment(&cfg, &body)?;
    let t = lunda_core::db::tx(&store.conn)?;
    // a renewed subscription keeps a used trial (settle_card_check charges at once)
    upsert(&t, tenant_id, cemetery_id, &name, plan, "card", "pending_card")?;
    t.execute(
        "UPDATE platform.cemetery_subscriptions SET recurrence_id = ?3 WHERE tenant_id = ?1 AND cemetery_id = ?2",
        params![tenant_id, cemetery_id, recurrence_id],
    )?;
    t.execute(
        "INSERT INTO platform.card_payments (payment_id, request_id, tenant_id, invoice_id, gross_huf, status, kind, cemetery_id)
         VALUES (?1, ?2, ?3, '', ?4, ?5, 'card_check', ?6)",
        params![payment_id, request_id, tenant_id, amount, state, cemetery_id],
    )?;
    t.commit()?;
    Ok(json!({ "gatewayUrl": gateway, "paymentId": payment_id }))
}

/// Barion reported on a card check: when the amount is reserved, it is released
/// at once and the trial starts (or, after a used trial, the period is charged).
pub fn settle_card_check(store: &Store, tenant_id: &str, payment_id: &str, cemetery_id: &str) -> Result<String> {
    let cfg = barion::config().ok_or_else(|| CoreError::new("card_unavailable", "card payment is not set up"))?;
    let v = barion::call(&cfg, &format!("/v4/payment/{payment_id}/paymentstate"), None)?;
    let state = v["Status"].as_str().unwrap_or("Unknown").to_string();
    let conn = &store.conn;
    conn.execute("UPDATE platform.card_payments SET status = ?2, updated_at = now() WHERE payment_id = ?1", params![payment_id, state])?;
    if !matches!(state.as_str(), "Reserved" | "Succeeded" | "PartiallySucceeded") {
        return Ok(state);
    }
    // release the reserved amount: finishing the reservation with 0 charges nothing
    if state == "Reserved" {
        let tx_id = v["Transactions"]
            .as_array()
            .and_then(|t| t.iter().find(|x| x["POSTransactionId"].as_str().is_some_and(|p| p.starts_with("check-"))))
            .and_then(|x| x["TransactionId"].as_str())
            .unwrap_or("")
            .to_string();
        barion::call(
            &cfg,
            "/v2/Payment/FinishReservation",
            Some(&json!({ "POSKey": cfg.pos_key, "PaymentId": payment_id, "Transactions": [{ "TransactionId": tx_id, "Total": 0 }] })),
        )?;
    }
    let trace = v["TraceId"].as_str().map(String::from);
    let had_trial: bool = conn
        .query_row(
            "SELECT trial_ends_at IS NOT NULL FROM platform.cemetery_subscriptions WHERE tenant_id = ?1 AND cemetery_id = ?2",
            params![tenant_id, cemetery_id],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or(false);
    let n = conn.execute(
        "UPDATE platform.cemetery_subscriptions SET status = CASE WHEN ?3 THEN 'active' ELSE 'trial' END,
             trial_ends_at = CASE WHEN ?3 THEN trial_ends_at ELSE now() + make_interval(days => ?4) END,
             trace_id = ?5, updated_at = now()
         WHERE tenant_id = ?1 AND cemetery_id = ?2 AND status = 'pending_card'",
        params![tenant_id, cemetery_id, had_trial, TRIAL_DAYS as i32, trace],
    )?;
    conn.execute("UPDATE platform.card_payments SET status = 'Succeeded' WHERE payment_id = ?1", params![payment_id])?;
    if n == 1 && had_trial {
        // the trial was used before: the first period is due now
        charge(store, tenant_id, cemetery_id)?;
    }
    Ok("Succeeded".into())
}

/// Charges the period fee with the saved card. On success the final invoice is
/// issued and the period moves on; a failure is retried by the daily job.
pub fn charge(store: &Store, tenant_id: &str, cemetery_id: &str) -> Result<bool> {
    let conn = &store.conn;
    let Some(sub) = get(conn, tenant_id, cemetery_id)? else { return Ok(false) };
    let (recurrence, trace): (Option<String>, Option<String>) = conn.query_row(
        "SELECT recurrence_id, trace_id FROM platform.cemetery_subscriptions WHERE tenant_id = ?1 AND cemetery_id = ?2",
        params![tenant_id, cemetery_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (Some(recurrence), Some(cfg)) = (recurrence, barion::config()) else { return Ok(false) };
    let start = sub.period_end.as_deref().map(parse).transpose()?.map(|e| e.max(today())).unwrap_or_else(today);
    let end = period_end(start, &sub.plan);
    let net = unit_net(&sub.plan);
    let total = gross(net);
    let request_id = new_id();
    let body = json!({
        "POSKey": cfg.pos_key,
        "PaymentType": "Immediate",
        "GuestCheckOut": true,
        "FundingSources": ["BankCard"],
        "PaymentRequestId": request_id,
        "RecurrenceId": recurrence,
        "InitiateRecurrence": false,
        "RecurrenceType": "MerchantInitiatedPayment",
        "TraceId": trace,
        "ChallengePreference": "NoChallengeNeeded",
        "RedirectUrl": "https://www.temisz.hu/app/",
        "CallbackUrl": "https://www.temisz.hu/api/barion/callback",
        "Locale": "hu-HU",
        "Currency": "HUF",
        "OrderNumber": format!("T-{}", &cemetery_id[..8.min(cemetery_id.len())]),
        "Transactions": [{
            "POSTransactionId": format!("renew-{request_id}"),
            "Payee": cfg.payee,
            "Total": total,
            "Comment": format!("TEMISZ előfizetés – {}", sub.cemetery_name),
            "Items": [{
                "Name": "TEMISZ előfizetés",
                "Description": format!("{} ({} – {})", sub.cemetery_name, hu_date(start), hu_date(end - Duration::days(1))),
                "Quantity": 1, "Unit": "db", "UnitPrice": total, "ItemTotal": total, "SKU": "TEMISZ-SUB",
            }],
        }],
    });
    let outcome = barion::call(&cfg, "/v2/Payment/Start", Some(&body));
    let (pid, status) = match &outcome {
        Ok(v) => (v["PaymentId"].as_str().unwrap_or("").to_string(), v["Status"].as_str().unwrap_or("Unknown").to_string()),
        Err(_) => (String::new(), "Failed".to_string()),
    };
    if !pid.is_empty() {
        conn.execute(
            "INSERT INTO platform.card_payments (payment_id, request_id, tenant_id, invoice_id, gross_huf, status, kind, cemetery_id)
             VALUES (?1, ?2, ?3, '', ?4, ?5, 'renewal', ?6) ON CONFLICT (payment_id) DO NOTHING",
            params![pid, request_id, tenant_id, total, status, cemetery_id],
        )?;
    }
    if status == "Succeeded" {
        let t = lunda_core::db::tx(conn)?;
        let inv = billing::issue(&t, tenant_id, false, &format!("TEMISZ előfizetés – {}", sub.cemetery_name), 1, net, (start, end), "card", None)?;
        t.execute("UPDATE platform.invoices SET cemetery_id = ?2 WHERE id = ?1", params![inv, cemetery_id])?;
        t.execute(
            "UPDATE platform.cemetery_subscriptions SET status = 'active', current_period_start = CAST(?3 AS DATE), current_period_end = CAST(?4 AS DATE),
                 failed_charges = 0, past_due_since = NULL, updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2",
            params![tenant_id, cemetery_id, ymd(start), ymd(end)],
        )?;
        t.commit()?;
        return Ok(true);
    }
    conn.execute(
        "UPDATE platform.cemetery_subscriptions SET status = 'past_due', failed_charges = failed_charges + 1,
             past_due_since = coalesce(past_due_since, CAST(?3 AS DATE)), updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2",
        params![tenant_id, cemetery_id, ymd(today())],
    )?;
    Ok(false)
}

/// Cancels at the end of the trial or the paid period (nothing more is charged).
pub fn cancel(store: &Store, s: &Session, tenant_id: &str, cemetery_id: &str, resume: bool) -> Result<()> {
    s.require(Permission::ManageSettings)?;
    let sub = get(&store.conn, tenant_id, cemetery_id)?.ok_or_else(|| CoreError::not_found("subscription", cemetery_id))?;
    if sub.method == "demo" {
        return Err(CoreError::new("bad_request", "the sample cemetery has no subscription"));
    }
    if resume {
        if !usable(&sub) {
            return Err(CoreError::new("subscription_lapsed", "subscribe again"));
        }
        let status = if sub.period_end.is_some() { "active" } else { "trial" };
        store.conn.execute(
            "UPDATE platform.cemetery_subscriptions SET status = ?3, cancel_at_period_end = false, updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2 AND status = 'canceled'",
            params![tenant_id, cemetery_id, status],
        )?;
    } else {
        // a card check not finished: nothing to keep
        let status = if sub.status == "pending_card" { "lapsed" } else { "canceled" };
        store.conn.execute(
            "UPDATE platform.cemetery_subscriptions SET status = ?3, cancel_at_period_end = true, updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2",
            params![tenant_id, cemetery_id, status],
        )?;
    }
    let _ = lunda_core::audit::record_simple(&store.conn, s, "subscription", cemetery_id, if resume { "resume" } else { "cancel" }, None);
    Ok(())
}

/// A transfer proforma of a cemetery was paid: the period starts or moves on.
pub fn proforma_paid(conn: &Connection, tenant_id: &str, cemetery_id: Option<&str>, start: &str, end: &str) -> Result<()> {
    let filter = if cemetery_id.is_some() { "AND cemetery_id = ?4" } else { "AND payment_method = 'transfer'" };
    conn.execute(
        &format!(
            "UPDATE platform.cemetery_subscriptions SET status = CASE WHEN status = 'canceled' THEN status ELSE 'active' END,
                 current_period_start = CASE WHEN current_period_end IS NULL OR CAST(?2 AS DATE) >= current_period_end THEN CAST(?2 AS DATE) ELSE current_period_start END,
                 current_period_end = GREATEST(coalesce(current_period_end, CAST(?3 AS DATE)), CAST(?3 AS DATE)),
                 past_due_since = NULL, updated_at = now()
             WHERE tenant_id = ?1 {filter}"
        ),
        params![tenant_id, start, end, cemetery_id.unwrap_or("")],
    )?;
    Ok(())
}

/// For the Előfizetés page.
pub fn overview(store: &Store, tenant_id: &str) -> Result<Value> {
    let subs = list(&store.conn, tenant_id)?;
    let mut st = store.conn.prepare("SELECT id, name FROM cemeteries WHERE voided_at IS NULL ORDER BY name COLLATE \"hu-x-icu\"")?;
    let cems: Vec<(String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<std::result::Result<_, _>>()?;
    let rows: Vec<Value> = cems
        .iter()
        .map(|(id, name)| match subs.iter().find(|s| &s.cemetery_id == id) {
            Some(s) => json!({
                "cemeteryId": id, "name": name, "plan": s.plan, "method": s.method, "status": if usable(s) || s.status == "pending_card" { s.status.clone() } else { "lapsed".into() },
                "trialEndsAt": s.trial_ends, "periodEnd": s.period_end, "cancelAtPeriodEnd": s.cancel_at_end, "trialLike": trial_like(s),
            }),
            None => json!({ "cemeteryId": id, "name": name, "status": "none", "trialLike": false }),
        })
        .collect();
    Ok(json!({ "cemeteries": rows, "trialDays": TRIAL_DAYS, "verifyHuf": verify_amount() }))
}

// ---------------------------------------------------------------- daily job

/// Trial reminders, charges at trial and period end, retries, transfer
/// renewals, lapsing. Runs once a day.
#[allow(clippy::type_complexity)]
pub fn daily(store: &Store, base_url: &str) -> Result<Value> {
    let conn = &store.conn;
    let mut done = serde_json::Map::new();
    let rows: Vec<(String, String, String, String, bool, Option<String>, bool, Option<String>, i32, Option<String>)> = conn
        .prepare(
            "SELECT c.tenant_id, c.cemetery_id, c.status, c.payment_method, c.cancel_at_period_end, to_char(c.current_period_end, 'YYYY-MM-DD'),
                    coalesce(c.trial_ends_at <= now(), false), to_char(c.trial_ends_at AT TIME ZONE 'Europe/Budapest', 'YYYY-MM-DD'),
                    c.failed_charges, to_char(c.past_due_since, 'YYYY-MM-DD')
             FROM platform.cemetery_subscriptions c JOIN platform.tenants t ON t.id = c.tenant_id
             WHERE t.status <> 'closed' AND c.status IN ('trial', 'active', 'past_due', 'canceled')",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?, r.get(9)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let (mut charged, mut failed, mut lapsed, mut reminded, mut proformas) = (0, 0, 0, 0, 0);
    let today = today();
    for (tenant, cem, status, method, cancel, end, trial_over, trial_ends, _fails, past_due_since) in rows {
        let schema: String = conn.query_row("SELECT schema_name FROM platform.tenants WHERE id = ?1", params![tenant], |r| r.get(0))?;
        conn.use_schema(&schema)?;
        let Some(sub) = get(conn, &tenant, &cem)? else { continue };
        let end_date = end.as_deref().map(parse).transpose()?;
        let lapse = |why: &str| -> Result<()> {
            conn.execute(
                "UPDATE platform.cemetery_subscriptions SET status = 'lapsed', updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2",
                params![tenant, cem],
            )?;
            if billing::notify_once(conn, &tenant, &format!("lapsed:{cem}:{}", ymd(today)))? {
                let (to, _) = billing::tenant_email(conn, &tenant)?;
                mail::send(
                    conn,
                    &to,
                    &format!("A(z) {} csak olvasható lett", sub.cemetery_name),
                    &format!(
                        "Jó napot kívánunk!\n\n{why} A(z) {} adatai megmaradtak és olvashatók; a további munkához fizessen elő újra a Beállítások › Előfizetés oldalon:\n{base_url}/{}",
                        sub.cemetery_name,
                        mail::FOOTER
                    ),
                    "lapsed",
                )?;
            }
            Ok(())
        };
        match (status.as_str(), method.as_str()) {
            // card trial: a reminder 3 days before, then the first charge
            ("trial", "card") => {
                if trial_over {
                    if charge(store, &tenant, &cem)? {
                        charged += 1;
                    } else {
                        failed += 1;
                    }
                } else if let Some(te) = &trial_ends {
                    let left = (parse(te)? - today).num_days();
                    if left <= 3 && billing::notify_once(conn, &tenant, &format!("trial3:{cem}"))? {
                        let (to, _) = billing::tenant_email(conn, &tenant)?;
                        mail::send(
                            conn,
                            &to,
                            &format!("{left} nap múlva indul a(z) {} előfizetése", sub.cemetery_name),
                            &format!(
                                "Jó napot kívánunk!\n\nA(z) {} próbaideje {} napján lejár, ekkor a mentett kártyáról levonjuk az első időszak díját ({} Ft). Ha nem szeretné folytatni, mondja le a Beállítások › Előfizetés oldalon:\n{base_url}/{}",
                                sub.cemetery_name,
                                hu_date(parse(te)?),
                                billing::fmt_huf(gross(unit_net(&sub.plan))),
                                mail::FOOTER
                            ),
                            "trial_reminder",
                        )?;
                        reminded += 1;
                    }
                }
            }
            // an older trial without a card: locked when it is over
            ("trial", _) if trial_over => {
                lapse("A próbaidő lejárt.")?;
                lapsed += 1;
            }
            ("active", "card") if end_date.is_some_and(|e| e <= today) && !cancel => {
                if charge(store, &tenant, &cem)? {
                    charged += 1;
                } else {
                    failed += 1;
                }
            }
            ("past_due", "card") => {
                let since = past_due_since.as_deref().map(parse).transpose()?.unwrap_or(today);
                if (today - since).num_days() > billing::OVERDUE_GRACE_DAYS {
                    lapse("A kártyás terhelés többszöri próbálkozásra sem sikerült.")?;
                    lapsed += 1;
                } else if charge(store, &tenant, &cem)? {
                    charged += 1;
                } else {
                    failed += 1;
                }
            }
            ("active", "transfer") if !cancel => {
                // the next period's proforma ahead of time; an unpaid one locks after the grace period
                if let Some(e) = end_date {
                    if e <= today + Duration::days(billing::RENEWAL_NOTICE_DAYS) {
                        let exists: bool = conn.query_row(
                            "SELECT EXISTS (SELECT 1 FROM platform.invoices WHERE tenant_id = ?1 AND cemetery_id = ?2 AND kind = 'proforma'
                                            AND period_start = CAST(?3 AS DATE) AND status <> 'void')",
                            params![tenant, cem, ymd(e)],
                            |r| r.get(0),
                        )?;
                        if !exists {
                            let inv = billing::issue(
                                conn,
                                &tenant,
                                true,
                                &format!("TEMISZ előfizetés – {} – következő időszak", sub.cemetery_name),
                                1,
                                unit_net(&sub.plan),
                                (e, period_end(e, &sub.plan)),
                                "transfer",
                                None,
                            )?;
                            conn.execute("UPDATE platform.invoices SET cemetery_id = ?2 WHERE id = ?1", params![inv, cem])?;
                            proformas += 1;
                        }
                    }
                }
                let overdue: bool = conn.query_row(
                    "SELECT EXISTS (SELECT 1 FROM platform.invoices WHERE tenant_id = ?1 AND cemetery_id = ?2 AND kind = 'proforma' AND status = 'open'
                                    AND due_date + CAST(?3 AS INTEGER) < CAST(?4 AS DATE))",
                    params![tenant, cem, billing::OVERDUE_GRACE_DAYS as i32, ymd(today)],
                    |r| r.get(0),
                )?;
                if overdue {
                    lapse("A díjbekérő fizetési határideje lejárt.")?;
                    lapsed += 1;
                }
            }
            ("canceled", _) | ("active", _) if !usable(&sub) || (cancel && end_date.is_some_and(|e| e <= today)) => {
                lapse("Az előfizetés lemondás miatt véget ért.")?;
                lapsed += 1;
            }
            _ => {}
        }
    }
    done.insert("charged".into(), json!(charged));
    done.insert("chargeFailed".into(), json!(failed));
    done.insert("lapsed".into(), json!(lapsed));
    done.insert("trialReminders".into(), json!(reminded));
    done.insert("renewalProformas".into(), json!(proformas));
    Ok(Value::Object(done))
}
