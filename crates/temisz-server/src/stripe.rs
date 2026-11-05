//! Card payments with Stripe: Checkout for the first payment, recurring
//! billing by Stripe, the customer portal for card changes, webhooks.
//! Needs STRIPE_SECRET_KEY, STRIPE_WEBHOOK_SECRET and STRIPE_TAX_RATE_ID
//! (a 27% exclusive VAT tax rate created in the Stripe dashboard).

use hmac::{Hmac, Mac};
use lunda_core::{CoreError, Result};
use serde_json::Value;
use sha2::Sha256;

pub fn available() -> bool {
    std::env::var("STRIPE_SECRET_KEY").is_ok()
}

fn key() -> Result<String> {
    std::env::var("STRIPE_SECRET_KEY").map_err(|_| CoreError::new("card_unavailable", "card payment is not configured"))
}

fn form(pairs: &[(&str, String)]) -> String {
    let enc = |s: &str| {
        s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect::<String>()
    };
    pairs.iter().map(|(k, v)| format!("{}={}", enc(k), enc(v))).collect::<Vec<_>>().join("&")
}

fn call(method: &str, path: &str, pairs: &[(&str, String)]) -> Result<Value> {
    let auth = format!("Bearer {}", key()?);
    let url = format!("https://api.stripe.com/v1/{path}");
    let res = if method == "GET" {
        ureq::get(&url).header("Authorization", &auth).call()
    } else {
        ureq::post(&url).header("Authorization", &auth).header("Content-Type", "application/x-www-form-urlencoded").send(form(pairs))
    };
    let mut res = res.map_err(|e| CoreError::new("payment_failed", e.to_string()))?;
    let text = res.body_mut().read_to_string().map_err(|e| CoreError::new("payment_failed", e.to_string()))?;
    Ok(serde_json::from_str(&text)?)
}

/// Stripe amounts for HUF are in fillér (two decimals) even though only whole forints are charged.
pub fn huf_minor(huf: i64) -> i64 {
    huf * 100
}

/// Starts Checkout for a subscription; returns the URL to send the browser to.
pub fn checkout(tenant_id: &str, email: &str, plan: &str, quantity: i64, unit_net_huf: i64, base_url: &str) -> Result<String> {
    let interval = if plan == "yearly" { "year" } else { "month" };
    let mut pairs = vec![
        ("mode", "subscription".to_string()),
        ("client_reference_id", tenant_id.to_string()),
        ("customer_email", email.to_string()),
        ("success_url", format!("{base_url}/#elofizetes=sikeres")),
        ("cancel_url", format!("{base_url}/#elofizetes=megszakitva")),
        ("locale", "hu".to_string()),
        ("line_items[0][quantity]", quantity.to_string()),
        ("line_items[0][price_data][currency]", "huf".to_string()),
        ("line_items[0][price_data][unit_amount]", huf_minor(unit_net_huf).to_string()),
        ("line_items[0][price_data][tax_behavior]", "exclusive".to_string()),
        ("line_items[0][price_data][recurring][interval]", interval.to_string()),
        ("line_items[0][price_data][product_data][name]", "TEMISZ előfizetés (temetőnként)".to_string()),
        ("subscription_data[metadata][tenant_id]", tenant_id.to_string()),
    ];
    if let Ok(rate) = std::env::var("STRIPE_TAX_RATE_ID") {
        pairs.push(("line_items[0][tax_rates][0]", rate));
    }
    let v = call("POST", "checkout/sessions", &pairs)?;
    v["url"].as_str().map(String::from).ok_or_else(|| CoreError::new("payment_failed", v["error"]["message"].as_str().unwrap_or("no checkout url").to_string()))
}

/// Customer portal (change card, cancel).
pub fn portal(customer_id: &str, base_url: &str) -> Result<String> {
    let v = call("POST", "billing_portal/sessions", &[("customer", customer_id.to_string()), ("return_url", format!("{base_url}/"))])?;
    v["url"].as_str().map(String::from).ok_or_else(|| CoreError::new("payment_failed", "no portal url"))
}

/// Changes the billed number of cemeteries; Stripe charges the difference now.
pub fn set_quantity(subscription_id: &str, quantity: i64) -> Result<()> {
    let sub = call("GET", &format!("subscriptions/{subscription_id}"), &[])?;
    let item = sub["items"]["data"][0]["id"].as_str().ok_or_else(|| CoreError::new("payment_failed", "subscription item not found"))?.to_string();
    call(
        "POST",
        &format!("subscriptions/{subscription_id}"),
        &[("items[0][id]", item), ("items[0][quantity]", quantity.to_string()), ("proration_behavior", "always_invoice".to_string())],
    )?;
    Ok(())
}

/// Cancels at once (account closed by the customer).
pub fn cancel(subscription_id: &str) -> Result<()> {
    let auth = format!("Bearer {}", key()?);
    ureq::delete(&format!("https://api.stripe.com/v1/subscriptions/{subscription_id}"))
        .header("Authorization", &auth)
        .call()
        .map_err(|e| CoreError::new("payment_failed", e.to_string()))?;
    Ok(())
}

pub fn subscription(subscription_id: &str) -> Result<Value> {
    call("GET", &format!("subscriptions/{subscription_id}"), &[])
}

/// Verifies the `Stripe-Signature` header (v1 = HMAC-SHA256 of "t.payload"), 5-minute tolerance.
pub fn verify_webhook(payload: &[u8], header: &str, secret: &str, now: i64) -> bool {
    let mut ts = None;
    let mut sigs = vec![];
    for part in header.split(',') {
        match part.split_once('=') {
            Some(("t", v)) => ts = v.parse::<i64>().ok(),
            Some(("v1", v)) => sigs.push(v.to_string()),
            _ => {}
        }
    }
    let Some(ts) = ts else { return false };
    if (now - ts).abs() > 300 {
        return false;
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("key");
    mac.update(format!("{ts}.").as_bytes());
    mac.update(payload);
    let expected = hex::encode(mac.finalize().into_bytes());
    sigs.iter().any(|s| constant_eq(s.as_bytes(), expected.as_bytes()))
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_signature() {
        let secret = "whsec_test";
        let payload = br#"{"id":"evt_1"}"#;
        let t = 1_800_000_000;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(format!("{t}.").as_bytes());
        mac.update(payload);
        let sig = hex::encode(mac.finalize().into_bytes());
        let header = format!("t={t},v1={sig}");
        assert!(verify_webhook(payload, &header, secret, t + 10));
        assert!(!verify_webhook(payload, &header, secret, t + 1000));
        assert!(!verify_webhook(b"{}", &header, secret, t));
        assert!(!verify_webhook(payload, &header, "other", t));
    }

    #[test]
    fn form_encoding() {
        assert_eq!(form(&[("a[0][b]", "x y&z".into())]), "a%5B0%5D%5Bb%5D=x%20y%26z");
    }
}
