//! Subscriptions per cemetery (migrations 0012, 0014). The organisation is
//! free; every cemetery needs its own subscription to be written:
//!
//! * card: Barion checks the card first (a small amount is reserved and
//!   released at once, and the card is saved as a token), then 15 days of
//!   trial, then the period fee is charged with the token every month or
//!   year until the subscription is cancelled;
//! * transfer: no trial; a proforma at once, the cemetery works while it is
//!   open (the daily job locks it when unpaid after the grace period);
//! * demo: the sample cemetery, free for 30 days.
//!
//! Until the first payment arrives (`paid_at`) no data leaves the cemetery
//! (exports, the authenticated books); prints of a trial or demo cemetery
//! carry a "PRÓBAIDŐ" mark. An organisation whose earlier trial ended unpaid
//! gets no second trial: the first period is charged at once.

#![allow(clippy::type_complexity)]

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
pub const DEMO_DAYS: i64 = 30;

/// Reserved on the card for the check, released right after (HUF).
pub fn verify_amount() -> i64 {
    std::env::var("BARION_VERIFY_HUF").ok().and_then(|v| v.trim().parse().ok()).filter(|v: &i64| *v > 0).unwrap_or(100)
}

/// Card subscriptions need Barion's token payments, which Barion enables per
/// shop on request: offered only when BARION_RECURRING says it is enabled.
pub fn card_enabled() -> bool {
    barion::config().is_some()
        && std::env::var("BARION_RECURRING").map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "on" | "yes")).unwrap_or(false)
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
    /// the first payment arrived (exports allowed from then on)
    pub paid: bool,
}

const SELECT: &str = "SELECT cemetery_id, cemetery_name, plan, payment_method, status, to_char(trial_ends_at AT TIME ZONE 'Europe/Budapest', 'YYYY-MM-DD'),
            coalesce(trial_ends_at <= now(), false), to_char(current_period_start, 'YYYY-MM-DD'), to_char(current_period_end, 'YYYY-MM-DD'), cancel_at_period_end,
            paid_at IS NOT NULL
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
        paid: r.get(10)?,
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

fn paid_period_left(sub: &CemSub) -> bool {
    sub.period_end.as_deref().and_then(|e| parse(e).ok()).is_some_and(|e| e > today())
}

/// The cemetery can be written: trial, paid, a running grace period, a
/// cancelled subscription until its end, the demo while it lasts.
pub fn usable(sub: &CemSub) -> bool {
    match sub.status.as_str() {
        "trial" => sub.method == "card" || !sub.trial_over,
        "active" | "past_due" => true,
        "demo" => !sub.trial_over,
        "canceled" => paid_period_left(sub) || (sub.period_end.is_none() && sub.trial_ends.is_some() && !sub.trial_over),
        _ => false,
    }
}

/// Prints carry the "PRÓBAIDŐ" mark: a trial or the demo, nothing paid yet.
pub fn marked(sub: &CemSub) -> bool {
    !sub.paid && (matches!(sub.status.as_str(), "trial" | "demo") || (sub.status == "canceled" && sub.period_end.is_none()))
}

fn cemeteries(store: &Store) -> Result<Vec<(String, String)>> {
    let mut st = store.conn.prepare("SELECT id, name FROM cemeteries WHERE voided_at IS NULL ORDER BY name COLLATE \"hu-x-icu\"")?;
    let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

/// What the tenant's cemeteries (in the schema the connection points at) allow now.
pub struct Limits {
    /// no usable subscription: read only
    pub locked: Vec<String>,
    /// nothing paid yet: no export, no authenticated books
    pub unpaid: Vec<String>,
    /// trial or demo: prints are marked
    pub marked: Vec<String>,
    /// at least one cemetery was paid for (organisation-wide exports allowed)
    pub any_paid: bool,
}

pub fn limits(store: &Store, tenant_id: &str) -> Result<Limits> {
    let subs = list(&store.conn, tenant_id)?;
    let mut l = Limits { locked: vec![], unpaid: vec![], marked: vec![], any_paid: false };
    for (id, _) in cemeteries(store)? {
        match subs.iter().find(|s| s.cemetery_id == id) {
            Some(s) => {
                if !usable(s) {
                    l.locked.push(id.clone());
                }
                if s.paid {
                    l.any_paid = true;
                } else {
                    l.unpaid.push(id.clone());
                }
                if marked(s) {
                    l.marked.push(id);
                }
            }
            None => {
                l.locked.push(id.clone());
                l.unpaid.push(id);
            }
        }
    }
    Ok(l)
}

pub fn locked_cemeteries(store: &Store, tenant_id: &str) -> Result<Vec<String>> {
    Ok(limits(store, tenant_id)?.locked)
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
        if usable(o) && !matches!(o.status.as_str(), "demo" | "canceled") && !(o.status == "trial" && o.method != "card") {
            return Err(CoreError::new("already_subscribed", "this cemetery already has a subscription"));
        }
    }
    Ok((name, old))
}

/// The organisation (or another organisation of the same person) already
/// used a cemetery without ever paying for it: a trial that ran out, an
/// unpaid order that lapsed or was cancelled. No second free period then:
/// a card is charged at once, a transfer order works only after payment.
fn unpaid_history(conn: &Connection, tenant_id: &str) -> Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS (
            SELECT 1 FROM platform.cemetery_subscriptions c
            WHERE c.payment_method <> 'demo' AND c.paid_at IS NULL
              AND (c.status = 'lapsed'
                   OR (c.status = 'canceled' AND (c.trial_ends_at IS NULL OR c.trial_ends_at <= now()))
                   OR (c.status = 'trial' AND c.payment_method <> 'card' AND c.trial_ends_at <= now()))
              AND (c.tenant_id = ?1 OR c.tenant_id IN (
                    SELECT o.tenant_id FROM platform.accounts o
                    WHERE lower(o.email) IN (SELECT lower(m.email) FROM platform.accounts m WHERE m.tenant_id = ?1))))",
        params![tenant_id],
        |r| r.get(0),
    )?)
}

/// The sample cemetery: a free demonstration for 30 days.
pub fn register_demo(conn: &Connection, tenant_id: &str, cemetery_id: &str, name: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO platform.cemetery_subscriptions (tenant_id, cemetery_id, cemetery_name, plan, payment_method, status, trial_ends_at)
         VALUES (?1, ?2, ?3, 'monthly', 'demo', 'demo', now() + make_interval(days => ?4)) ON CONFLICT DO NOTHING",
        params![tenant_id, cemetery_id, name, DEMO_DAYS as i32],
    )?;
    Ok(())
}

/// The cemetery was archived: its subscription ends now, nothing more is charged.
pub fn cemetery_archived(conn: &Connection, tenant_id: &str, cemetery_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE platform.cemetery_subscriptions SET status = 'lapsed', cancel_at_period_end = true, updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2",
        params![tenant_id, cemetery_id],
    )?;
    conn.execute(
        "UPDATE platform.invoices SET status = 'void' WHERE tenant_id = ?1 AND cemetery_id = ?2 AND kind = 'proforma' AND status = 'open'",
        params![tenant_id, cemetery_id],
    )?;
    Ok(())
}

/// Transfer: no trial, a proforma for the first period now. The cemetery works
/// at once, except after an earlier unpaid use: then only once the proforma is paid.
pub fn start_transfer(store: &Store, s: &Session, tenant_id: &str, cemetery_id: &str, plan: &str) -> Result<Value> {
    let (name, old) = check_start(store, s, tenant_id, cemetery_id, plan)?;
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
    // a period already paid for (a cancelled subscription) is not billed twice
    let start = old.as_ref().filter(|o| paid_period_left(o)).and_then(|o| o.period_end.as_deref().and_then(|e| parse(e).ok())).unwrap_or_else(today);
    let end = period_end(start, plan);
    let prepay = !old.as_ref().is_some_and(paid_period_left) && unpaid_history(&store.conn, tenant_id)?;
    let status = if prepay { "lapsed" } else { "active" };
    let t = lunda_core::db::tx(&store.conn)?;
    // the period end moves on only when a payment arrives: until then it is the start
    t.execute(
        "INSERT INTO platform.cemetery_subscriptions (tenant_id, cemetery_id, cemetery_name, plan, payment_method, status, current_period_start, current_period_end)
         VALUES (?1, ?2, ?3, ?4, 'transfer', ?6, CAST(?5 AS DATE), CAST(?5 AS DATE))
         ON CONFLICT (tenant_id, cemetery_id) DO UPDATE SET cemetery_name = excluded.cemetery_name, plan = excluded.plan, payment_method = 'transfer',
             status = excluded.status, cancel_at_period_end = false, failed_charges = 0, past_due_since = NULL, recurrence_id = NULL, updated_at = now(),
             current_period_start = coalesce(platform.cemetery_subscriptions.current_period_start, excluded.current_period_start),
             current_period_end = GREATEST(coalesce(platform.cemetery_subscriptions.current_period_end, excluded.current_period_start), excluded.current_period_start)",
        params![tenant_id, cemetery_id, name, plan, ymd(start), status],
    )?;
    let inv = billing::issue(&t, tenant_id, true, &format!("TEMISZ előfizetés – {name}"), 1, unit_net(plan), (start, end), "transfer", None)?;
    t.execute("UPDATE platform.invoices SET cemetery_id = ?2 WHERE id = ?1", params![inv, cemetery_id])?;
    t.commit()?;
    let _ = lunda_core::audit::record_simple(&store.conn, s, "subscription", cemetery_id, "subscribe", Some(&format!("{plan}, transfer")));
    Ok(json!({ "ok": true, "invoiceId": inv, "prepay": prepay }))
}

/// Card: Barion checks the card (reserves a small amount, saves the card as a
/// token). Returns the gateway address. The subscription itself changes only
/// when the check succeeds: an abandoned check leaves everything as it was.
pub fn start_card(store: &Store, s: &Session, tenant_id: &str, cemetery_id: &str, plan: &str, app_url: &str, site_url: &str) -> Result<Value> {
    let (name, _) = check_start(store, s, tenant_id, cemetery_id, plan)?;
    if !card_enabled() {
        return Err(CoreError::new("card_unavailable", "card subscriptions are not enabled"));
    }
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
    store.conn.execute(
        "INSERT INTO platform.card_payments (payment_id, request_id, tenant_id, invoice_id, gross_huf, status, kind, cemetery_id, plan, recurrence_id)
         VALUES (?1, ?2, ?3, '', ?4, ?5, 'card_check', ?6, ?7, ?8)",
        params![payment_id, request_id, tenant_id, amount, state, cemetery_id, plan, recurrence_id],
    )?;
    let _ = lunda_core::audit::record_simple(&store.conn, s, "subscription", cemetery_id, "card_check_started", Some(plan));
    Ok(json!({ "gatewayUrl": gateway, "paymentId": payment_id }))
}

/// Barion reported on a card check. When the amount is reserved it is released
/// at once, and (exactly once) the subscription becomes a card subscription:
/// a paid period or a running trial goes on, a new cemetery starts its trial,
/// and after a used-up trial the first period is charged now.
/// The connection must point at the tenant's schema.
pub fn settle_card_check(store: &Store, tenant_id: &str, payment_id: &str) -> Result<String> {
    let cfg = barion::config().ok_or_else(|| CoreError::new("card_unavailable", "card payment is not set up"))?;
    let conn = &store.conn;
    let (cemetery_id, plan, recurrence): (String, String, String) = conn.query_row(
        "SELECT coalesce(cemetery_id, ''), coalesce(plan, 'monthly'), coalesce(recurrence_id, '') FROM platform.card_payments WHERE payment_id = ?1",
        params![payment_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let v = barion::call(&cfg, &format!("/v4/payment/{payment_id}/paymentstate"), None)?;
    let state = v["Status"].as_str().unwrap_or("Unknown").to_string();
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
    // once: a second callback or the browser's return changes nothing
    let claimed = conn.execute(
        "UPDATE platform.card_payments SET status = 'Succeeded', settled_at = now() WHERE payment_id = ?1 AND settled_at IS NULL",
        params![payment_id],
    )?;
    if claimed == 0 {
        return Ok("Succeeded".into());
    }
    let trace = v["TraceId"].as_str().map(String::from);
    let name = cemetery_name(store, &cemetery_id)?;
    let old = get(conn, tenant_id, &cemetery_id)?;
    let keeps_period = old.as_ref().is_some_and(paid_period_left);
    let trial_runs = old.as_ref().is_some_and(|o| o.method != "demo" && o.trial_ends.is_some() && !o.trial_over && o.period_end.is_none());
    let charge_now =
        !keeps_period && !trial_runs && (old.as_ref().is_some_and(|o| o.method != "demo" && o.trial_ends.is_some()) || unpaid_history(conn, tenant_id)?);
    // 'period': the paid period goes on; 'keep': the running trial goes on; 'new': a fresh trial; 'none': charged below
    let mode = if keeps_period {
        "period"
    } else if trial_runs {
        "keep"
    } else if charge_now {
        "none"
    } else {
        "new"
    };
    conn.execute(
        "INSERT INTO platform.cemetery_subscriptions (tenant_id, cemetery_id, cemetery_name, plan, payment_method, status, trial_ends_at, recurrence_id, trace_id)
         VALUES (?1, ?2, ?3, ?4, 'card', CASE WHEN ?5 = 'new' THEN 'trial' ELSE 'active' END,
                 CASE WHEN ?5 = 'new' THEN now() + make_interval(days => ?6) END, ?7, ?8)
         ON CONFLICT (tenant_id, cemetery_id) DO UPDATE SET cemetery_name = excluded.cemetery_name, plan = excluded.plan, payment_method = 'card',
             recurrence_id = excluded.recurrence_id, trace_id = excluded.trace_id, cancel_at_period_end = false, failed_charges = 0, past_due_since = NULL,
             updated_at = now(),
             status = CASE WHEN ?5 IN ('new', 'keep') THEN 'trial' ELSE 'active' END,
             trial_ends_at = CASE WHEN ?5 = 'new' THEN excluded.trial_ends_at ELSE platform.cemetery_subscriptions.trial_ends_at END,
             current_period_start = CASE WHEN ?5 = 'period' THEN platform.cemetery_subscriptions.current_period_start END,
             current_period_end = CASE WHEN ?5 = 'period' THEN platform.cemetery_subscriptions.current_period_end END",
        params![tenant_id, cemetery_id, name, plan, mode, TRIAL_DAYS as i32, recurrence, trace],
    )?;
    if charge_now && charge(store, tenant_id, &cemetery_id)? != Charge::Paid {
        // no free period this time: without the first payment the cemetery stays locked
        conn.execute(
            "UPDATE platform.cemetery_subscriptions SET status = 'lapsed', updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2 AND paid_at IS NULL",
            params![tenant_id, cemetery_id],
        )?;
        return Ok("ChargeFailed".into());
    }
    Ok("Succeeded".into())
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Charge {
    Paid,
    Failed,
    /// Barion has not decided yet: asked again later, nothing is charged twice
    Pending,
}

const FAILED_STATES: &[&str] = &["Canceled", "Failed", "Expired", "Rejected"];

/// Charges the next period with the saved card. An earlier charge still in
/// progress is settled first, so a period is never charged twice.
pub fn charge(store: &Store, tenant_id: &str, cemetery_id: &str) -> Result<Charge> {
    let conn = &store.conn;
    let Some(sub) = get(conn, tenant_id, cemetery_id)? else { return Ok(Charge::Failed) };
    let open: Vec<String> = conn
        .prepare(
            "SELECT payment_id FROM platform.card_payments WHERE tenant_id = ?1 AND cemetery_id = ?2 AND kind = 'renewal' AND settled_at IS NULL
               AND status NOT IN ('Canceled', 'Failed', 'Expired', 'Rejected') AND created_at > now() - interval '5 days'",
        )?
        .query_map(params![tenant_id, cemetery_id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut pending = false;
    for pid in open {
        match settle_renewal(store, &pid, None)? {
            Charge::Paid => return Ok(Charge::Paid),
            Charge::Pending => pending = true,
            Charge::Failed => {}
        }
    }
    if pending {
        return Ok(Charge::Pending);
    }
    let (recurrence, trace): (Option<String>, Option<String>) = conn.query_row(
        "SELECT recurrence_id, trace_id FROM platform.cemetery_subscriptions WHERE tenant_id = ?1 AND cemetery_id = ?2",
        params![tenant_id, cemetery_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let (Some(recurrence), Some(cfg)) = (recurrence.filter(|r| !r.is_empty()), barion::config()) else {
        failed_charge(conn, tenant_id, cemetery_id)?;
        return Ok(Charge::Failed);
    };
    let start = sub.period_end.as_deref().map(parse).transpose()?.map(|e| e.max(today())).unwrap_or_else(today);
    let end = period_end(start, &sub.plan);
    let total = gross(unit_net(&sub.plan));
    let request_id = new_id();
    let site = std::env::var("PUBLIC_URL").ok().map(|u| u.trim().trim_end_matches('/').trim_end_matches("/app").to_string()).filter(|u| !u.is_empty());
    let site = site.unwrap_or_else(|| "https://www.temisz.hu".into());
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
        "RedirectUrl": format!("{site}/app/"),
        "CallbackUrl": format!("{site}/api/barion/callback"),
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
    let Ok(v) = barion::call(&cfg, "/v2/Payment/Start", Some(&body)) else {
        failed_charge(conn, tenant_id, cemetery_id)?;
        return Ok(Charge::Failed);
    };
    let pid = v["PaymentId"].as_str().unwrap_or("").to_string();
    let status = v["Status"].as_str().unwrap_or("Unknown").to_string();
    if pid.is_empty() {
        failed_charge(conn, tenant_id, cemetery_id)?;
        return Ok(Charge::Failed);
    }
    conn.execute(
        "INSERT INTO platform.card_payments (payment_id, request_id, tenant_id, invoice_id, gross_huf, status, kind, cemetery_id, plan, period_start, period_end)
         VALUES (?1, ?2, ?3, '', ?4, ?5, 'renewal', ?6, ?7, CAST(?8 AS DATE), CAST(?9 AS DATE)) ON CONFLICT (payment_id) DO NOTHING",
        params![pid, request_id, tenant_id, total, status, cemetery_id, sub.plan, ymd(start), ymd(end)],
    )?;
    settle_renewal(store, &pid, Some(status))
}

fn failed_charge(conn: &Connection, tenant_id: &str, cemetery_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE platform.cemetery_subscriptions SET status = 'past_due', failed_charges = failed_charges + 1,
             past_due_since = coalesce(past_due_since, CAST(?3 AS DATE)), updated_at = now()
         WHERE tenant_id = ?1 AND cemetery_id = ?2 AND status IN ('trial', 'active', 'past_due')",
        params![tenant_id, cemetery_id, ymd(today())],
    )?;
    Ok(())
}

/// The outcome of a renewal charge: on success (exactly once) the final
/// invoice is issued and the period moves on. Called right after the charge
/// and from Barion's callback, with the state Barion reports.
pub fn settle_renewal(store: &Store, payment_id: &str, known: Option<String>) -> Result<Charge> {
    let conn = &store.conn;
    let row: Option<(String, String, i64, Option<String>, Option<String>, Option<String>, bool)> = conn
        .query_row(
            "SELECT tenant_id, coalesce(cemetery_id, ''), gross_huf, plan, to_char(period_start, 'YYYY-MM-DD'), to_char(period_end, 'YYYY-MM-DD'), settled_at IS NOT NULL
             FROM platform.card_payments WHERE payment_id = ?1 AND kind = 'renewal'",
            params![payment_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .optional()?;
    let Some((tenant, cemetery, total, plan, start, end, settled)) = row else { return Err(CoreError::not_found("payment", payment_id)) };
    if settled {
        return Ok(Charge::Paid);
    }
    let state = match known {
        Some(s) => s,
        None => {
            let cfg = barion::config().ok_or_else(|| CoreError::new("card_unavailable", "card payment is not set up"))?;
            let (state, paid, ok) = barion::payment_state(&cfg, payment_id)?;
            if state == "Succeeded" && !(ok && paid >= total) {
                "Failed".to_string()
            } else {
                state
            }
        }
    };
    conn.execute("UPDATE platform.card_payments SET status = ?2, updated_at = now() WHERE payment_id = ?1", params![payment_id, state])?;
    if state == "Succeeded" {
        let (Some(plan), Some(start), Some(end)) = (plan, start, end) else { return Err(CoreError::new("internal", "renewal without a period")) };
        let t = lunda_core::db::tx(conn)?;
        if t.execute("UPDATE platform.card_payments SET settled_at = now() WHERE payment_id = ?1 AND settled_at IS NULL", params![payment_id])? == 0 {
            return Ok(Charge::Paid);
        }
        let name: String = t
            .query_row("SELECT cemetery_name FROM platform.cemetery_subscriptions WHERE tenant_id = ?1 AND cemetery_id = ?2", params![tenant, cemetery], |r| {
                r.get(0)
            })
            .optional()?
            .unwrap_or_default();
        let inv = billing::issue(&t, &tenant, false, &format!("TEMISZ előfizetés – {name}"), 1, unit_net(&plan), (parse(&start)?, parse(&end)?), "card", None)?;
        t.execute("UPDATE platform.invoices SET cemetery_id = ?2 WHERE id = ?1", params![inv, cemetery])?;
        t.execute(
            "UPDATE platform.cemetery_subscriptions SET status = CASE WHEN status = 'canceled' THEN status ELSE 'active' END,
                 current_period_start = CAST(?3 AS DATE), current_period_end = CAST(?4 AS DATE), paid_at = coalesce(paid_at, now()),
                 failed_charges = 0, past_due_since = NULL, updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2",
            params![tenant, cemetery, start, end],
        )?;
        t.commit()?;
        return Ok(Charge::Paid);
    }
    if FAILED_STATES.contains(&state.as_str()) {
        failed_charge(conn, &tenant, &cemetery)?;
        return Ok(Charge::Failed);
    }
    Ok(Charge::Pending)
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
        store.conn.execute(
            "UPDATE platform.cemetery_subscriptions SET status = 'canceled', cancel_at_period_end = true, updated_at = now()
             WHERE tenant_id = ?1 AND cemetery_id = ?2 AND status IN ('trial', 'active', 'past_due')",
            params![tenant_id, cemetery_id],
        )?;
        // open proformas are not owed any more: the next period's, or an order never paid
        // (which ends at once; ordering again then works only after payment)
        store.conn.execute(
            "UPDATE platform.invoices SET status = 'void' WHERE tenant_id = ?1 AND cemetery_id = ?2 AND kind = 'proforma' AND status = 'open'",
            params![tenant_id, cemetery_id],
        )?;
    }
    let _ = lunda_core::audit::record_simple(&store.conn, s, "subscription", cemetery_id, if resume { "resume" } else { "cancel" }, None);
    Ok(())
}

/// A transfer proforma of a cemetery was paid: the period starts or moves on,
/// and from the first payment on the data can be exported.
pub fn proforma_paid(conn: &Connection, tenant_id: &str, cemetery_id: Option<&str>, start: &str, end: &str) -> Result<()> {
    let filter = if cemetery_id.is_some() { "AND cemetery_id = ?4" } else { "AND payment_method = 'transfer' AND ?4 = ''" };
    conn.execute(
        &format!(
            "UPDATE platform.cemetery_subscriptions SET status = CASE WHEN status = 'canceled' THEN status ELSE 'active' END,
                 current_period_start = CASE WHEN current_period_end IS NULL OR CAST(?2 AS DATE) >= current_period_end THEN CAST(?2 AS DATE) ELSE current_period_start END,
                 current_period_end = GREATEST(coalesce(current_period_end, CAST(?3 AS DATE)), CAST(?3 AS DATE)),
                 paid_at = coalesce(paid_at, now()), past_due_since = NULL, updated_at = now()
             WHERE tenant_id = ?1 {filter}"
        ),
        params![tenant_id, start, end, cemetery_id.unwrap_or("")],
    )?;
    Ok(())
}

/// For the Előfizetés page.
pub fn overview(store: &Store, tenant_id: &str) -> Result<Value> {
    let subs = list(&store.conn, tenant_id)?;
    let awaiting: Vec<String> = store
        .conn
        .prepare(
            "SELECT DISTINCT cemetery_id FROM platform.invoices WHERE tenant_id = ?1 AND kind = 'proforma' AND status = 'open' AND cemetery_id IS NOT NULL",
        )?
        .query_map(params![tenant_id], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let rows: Vec<Value> = cemeteries(store)?
        .iter()
        .map(|(id, name)| match subs.iter().find(|s| &s.cemetery_id == id) {
            Some(s) => json!({
                "cemeteryId": id, "name": name, "plan": s.plan, "method": s.method, "status": if usable(s) { s.status.clone() } else { "lapsed".into() },
                "trialEndsAt": s.trial_ends, "periodEnd": s.period_end, "cancelAtPeriodEnd": s.cancel_at_end, "paid": s.paid,
                "openProforma": awaiting.contains(id),
            }),
            None => json!({ "cemeteryId": id, "name": name, "status": "none", "paid": false }),
        })
        .collect();
    Ok(json!({
        "cemeteries": rows, "trialDays": TRIAL_DAYS, "demoDays": DEMO_DAYS, "verifyHuf": verify_amount(),
        "cardSubscriptions": card_enabled(), "trialUsed": unpaid_history(&store.conn, tenant_id)?,
    }))
}

// ---------------------------------------------------------------- daily job

/// Trial reminders, charges at trial and period end, retries, transfer
/// renewals, lapsing. Runs once a day. One customer's failure (the invoicing
/// service, Barion) does not stop the others.
pub fn daily(store: &Store, base_url: &str) -> Result<Value> {
    let conn = &store.conn;
    let rows: Vec<(String, String)> = conn
        .prepare(
            "SELECT c.tenant_id, c.cemetery_id FROM platform.cemetery_subscriptions c JOIN platform.tenants t ON t.id = c.tenant_id
             WHERE t.status <> 'closed' AND t.deleted_at IS NULL AND c.status IN ('trial', 'active', 'past_due', 'canceled', 'demo')
             ORDER BY c.tenant_id",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut n = Counts::default();
    for (tenant, cem) in rows {
        if let Err(e) = daily_one(store, base_url, &tenant, &cem, &mut n) {
            eprintln!("subscription daily job failed for {tenant}/{cem}: {e}");
            n.errors += 1;
        }
    }
    Ok(json!({
        "charged": n.charged, "chargeFailed": n.failed, "chargePending": n.pending, "lapsed": n.lapsed,
        "trialReminders": n.reminded, "renewalProformas": n.proformas, "errors": n.errors,
    }))
}

#[derive(Default)]
struct Counts {
    charged: u32,
    failed: u32,
    pending: u32,
    lapsed: u32,
    reminded: u32,
    proformas: u32,
    errors: u32,
}

fn daily_one(store: &Store, base_url: &str, tenant: &str, cem: &str, n: &mut Counts) -> Result<()> {
    let conn = &store.conn;
    let Some(sub) = get(conn, tenant, cem)? else { return Ok(()) };
    let today = today();
    let end_date = sub.period_end.as_deref().map(parse).transpose()?;
    let past_due_since: Option<String> = conn.query_row(
        "SELECT to_char(past_due_since, 'YYYY-MM-DD') FROM platform.cemetery_subscriptions WHERE tenant_id = ?1 AND cemetery_id = ?2",
        params![tenant, cem],
        |r| r.get(0),
    )?;
    let lapse = |why: &str| -> Result<()> {
        conn.execute(
            "UPDATE platform.cemetery_subscriptions SET status = 'lapsed', updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2",
            params![tenant, cem],
        )?;
        if billing::notify_once(conn, tenant, &format!("lapsed:{cem}:{}", ymd(today)))? {
            let (to, _) = billing::tenant_email(conn, tenant)?;
            mail::send(
                conn,
                &to,
                &format!("A(z) {} csak olvasható lett", sub.cemetery_name),
                &format!(
                    "Jó napot kívánunk!\n\n{why} A(z) {} adatai megmaradtak és olvashatók; a további munkához fizessen elő a Beállítások › Előfizetés oldalon:\n{base_url}/{}",
                    sub.cemetery_name,
                    mail::FOOTER
                ),
                "lapsed",
            )?;
        }
        Ok(())
    };
    let run_charge = |n: &mut Counts| -> Result<()> {
        match charge(store, tenant, cem)? {
            Charge::Paid => n.charged += 1,
            Charge::Failed => {
                n.failed += 1;
                if billing::notify_once(conn, tenant, &format!("chargefail:{cem}:{}", ymd(today)))? {
                    let (to, _) = billing::tenant_email(conn, tenant)?;
                    mail::send(
                        conn,
                        &to,
                        &format!("Sikertelen kártyás terhelés – {}", sub.cemetery_name),
                        &format!(
                            "Jó napot kívánunk!\n\nA(z) {} előfizetésének díját ({} Ft) nem sikerült levonni a mentett bankkártyáról. {} napig naponta újrapróbáljuk; ha addig sem sikerül, a temető csak olvasható lesz. Ha a kártya lejárt vagy lecserélte, fizessen elő újra a Beállítások › Előfizetés oldalon:\n{base_url}/{}",
                            sub.cemetery_name,
                            billing::fmt_huf(gross(unit_net(&sub.plan))),
                            billing::OVERDUE_GRACE_DAYS,
                            mail::FOOTER
                        ),
                        "charge_failed",
                    )?;
                }
            }
            Charge::Pending => n.pending += 1,
        }
        Ok(())
    };
    match (sub.status.as_str(), sub.method.as_str()) {
        // the demonstration is over: read only (no e-mail, nothing was ordered)
        ("demo", _) => {
            if sub.trial_over {
                conn.execute(
                    "UPDATE platform.cemetery_subscriptions SET status = 'lapsed', updated_at = now() WHERE tenant_id = ?1 AND cemetery_id = ?2",
                    params![tenant, cem],
                )?;
                n.lapsed += 1;
            }
        }
        // card trial: a reminder 3 days before, then the first charge
        ("trial", "card") => {
            if sub.trial_over {
                run_charge(n)?;
            } else if let Some(te) = &sub.trial_ends {
                let left = (parse(te)? - today).num_days();
                if left <= 3 && billing::notify_once(conn, tenant, &format!("trial3:{cem}"))? {
                    let (to, _) = billing::tenant_email(conn, tenant)?;
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
                    n.reminded += 1;
                }
            }
        }
        // an older trial without a card: locked when it is over
        ("trial", _) => {
            if sub.trial_over {
                lapse("A próbaidő lejárt.")?;
                n.lapsed += 1;
            }
        }
        ("active", "card") if !sub.cancel_at_end => {
            if end_date.is_none_or(|e| e <= today) {
                run_charge(n)?;
            }
        }
        ("past_due", "card") => {
            let since = past_due_since.as_deref().map(parse).transpose()?.unwrap_or(today);
            if (today - since).num_days() > billing::OVERDUE_GRACE_DAYS {
                lapse("A kártyás terhelés többszöri próbálkozásra sem sikerült.")?;
                n.lapsed += 1;
            } else {
                run_charge(n)?;
            }
        }
        ("active", "transfer") if !sub.cancel_at_end => {
            // an unpaid proforma locks the cemetery after the grace period
            let overdue: bool = conn.query_row(
                "SELECT EXISTS (SELECT 1 FROM platform.invoices WHERE tenant_id = ?1 AND (cemetery_id = ?2 OR cemetery_id IS NULL) AND kind = 'proforma'
                                AND status = 'open' AND due_date + CAST(?3 AS INTEGER) < CAST(?4 AS DATE))",
                params![tenant, cem, billing::OVERDUE_GRACE_DAYS as i32, ymd(today)],
                |r| r.get(0),
            )?;
            if overdue {
                lapse("A díjbekérő fizetési határideje lejárt.")?;
                n.lapsed += 1;
                return Ok(());
            }
            // the next period's proforma ahead of time (once the current one is paid)
            if let Some(e) = end_date.filter(|e| sub.paid && *e <= today + Duration::days(billing::RENEWAL_NOTICE_DAYS)) {
                let exists: bool = conn.query_row(
                    "SELECT EXISTS (SELECT 1 FROM platform.invoices WHERE tenant_id = ?1 AND cemetery_id = ?2 AND kind = 'proforma'
                                    AND period_start = CAST(?3 AS DATE) AND status <> 'void')",
                    params![tenant, cem, ymd(e)],
                    |r| r.get(0),
                )?;
                if !exists {
                    let name = format!("TEMISZ előfizetés – {} – következő időszak", sub.cemetery_name);
                    let inv = billing::issue(conn, tenant, true, &name, 1, unit_net(&sub.plan), (e, period_end(e, &sub.plan)), "transfer", None)?;
                    conn.execute("UPDATE platform.invoices SET cemetery_id = ?2 WHERE id = ?1", params![inv, cem])?;
                    n.proformas += 1;
                }
            }
        }
        // cancelled: over when the paid period or the trial ends
        _ => {
            if !usable(&sub) {
                lapse("Az előfizetés lemondás miatt véget ért.")?;
                n.lapsed += 1;
            }
        }
    }
    Ok(())
}
