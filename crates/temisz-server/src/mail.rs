//! Outgoing e-mail. Every message is logged in `platform.emails`; it is
//! delivered through Resend when RESEND_API_KEY is set (production), and only
//! logged otherwise (development, E2E tests read the log).

use lunda_core::sql::{params, Connection};
use lunda_core::util::new_id;
use lunda_core::Result;

pub fn send(conn: &Connection, to: &str, subject: &str, body: &str, kind: &str) -> Result<()> {
    let id = new_id();
    conn.execute("INSERT INTO platform.emails (id, to_address, subject, body, kind) VALUES (?1, ?2, ?3, ?4, ?5)", params![id, to, subject, body, kind])?;
    let Ok(key) = std::env::var("RESEND_API_KEY") else {
        if cfg!(debug_assertions) {
            eprintln!("[mail] {to}: {subject}\n{body}\n");
        }
        return Ok(());
    };
    let from = std::env::var("MAIL_FROM").unwrap_or_else(|_| "TEMISZ <noreply@temisz.hu>".into());
    let payload = serde_json::json!({ "from": from, "to": [to], "subject": subject, "text": body });
    let res = ureq::post("https://api.resend.com/emails")
        .header("Authorization", &format!("Bearer {key}"))
        .header("Content-Type", "application/json")
        .send(payload.to_string());
    match res {
        Ok(_) => conn.execute("UPDATE platform.emails SET sent_at = now() WHERE id = ?1", params![id])?,
        Err(e) => conn.execute("UPDATE platform.emails SET error = ?2 WHERE id = ?1", params![id, e.to_string()])?,
    };
    Ok(())
}

/// Signature appended to every message.
pub const FOOTER: &str =
    "\n\n--\nTEMISZ – Temetőmenedzsment és Informatikai Szoftver\nhttps://temisz.hu\nEz egy automatikus üzenet, kérjük, ne válaszoljon rá.";
