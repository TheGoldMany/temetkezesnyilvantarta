//! Free migration demo (temisz.hu/bemutato): a municipality uploads one of
//! its Excel/Word tables, TEMISZ loads it into a trial register and shows
//! it within 24 hours. The file holds personal data (the deceased, holders):
//! it is stored encrypted, only the platform admins can download it, and it
//! is deleted after 30 days (or when the admin deletes it).

use base64::Engine;
use lunda_core::sql::{params, Connection, OptionalExtension};
use lunda_core::util::new_id;
use lunda_core::{CoreError, Result};
use serde_json::{json, Value};

use crate::{lifecycle, mail, Request};

pub const MAX_FILE_BYTES: usize = 3 * 1024 * 1024;
pub const KEEP_DAYS: i32 = 30;
const ALLOWED: &[&str] = &["xlsx", "xls", "xlsm", "ods", "csv", "docx", "doc", "odt", "rtf", "txt", "pdf"];

fn field(v: &Value, k: &str, max: usize) -> String {
    v[k].as_str().unwrap_or("").trim().chars().take(max).collect()
}

/// Who is told about a new request: DEMO_REQUEST_EMAIL, else the platform admins.
fn notify_to() -> Vec<String> {
    let list =
        std::env::var("DEMO_REQUEST_EMAIL").ok().filter(|v| !v.trim().is_empty()).or_else(|| std::env::var("SUPERADMIN_EMAILS").ok()).unwrap_or_default();
    list.split(',').map(|e| e.trim().to_string()).filter(|e| crate::valid_email(e)).collect()
}

/// The public form: settlement, contact, consent and the file (base64).
pub fn submit(conn: &Connection, req: &Request) -> Result<Value> {
    // a JSON body only: a cross-site form cannot post here
    if !req.header("content-type").map(|c| c.starts_with("application/json")).unwrap_or(false) {
        return Err(CoreError::new("forbidden", "json required"));
    }
    crate::accounts::rate_limit(conn, &format!("demo:{}", req.ip), 5, 60)?;
    let v: Value = serde_json::from_slice(&req.body).map_err(|_| CoreError::new("bad_request", "invalid request"))?;
    let settlement = field(&v, "settlement", 120);
    let name = field(&v, "name", 120);
    let role = field(&v, "role", 80);
    let email = field(&v, "email", 200).to_lowercase();
    let phone = field(&v, "phone", 40);
    let message = field(&v, "message", 2000);
    let file_name = field(&v, "fileName", 200).replace(['/', '\\'], "_");
    for (k, val) in [("settlement", &settlement), ("name", &name)] {
        if val.chars().count() < 2 {
            return Err(CoreError::validation(k, "required").with("code", "required"));
        }
    }
    if !crate::valid_email(&email) {
        return Err(CoreError::validation("email", "invalid e-mail address").with("code", "email_invalid"));
    }
    if v["consent"].as_bool() != Some(true) {
        return Err(CoreError::new("consent_required", "the data processing terms must be accepted"));
    }
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    if !file_name.contains('.') || !ALLOWED.contains(&ext.as_str()) {
        return Err(CoreError::new("file_type", "Excel, Word, CSV or PDF file expected"));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(v["content"].as_str().unwrap_or("").as_bytes())
        .map_err(|_| CoreError::new("bad_request", "invalid file"))?;
    if bytes.is_empty() {
        return Err(CoreError::new("file_missing", "no file"));
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(CoreError::new("file_too_large", "file too large").with("maxMb", "3"));
    }
    let (stored, sealed) = match lifecycle::secrets_key() {
        Some(k) => (lifecycle::seal(&k, &bytes), true),
        None => (bytes.clone(), false),
    };
    let id = new_id();
    conn.execute(
        "INSERT INTO platform.demo_requests (id, settlement, contact_name, contact_role, email, phone, message, file_name, file_size, file_data, sealed, ip)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![id, settlement, name, role, email, phone, message, file_name, bytes.len() as i64, stored, sealed, req.ip],
    )?;
    let summary = format!(
        "Település: {settlement}\nKapcsolattartó: {name}{}\nE-mail: {email}\nTelefon: {}\nFájl: {file_name} ({} kB)\n\n{message}",
        if role.is_empty() { String::new() } else { format!(" ({role})") },
        if phone.is_empty() { "–" } else { &phone },
        bytes.len().div_ceil(1024)
    );
    for to in notify_to() {
        mail::send(
            conn,
            &to,
            &format!("Bemutató-kérés: {settlement}"),
            &format!("Új ingyenes bemutató-kérés érkezett.\n\n{summary}\n\nA fájl a programban, az Ügyfelek oldalon tölthető le.{}", mail::FOOTER),
            "demo_request",
        )?;
    }
    mail::send(
        conn,
        &email,
        "Megkaptuk a táblázatát – TEMISZ bemutató",
        &format!(
            "Jó napot kívánunk!\n\nKöszönjük, megkaptuk a(z) {settlement} temetőjének táblázatát ({file_name}). Egy munkanapon belül betöltjük egy próbanyilvántartásba, és e-mailben küldjük a belépést, hogy a saját adataival nézhesse meg a TEMISZ-t. Ha kérdése van, válaszoljon erre a levélre, vagy hívjon: +36 20 298 2210.\n\nA feltöltött fájlt titkosítva tároljuk, csak a bemutatóhoz használjuk, és legkésőbb {KEEP_DAYS} nap múlva töröljük.{}",
            mail::FOOTER
        ),
        "demo_request_ack",
    )?;
    Ok(json!({ "ok": true }))
}

/// Platform admin: the requests, newest first (without the files).
pub fn list(conn: &Connection) -> Result<Value> {
    let text: String = conn.query_row(
        "SELECT coalesce(json_agg(json_build_object('id', id, 'createdAt', created_at, 'settlement', settlement, 'name', contact_name, 'role', contact_role,
                 'email', email, 'phone', phone, 'message', message, 'fileName', file_name, 'fileSize', file_size, 'done', done_at IS NOT NULL)
                 ORDER BY created_at DESC), '[]')::text
         FROM platform.demo_requests",
        [],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&text)?)
}

/// Platform admin: the uploaded file (base64, for the browser to save).
pub fn download(conn: &Connection, id: &str) -> Result<Value> {
    let row: Option<(String, Vec<u8>, bool)> = conn
        .query_row("SELECT file_name, file_data, sealed FROM platform.demo_requests WHERE id = ?1", params![id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    let (name, data, sealed) = row.ok_or_else(|| CoreError::not_found("demo_request", id))?;
    let plain = if sealed {
        let key = lifecycle::secrets_key().ok_or_else(|| CoreError::new("secrets_key_missing", "SECRETS_KEY or CRON_SECRET is not set"))?;
        lifecycle::open_sealed(&key, &data)?
    } else {
        data
    };
    Ok(json!({ "fileName": name, "data": base64::engine::general_purpose::STANDARD.encode(plain), "path": "", "pages": 0, "sha256": "" }))
}

pub fn mark_done(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE platform.demo_requests SET done_at = coalesce(done_at, now()) WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM platform.demo_requests WHERE id = ?1", params![id])?;
    Ok(())
}

/// Daily: uploads older than 30 days are deleted.
pub fn purge(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("DELETE FROM platform.demo_requests WHERE created_at < now() - make_interval(days => ?1)", params![KEEP_DAYS])?)
}
