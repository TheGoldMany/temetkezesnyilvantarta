//! TEMISZ's own subscription invoices in Billingo (API v3). Billingo reports
//! the invoices to NAV Online Számla and e-mails them to the customer.
//!
//! Set BILLINGO_API_KEY (Billingo › Beállítások › API). The invoices go to
//! the first invoice block, or to BILLINGO_BLOCK_ID. Without a key the
//! Számla Agent (SZAMLAZZ_AGENT_KEY) or the test mode issues them.

use lunda_core::sql::{params, Connection, OptionalExtension};
use lunda_core::szamlazz::{Document, Issued, Party};
use lunda_core::{CoreError, Result};
use serde_json::{json, Value};

pub struct Config {
    pub key: String,
    pub api: String,
    pub block_id: Option<i64>,
}

pub fn config() -> Option<Config> {
    let key = std::env::var("BILLINGO_API_KEY").ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())?;
    Some(Config {
        key,
        api: std::env::var("BILLINGO_API_URL").ok().filter(|u| !u.trim().is_empty()).unwrap_or_else(|| "https://api.billingo.hu/v3".into()),
        block_id: std::env::var("BILLINGO_BLOCK_ID").ok().and_then(|b| b.trim().parse().ok()),
    })
}

fn failed(msg: String) -> CoreError {
    CoreError::new("invoicing_failed", msg)
}

/// (status, body)
fn call(cfg: &Config, method: &str, path: &str, body: Option<&Value>) -> Result<(u16, Vec<u8>)> {
    let url = format!("{}{path}", cfg.api);
    let res = match (method, body) {
        ("POST", b) => ureq::post(&url)
            .config()
            .http_status_as_error(false)
            .build()
            .header("X-API-KEY", &cfg.key)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send(b.map(|v| v.to_string()).unwrap_or_else(|| "{}".into()).as_bytes()),
        ("PUT", b) => ureq::put(&url)
            .config()
            .http_status_as_error(false)
            .build()
            .header("X-API-KEY", &cfg.key)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .send(b.map(|v| v.to_string()).unwrap_or_else(|| "{}".into()).as_bytes()),
        _ => ureq::get(&url).config().http_status_as_error(false).build().header("X-API-KEY", &cfg.key).call(),
    };
    let resp = res.map_err(|e| failed(format!("Billingo: {e}")))?;
    let status = resp.status().as_u16();
    let bytes = resp.into_body().with_config().limit(20 * 1024 * 1024).read_to_vec().map_err(|e| failed(format!("Billingo: {e}")))?;
    Ok((status, bytes))
}

fn json_call(cfg: &Config, method: &str, path: &str, body: Option<&Value>) -> Result<Value> {
    let (status, bytes) = call(cfg, method, path, body)?;
    let text = String::from_utf8_lossy(&bytes);
    if status >= 400 {
        let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        let msg = v["error"]["message"].as_str().or(v["message"].as_str()).map(str::to_string).unwrap_or_else(|| text.chars().take(300).collect());
        return Err(failed(format!("Billingo HTTP {status}: {msg}")));
    }
    serde_json::from_str(&text).map_err(|_| failed(format!("Billingo: unexpected answer to {path}")))
}

/// Billingo's VAT key: "27%" for a rate, the code itself for exemptions (AAM, TAM …).
pub fn vat_key(code: &str) -> String {
    let c = code.trim();
    if c.parse::<i64>().is_ok() {
        format!("{c}%")
    } else {
        c.to_string()
    }
}

fn partner_body(p: &Party) -> Value {
    let tax = p.tax_number.trim();
    json!({
        "name": p.name,
        "address": { "country_code": "HU", "post_code": p.postcode, "city": p.city, "address": p.address },
        "emails": [p.email],
        "taxcode": tax,
        "tax_type": if tax.is_empty() { "NO_TAX_NUMBER" } else { "HAS_TAX_NUMBER" },
    })
}

/// The customer as a Billingo partner: created once, then kept up to date.
fn partner(cfg: &Config, conn: &Connection, tenant_id: &str, p: &Party) -> Result<i64> {
    let known: Option<i64> =
        conn.query_row("SELECT billingo_partner_id FROM platform.billing_details WHERE tenant_id = ?1", params![tenant_id], |r| r.get(0)).optional()?.flatten();
    let body = partner_body(p);
    if let Some(id) = known {
        let (status, _) = call(cfg, "PUT", &format!("/partners/{id}"), Some(&body))?;
        if status < 400 {
            return Ok(id);
        }
        // deleted in Billingo meanwhile: a new one
    }
    let v = json_call(cfg, "POST", "/partners", Some(&body))?;
    let id = v["id"].as_i64().ok_or_else(|| failed("Billingo: no partner id".into()))?;
    conn.execute("UPDATE platform.billing_details SET billingo_partner_id = ?2 WHERE tenant_id = ?1", params![tenant_id, id])?;
    Ok(id)
}

fn block(cfg: &Config) -> Result<i64> {
    if let Some(b) = cfg.block_id {
        return Ok(b);
    }
    let v = json_call(cfg, "GET", "/document-blocks?type=invoice&per_page=25", None)?;
    v["data"].as_array().and_then(|d| d.first()).and_then(|b| b["id"].as_i64()).ok_or_else(|| failed("Billingo: no invoice block (számlatömb)".into()))
}

/// The PDF of a document; None while Billingo is still generating it.
pub fn pdf(cfg: &Config, document_id: i64) -> Result<Option<Vec<u8>>> {
    let (status, bytes) = call(cfg, "GET", &format!("/documents/{document_id}/download"), None)?;
    Ok((status == 200 && bytes.starts_with(b"%PDF")).then_some(bytes))
}

/// Issues the proforma or invoice, has Billingo e-mail it, and fetches the PDF.
pub fn issue(cfg: &Config, conn: &Connection, tenant_id: &str, d: &Document) -> Result<Issued> {
    let partner_id = partner(cfg, conn, tenant_id, &d.buyer)?;
    let block_id = block(cfg)?;
    let mut comment = d.note.clone();
    if let Some(p) = &d.proforma_number {
        comment += &format!(" Díjbekérő: {p}.");
    }
    let items: Vec<Value> = d
        .lines
        .iter()
        .map(|l| {
            json!({ "name": l.name, "unit_price": l.unit_net, "unit_price_type": "net", "quantity": l.quantity, "unit": l.unit,
                    "vat": vat_key(&l.vat_code), "comment": l.comment })
        })
        .collect();
    let body = json!({
        "partner_id": partner_id,
        "block_id": block_id,
        "type": if d.proforma { "proforma" } else { "invoice" },
        "fulfillment_date": d.fulfilment_date,
        "due_date": d.due_date,
        "payment_method": if d.payment_method == "Bankkártya" { "bankcard" } else { "wire_transfer" },
        "language": "hu",
        "currency": "HUF",
        "electronic": true,
        "paid": d.paid,
        "comment": comment,
        "items": items,
    });
    let doc = json_call(cfg, "POST", "/documents", Some(&body))?;
    let id = doc["id"].as_i64().ok_or_else(|| failed("Billingo: no document id".into()))?;
    let number = doc["invoice_number"].as_str().filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| format!("BILLINGO-{id}"));
    // Billingo e-mails the document; a failure here is not a failure of the invoice
    let mut log = format!("billingo:{id}");
    if let Err(e) = json_call(cfg, "POST", &format!("/documents/{id}/send"), Some(&json!({ "emails": [d.buyer.email] }))) {
        eprintln!("billingo send failed for {number}: {e}");
        log += " send-failed";
    }
    let pdf = pdf(cfg, id).unwrap_or(None);
    Ok(Issued { number, pdf, log })
}

/// The Billingo document behind an invoice row, from its provider log.
pub fn document_id(log: &str) -> Option<i64> {
    log.strip_prefix("billingo:")?.split_whitespace().next()?.parse().ok()
}

/// Whether Billingo itself e-mailed the document.
pub fn emailed(log: &str) -> bool {
    document_id(log).is_some() && !log.contains("send-failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vat_keys_and_logs() {
        assert_eq!(vat_key("27"), "27%");
        assert_eq!(vat_key("0"), "0%");
        assert_eq!(vat_key("AAM"), "AAM");
        assert_eq!(vat_key(" TAM "), "TAM");
        assert_eq!(document_id("billingo:123"), Some(123));
        assert_eq!(document_id("billingo:123 send-failed"), Some(123));
        assert!(emailed("billingo:5") && !emailed("billingo:5 send-failed") && !emailed("test mode"));
        let p = Party {
            name: "Kisrét".into(),
            tax_number: "".into(),
            postcode: "8999".into(),
            city: "Kisrét".into(),
            address: "Fő u. 1.".into(),
            email: "a@b.hu".into(),
        };
        assert_eq!(partner_body(&p)["tax_type"], "NO_TAX_NUMBER");
        assert_eq!(partner_body(&p)["address"]["country_code"], "HU");
    }
}
