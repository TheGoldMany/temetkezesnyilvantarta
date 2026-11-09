//! Card payment of TEMISZ proformas through Barion (Smart Gateway).
//!
//! * `start`: the customer's admin pays an open proforma; Barion returns the
//!   gateway address the browser is sent to.
//! * `refresh`: Barion's callback (and the return to the app) only names the
//!   payment; its state is always asked from Barion itself, so a forged
//!   callback cannot settle anything. A succeeded payment of the full amount
//!   settles the proforma (final invoice, period started) exactly once.
//!
//! Settings: BARION_POS_KEY (secret), BARION_PAYEE (the Barion wallet e-mail
//! that receives the money), BARION_ENV=prod for live payments (anything else
//! is the test system). BARION_API_URL overrides the address (tests).

use lunda_core::auth::{Permission, Session};
use lunda_core::sql::{params, Connection, OptionalExtension};
use lunda_core::util::new_id;
use lunda_core::{CoreError, Result};
use serde_json::{json, Value};

use crate::billing;

pub struct Config {
    pub pos_key: String,
    pub payee: String,
    pub api: String,
}

/// Barion is configured: card payment is offered.
pub fn config() -> Option<Config> {
    let pos_key = std::env::var("BARION_POS_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())?;
    let payee = std::env::var("BARION_PAYEE").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())?;
    let api = std::env::var("BARION_API_URL").ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| {
        if std::env::var("BARION_ENV").map(|e| e.trim().eq_ignore_ascii_case("prod")).unwrap_or(false) {
            "https://api.barion.com".into()
        } else {
            "https://api.test.barion.com".into()
        }
    });
    Some(Config { pos_key, payee, api: api.trim_end_matches('/').to_string() })
}

fn unavailable() -> CoreError {
    CoreError::new("card_unavailable", "card payment is not set up")
}

fn call(cfg: &Config, path: &str, body: Option<&Value>) -> Result<Value> {
    let url = format!("{}{path}", cfg.api);
    let failed = |e: String| CoreError::new("card_failed", e);
    let res = match body {
        Some(b) => ureq::post(&url)
            .config()
            .http_status_as_error(false)
            .build()
            .header("Content-Type", "application/json")
            .header("x-pos-key", &cfg.pos_key)
            .send(b.to_string().as_bytes()),
        None => ureq::get(&url).config().http_status_as_error(false).build().header("x-pos-key", &cfg.pos_key).call(),
    };
    let resp = res.map_err(|e| failed(e.to_string()))?;
    let status = resp.status().as_u16();
    let text = resp.into_body().read_to_string().map_err(|e| failed(e.to_string()))?;
    let v: Value = serde_json::from_str(&text).unwrap_or(json!({ "raw": text }));
    let errors = v.get("Errors").and_then(|e| e.as_array()).filter(|e| !e.is_empty());
    if status >= 400 || errors.is_some() {
        let msg = errors
            .and_then(|e| e.first())
            .map(|e| format!("{}: {}", e["ErrorCode"].as_str().unwrap_or(""), e["Description"].as_str().unwrap_or("")))
            .unwrap_or_else(|| format!("HTTP {status}"));
        return Err(failed(msg));
    }
    Ok(v)
}

/// Starts the card payment of an open proforma of the customer. Returns the
/// gateway address the browser goes to.
pub fn start(conn: &Connection, s: &Session, tenant_id: &str, invoice_id: &str, app_url: &str, site_url: &str) -> Result<Value> {
    s.require(Permission::ManageSettings)?;
    let cfg = config().ok_or_else(unavailable)?;
    let row: Option<(String, String, i64, String)> = conn
        .query_row(
            "SELECT number, description, gross_huf, status FROM platform.invoices WHERE id = ?1 AND tenant_id = ?2 AND kind = 'proforma'",
            params![invoice_id, tenant_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((number, description, gross, status)) = row else { return Err(CoreError::not_found("invoice", invoice_id)) };
    if status != "open" {
        return Err(CoreError::new("invoice_not_open", "this proforma is not open"));
    }
    let email: String = conn
        .query_row(
            "SELECT coalesce((SELECT email FROM platform.billing_details WHERE tenant_id = ?1), (SELECT billing_email FROM platform.tenants WHERE id = ?1))",
            params![tenant_id],
            |r| r.get(0),
        )
        .unwrap_or_default();
    let request_id = new_id();
    let body = json!({
        "POSKey": cfg.pos_key,
        "PaymentType": "Immediate",
        "GuestCheckOut": true,
        "FundingSources": ["All"],
        "PaymentRequestId": request_id,
        "PayerHint": email,
        "RedirectUrl": format!("{app_url}/#fizetes={request_id}"),
        "CallbackUrl": format!("{site_url}/api/barion/callback"),
        "Locale": "hu-HU",
        "Currency": "HUF",
        "OrderNumber": number,
        "Transactions": [{
            "POSTransactionId": invoice_id,
            "Payee": cfg.payee,
            "Total": gross,
            "Comment": format!("TEMISZ díjbekérő {number}"),
            "Items": [{
                "Name": "TEMISZ előfizetés",
                "Description": format!("{description} ({number})"),
                "Quantity": 1,
                "Unit": "db",
                "UnitPrice": gross,
                "ItemTotal": gross,
                "SKU": number,
            }],
        }],
    });
    let v = call(&cfg, "/v2/Payment/Start", Some(&body))?;
    let payment_id = v["PaymentId"].as_str().filter(|p| !p.is_empty()).ok_or_else(|| CoreError::new("card_failed", "no payment id in the answer"))?;
    let gateway =
        v["GatewayUrl"].as_str().filter(|g| g.starts_with("https://")).ok_or_else(|| CoreError::new("card_failed", "no gateway address in the answer"))?;
    conn.execute(
        "INSERT INTO platform.card_payments (payment_id, request_id, tenant_id, invoice_id, gross_huf, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![payment_id, request_id, tenant_id, invoice_id, gross, v["Status"].as_str().unwrap_or("Prepared")],
    )?;
    let _ = lunda_core::audit::record_simple(conn, s, "invoice", invoice_id, "card_payment_started", Some(payment_id));
    Ok(json!({ "gatewayUrl": gateway, "paymentId": payment_id }))
}

/// Asks Barion for the payment's state and settles the proforma when the
/// full amount succeeded. Unknown payment ids are ignored. Returns the state.
pub fn refresh(conn: &Connection, payment_id: &str) -> Result<String> {
    let cfg = config().ok_or_else(unavailable)?;
    let known: Option<(String, i64, String)> = conn
        .query_row("SELECT invoice_id, gross_huf, status FROM platform.card_payments WHERE payment_id = ?1", params![payment_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .optional()?;
    let Some((invoice_id, gross, old)) = known else { return Err(CoreError::not_found("payment", payment_id)) };
    if old == "Succeeded" {
        return Ok(old);
    }
    // the payment id goes into the path: only hex digits and dashes
    if !payment_id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
        return Err(CoreError::not_found("payment", payment_id));
    }
    let v = call(&cfg, &format!("/v4/payment/{payment_id}/paymentstate"), None)?;
    let state = v["Status"].as_str().unwrap_or("Unknown").to_string();
    let total = v["Total"].as_f64().unwrap_or(0.0).round() as i64;
    let currency = v["Currency"].as_str().unwrap_or("HUF");
    let ours = v["PaymentId"].as_str().map(|p| p.eq_ignore_ascii_case(payment_id)).unwrap_or(true);
    let t = lunda_core::db::tx(conn)?;
    t.execute("UPDATE platform.card_payments SET status = ?2, updated_at = now() WHERE payment_id = ?1", params![payment_id, state])?;
    if state == "Succeeded" && ours && currency == "HUF" && total >= gross {
        let open: bool = t.query_row("SELECT status = 'open' FROM platform.invoices WHERE id = ?1", params![invoice_id], |r| r.get(0))?;
        if open {
            billing::mark_paid_by(&t, &invoice_id, "card")?;
        }
    }
    t.commit()?;
    Ok(state)
}

/// After the return from the gateway: the state of the customer's payment.
pub fn status_for(conn: &Connection, tenant_id: &str, request_id: &str) -> Result<Value> {
    let row: Option<(String, String)> = conn
        .query_row("SELECT payment_id, status FROM platform.card_payments WHERE request_id = ?1 AND tenant_id = ?2", params![request_id, tenant_id], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .optional()?;
    let Some((payment_id, old)) = row else { return Err(CoreError::not_found("payment", request_id)) };
    let state = if old == "Succeeded" { old } else { refresh(conn, &payment_id).unwrap_or(old) };
    Ok(json!({ "status": state }))
}

/// Barion's callback: `paymentId` in the query string or the form body.
pub fn payment_id_from(query: &str, body: &[u8]) -> Option<String> {
    let find = |s: &str| s.split('&').find_map(|kv| kv.split_once('=').filter(|(k, _)| k.eq_ignore_ascii_case("paymentId")).map(|(_, v)| v.trim().to_string()));
    find(query)
        .or_else(|| find(&String::from_utf8_lossy(body)))
        .or_else(|| serde_json::from_slice::<Value>(body).ok().and_then(|v| v["PaymentId"].as_str().or(v["paymentId"].as_str()).map(String::from)))
}
