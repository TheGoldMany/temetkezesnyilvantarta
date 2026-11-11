//! Online card payment of an operator's invoices by the payer (FR-PEN online):
//! the operator connects its own Barion shop, the program makes a payment
//! link for an invoice, the payer opens it without signing in and pays by
//! card; the money goes to the operator's Barion wallet and the payment is
//! recorded on the invoice.
//!
//! * The operator's Barion key is stored encrypted (SECRETS_KEY or CRON_SECRET)
//!   and never shown again.
//! * A link is an unguessable token for one invoice; the page shows only what
//!   the payer needs (operator, invoice number, amount, due date).
//! * The state is always asked from Barion with the operator's key; a payment
//!   of the full amount is recorded once (reference "Barion <payment id>").

use lunda_core::auth::{Permission, Session};
use lunda_core::sql::{params, Connection, OptionalExtension};
use lunda_core::store::finance::PaymentInput;
use lunda_core::util::new_id;
use lunda_core::{settings, CoreError, Result, Store};
use serde_json::{json, Value};

use crate::barion::{self, Config};
use crate::lifecycle;

fn schema_of(conn: &Connection, tenant_id: &str) -> Result<String> {
    Ok(conn.query_row("SELECT schema_name FROM platform.tenants WHERE id = ?1", params![tenant_id], |r| r.get(0))?)
}

/// The operator's Barion settings for the settings page (the key is never returned).
pub fn settings_get(conn: &Connection, s: &Session, tenant_id: &str) -> Result<Value> {
    s.require(Permission::ManageSettings)?;
    let row: Option<(String, bool, String, String)> = conn
        .query_row(
            "SELECT payee, live, terms_url, to_char(updated_at, 'YYYY-MM-DD') FROM platform.tenant_barion WHERE tenant_id = ?1",
            params![tenant_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    Ok(match row {
        Some((payee, live, terms_url, updated)) => json!({ "configured": true, "payee": payee, "live": live, "termsUrl": terms_url, "updatedAt": updated }),
        None => json!({ "configured": false, "payee": "", "live": false, "termsUrl": "", "updatedAt": null }),
    })
}

/// Saves the operator's Barion shop. An empty key keeps the stored one; `remove` switches online payment off.
pub fn settings_save(conn: &Connection, s: &Session, tenant_id: &str, a: &Value) -> Result<Value> {
    s.require(Permission::ManageSettings)?;
    if a["remove"].as_bool() == Some(true) {
        conn.execute("DELETE FROM platform.tenant_barion WHERE tenant_id = ?1", params![tenant_id])?;
        let _ = lunda_core::audit::record_simple(conn, s, "settings", tenant_id, "online_payment_off", None);
        return settings_get(conn, s, tenant_id);
    }
    let payee = a["payee"].as_str().unwrap_or("").trim().to_lowercase();
    if !crate::valid_email(&payee) {
        return Err(CoreError::validation("payee", "the Barion wallet e-mail address is invalid").with("code", "email_invalid"));
    }
    let terms_url = a["termsUrl"].as_str().unwrap_or("").trim().to_string();
    if !terms_url.starts_with("https://") || terms_url.len() > 500 || terms_url.contains(char::is_whitespace) {
        return Err(CoreError::validation("termsUrl", "an https:// address of the payment terms is required").with("code", "url_invalid"));
    }
    let live = a["live"].as_bool().unwrap_or(false);
    let key = a["posKey"].as_str().unwrap_or("").trim().to_string();
    let secret = lifecycle::secrets_key().ok_or_else(|| CoreError::new("secrets_key_missing", "the server has no key for stored secrets"))?;
    if key.is_empty() {
        let n = conn.execute(
            "UPDATE platform.tenant_barion SET payee = ?2, live = ?3, terms_url = ?4, updated_at = now(), updated_by = ?5 WHERE tenant_id = ?1",
            params![tenant_id, payee, live, terms_url, s.user_name],
        )?;
        if n == 0 {
            return Err(CoreError::validation("posKey", "the Barion secret key is required").with("code", "required"));
        }
    } else {
        // a Barion POSKey is a GUID
        let guid = key.len() == 36 && key.chars().enumerate().all(|(i, c)| if [8, 13, 18, 23].contains(&i) { c == '-' } else { c.is_ascii_hexdigit() });
        if !guid {
            return Err(CoreError::validation("posKey", "this is not a Barion secret key").with("code", "pos_key_invalid"));
        }
        let enc = lifecycle::seal(&secret, key.as_bytes());
        conn.execute(
            "INSERT INTO platform.tenant_barion (tenant_id, pos_key_enc, payee, live, terms_url, updated_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (tenant_id) DO UPDATE SET pos_key_enc = excluded.pos_key_enc, payee = excluded.payee, live = excluded.live,
                 terms_url = excluded.terms_url, updated_at = now(), updated_by = excluded.updated_by",
            params![tenant_id, enc, payee, live, terms_url, s.user_name],
        )?;
    }
    let _ = lunda_core::audit::record_simple(
        conn,
        s,
        "settings",
        tenant_id,
        "online_payment_saved",
        Some(&format!("{payee}, {}", if live { "éles" } else { "teszt" })),
    );
    settings_get(conn, s, tenant_id)
}

/// The operator's Barion connection, when online payment is on: (config, terms address).
pub fn tenant_config(conn: &Connection, tenant_id: &str) -> Result<Option<(Config, String)>> {
    let row: Option<(Vec<u8>, String, bool, String)> = conn
        .query_row("SELECT pos_key_enc, payee, live, terms_url FROM platform.tenant_barion WHERE tenant_id = ?1", params![tenant_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })
        .optional()?;
    let Some((enc, payee, live, terms)) = row else { return Ok(None) };
    let secret = lifecycle::secrets_key().ok_or_else(|| CoreError::new("secrets_key_missing", "the server has no key for stored secrets"))?;
    let key = String::from_utf8(lifecycle::open_sealed(&secret, &enc)?).map_err(|_| CoreError::new("card_unavailable", "stored key unreadable"))?;
    Ok(Some((Config { pos_key: key, payee, api: barion::api_url(live) }, terms)))
}

fn payable(store: &Store, invoice_id: &str) -> Result<lunda_core::store::finance::Invoice> {
    let inv = store.get_invoice(invoice_id)?;
    if inv.kind != "invoice" || !matches!(inv.status.as_str(), "issued" | "partially_paid") || inv.gross_huf - inv.paid_huf <= 0 {
        return Err(CoreError::new("invoice_not_open", "this invoice cannot be paid online"));
    }
    Ok(inv)
}

/// The payment link of an invoice (the same link every time). The connection
/// must be in the tenant's schema.
pub fn pay_link(store: &Store, s: &Session, tenant_id: &str, invoice_id: &str, site_url: &str) -> Result<Value> {
    s.require(Permission::RecordPayments)?;
    if tenant_config(&store.conn, tenant_id)?.is_none() {
        return Err(CoreError::new("online_payment_off", "online payment is not set up"));
    }
    payable(store, invoice_id)?;
    let token: String = store.conn.query_row(
        "INSERT INTO platform.pay_links (token, tenant_id, invoice_id) VALUES (?1, ?2, ?3)
         ON CONFLICT (tenant_id, invoice_id) DO UPDATE SET tenant_id = excluded.tenant_id RETURNING token",
        params![new_id().replace('-', ""), tenant_id, invoice_id],
        |r| r.get(0),
    )?;
    Ok(json!({ "url": format!("{site_url}/fizetes?t={token}"), "token": token }))
}

/// For the payment reminder e-mail: the link, when online payment is on and the invoice is payable.
pub fn link_for_mail(store: &Store, tenant_id: &str, invoice_id: &str, site_url: &str) -> Option<String> {
    pay_link(store, &Session::system(), tenant_id, invoice_id, site_url).ok().and_then(|v| v["url"].as_str().map(String::from))
}

/// Resolves a link: switches to the tenant's schema; (tenant, invoice id).
fn open_link(store: &Store, token: &str) -> Result<(String, String)> {
    let row: Option<(String, String)> = store
        .conn
        .query_row(
            "SELECT l.tenant_id, l.invoice_id FROM platform.pay_links l JOIN platform.tenants t ON t.id = l.tenant_id
             WHERE l.token = ?1 AND t.status <> 'closed' AND t.deleted_at IS NULL",
            params![token],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((tenant, invoice)) = row else { return Err(CoreError::not_found("link", token)) };
    store.conn.use_schema(&schema_of(&store.conn, &tenant)?)?;
    Ok((tenant, invoice))
}

/// "Kovács János" → "Kovács J." (the link may be forwarded).
fn short_name(name: &str) -> String {
    let mut parts = name.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some(a), Some(b)) => format!("{a} {}.", b.chars().next().unwrap_or(' ')),
        (Some(a), None) => a.to_string(),
        _ => String::new(),
    }
}

/// The payment page: what is paid to whom.
pub fn public_info(store: &Store, token: &str) -> Result<Value> {
    let (tenant, invoice_id) = open_link(store, token)?;
    let cfg = tenant_config(&store.conn, &tenant)?;
    let inv = store.get_invoice(&invoice_id)?;
    let cemetery: String = store.conn.query_row("SELECT name FROM cemeteries WHERE id = ?1", params![inv.cemetery_id], |r| r.get(0)).unwrap_or_default();
    let operator = settings::get(&store.conn, "operator.name", None)?.unwrap_or_default();
    let due = inv.gross_huf - inv.paid_huf;
    let state = if inv.status == "paid" || due <= 0 {
        "paid"
    } else if matches!(inv.status.as_str(), "issued" | "partially_paid") {
        "open"
    } else {
        "closed"
    };
    Ok(json!({
        "operator": operator, "cemetery": cemetery, "number": inv.number, "payer": short_name(&inv.buyer_name),
        "dueHuf": due.max(0), "grossHuf": inv.gross_huf, "dueDate": inv.due_date, "state": state,
        "enabled": cfg.is_some(), "termsUrl": cfg.map(|(_, t)| t).unwrap_or_default(),
    }))
}

/// Starts the card payment from the payment page; returns the Barion gateway address.
pub fn public_start(store: &Store, token: &str, site_url: &str, accepted: bool) -> Result<Value> {
    if !accepted {
        return Err(CoreError::new("terms_required", "the payment terms must be accepted"));
    }
    let (tenant, invoice_id) = open_link(store, token)?;
    let (cfg, _) = tenant_config(&store.conn, &tenant)?.ok_or_else(|| CoreError::new("online_payment_off", "online payment is not set up"))?;
    let inv = payable(store, &invoice_id)?;
    let due = inv.gross_huf - inv.paid_huf;
    let operator = settings::get(&store.conn, "operator.name", None)?.unwrap_or_default();
    let request_id = new_id();
    let hint = Some(inv.buyer_email.trim()).filter(|e| crate::valid_email(e)).unwrap_or("");
    let mut body = json!({
        "POSKey": cfg.pos_key,
        "PaymentType": "Immediate",
        "GuestCheckOut": true,
        "FundingSources": ["All"],
        "PaymentRequestId": request_id,
        "RedirectUrl": format!("{site_url}/fizetes?t={token}&r={request_id}"),
        "CallbackUrl": format!("{site_url}/api/barion/callback"),
        "Locale": "hu-HU",
        "Currency": "HUF",
        "OrderNumber": inv.number,
        "Transactions": [{
            "POSTransactionId": invoice_id,
            "Payee": cfg.payee,
            "Total": due,
            "Comment": format!("{operator}: {} számla", inv.number),
            "Items": [{
                "Name": "Temetői díj",
                "Description": format!("{} számla kiegyenlítése", inv.number),
                "Quantity": 1,
                "Unit": "db",
                "UnitPrice": due,
                "ItemTotal": due,
                "SKU": inv.number,
            }],
        }],
    });
    if !hint.is_empty() {
        body["PayerHint"] = json!(hint);
    }
    let (payment_id, gateway, state) = barion::start_payment(&cfg, &body)?;
    store.conn.execute(
        "INSERT INTO platform.card_payments (payment_id, request_id, tenant_id, invoice_id, gross_huf, status, kind, pay_token) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'resident', ?7)",
        params![payment_id, request_id, tenant, invoice_id, due, state, token],
    )?;
    Ok(json!({ "gatewayUrl": gateway }))
}

/// Back on the payment page: the state of the payment started there.
pub fn public_status(store: &Store, token: &str, request_id: &str) -> Result<Value> {
    let row: Option<(String, String)> = store
        .conn
        .query_row(
            "SELECT payment_id, status FROM platform.card_payments WHERE request_id = ?1 AND pay_token = ?2 AND kind = 'resident'",
            params![request_id, token],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((payment_id, old)) = row else { return Err(CoreError::not_found("payment", request_id)) };
    let state = if old == "Succeeded" { old } else { barion::refresh(store, &payment_id).unwrap_or(old) };
    Ok(json!({ "status": state }))
}

/// Barion's state with the operator's key; a full payment is recorded on the invoice once.
pub fn settle(store: &Store, tenant_id: &str, payment_id: &str, invoice_id: &str, due: i64) -> Result<String> {
    let conn = &store.conn;
    let (cfg, _) = tenant_config(conn, tenant_id)?.ok_or_else(|| CoreError::new("online_payment_off", "online payment is not set up"))?;
    let (state, total, ok) = barion::payment_state(&cfg, payment_id)?;
    let t = lunda_core::db::tx(conn)?;
    t.execute("UPDATE platform.card_payments SET status = ?2, updated_at = now() WHERE payment_id = ?1", params![payment_id, state])?;
    if state == "Succeeded" && ok && total >= due {
        conn.use_schema(&schema_of(conn, tenant_id)?)?;
        let reference = format!("Barion {payment_id}");
        let done: Option<i64> =
            conn.query_row("SELECT 1 FROM payments WHERE reference = ?1 AND voided_at IS NULL", params![reference], |r| r.get(0)).optional()?;
        if done.is_none() {
            let inv = store.get_invoice(invoice_id)?;
            store.record_payment(
                &Session::system(),
                &PaymentInput {
                    cemetery_id: inv.cemetery_id,
                    invoice_id: Some(invoice_id.to_string()),
                    person_id: None,
                    payer_name: None,
                    amount_huf: total,
                    method: "card".into(),
                    paid_on: None,
                    reference,
                    notes: "Online, bankkártyával (Barion)".into(),
                },
            )?;
        }
    }
    t.commit()?;
    Ok(state)
}
