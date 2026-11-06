//! Key/value settings (FR-BEA-01), global or per cemetery.

use crate::sql::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;

use crate::audit;
use crate::auth::{Permission, Session};
use crate::error::{CoreError, Result};
use crate::util::now_ts;

/// Default values, seeded on database creation.
pub const DEFAULTS: &[(&str, &str)] = &[
    ("operator.name", ""),
    ("operator.address", ""),
    ("operator.phone", ""),
    ("operator.email", ""),
    ("operator.tax_number", ""),
    ("operator.logo", ""), // data URL (PNG/JPEG)
    ("signer.name", ""),
    ("signer.title", "temetőgondnok"),
    ("regulation.name", ""), // e.g. "Lajosmizse Város Önkormányzatának 12/2020. (V. 28.) rendelete"
    ("rules.grace_period_days", "180"),
    ("rules.urn_rest_period_years", "0"),
    ("rules.expiry_warning_days", "90"),
    ("rules.rounding", "1"), // HUF
    ("security.login_required", "true"),
    ("security.idle_logout_minutes", "15"),
    ("backup.directory", ""),
    ("backup.keep_count", "30"),
    ("backup.on_exit", "true"),
    ("backup.warning_days", "7"),
    ("update.endpoint", ""), // empty = URL from the build configuration
    ("ui.theme", "light"),
    ("ui.zoom", "100"),
    ("onboarding.completed", "false"),
    ("public.search_enabled", "false"), // FR-UGY-02: opt-in public grave search
    ("notify.auto_expiry", "false"),    // automatic expiry reminders by e-mail
];

/// Keys that only an administrator may change (security-relevant).
const ADMIN_ONLY_PREFIXES: &[&str] = &["security.", "backup.", "update.", "finance.provider", "public."];

pub fn get(conn: &Connection, key: &str, cemetery_id: Option<&str>) -> Result<Option<String>> {
    if let Some(cid) = cemetery_id {
        if let Some(v) = conn.query_row("SELECT value FROM settings WHERE key=?1 AND cemetery_id=?2", params![key, cid], |r| r.get(0)).optional()? {
            return Ok(Some(v));
        }
    }
    Ok(conn.query_row("SELECT value FROM settings WHERE key=?1 AND cemetery_id=''", params![key], |r| r.get(0)).optional()?)
}

pub fn get_or(conn: &Connection, key: &str, default: &str) -> Result<String> {
    Ok(get(conn, key, None)?.unwrap_or_else(|| default.to_string()))
}

pub fn get_i64(conn: &Connection, key: &str, cemetery_id: Option<&str>, default: i64) -> Result<i64> {
    Ok(get(conn, key, cemetery_id)?.and_then(|v| v.trim().parse().ok()).unwrap_or(default))
}

pub fn get_bool(conn: &Connection, key: &str, default: bool) -> Result<bool> {
    Ok(get(conn, key, None)?.map(|v| v == "true" || v == "1").unwrap_or(default))
}

pub fn all(conn: &Connection, cemetery_id: Option<&str>) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut stmt = conn.prepare("SELECT key, value FROM settings WHERE cemetery_id='' AND key NOT LIKE 'internal.%'")?;
    for row in stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (k, v) = row?;
        out.insert(k, v);
    }
    if let Some(cid) = cemetery_id {
        let mut stmt = conn.prepare("SELECT key, value FROM settings WHERE cemetery_id=?1")?;
        for row in stmt.query_map([cid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (k, v) = row?;
            out.insert(k, v);
        }
    }
    Ok(out)
}

/// Sets values (audited). Internal keys (`internal.*`) can only be set by the system.
pub fn set_many(conn: &Connection, session: &Session, cemetery_id: Option<&str>, values: &BTreeMap<String, String>) -> Result<()> {
    session.require(Permission::ManageSettings)?;
    let cid = cemetery_id.unwrap_or("");
    let tx = crate::db::tx(conn)?;
    for (k, v) in values {
        if k.starts_with("internal.") && session.user_id.is_some() {
            return Err(CoreError::forbidden("internal_settings"));
        }
        if ADMIN_ONLY_PREFIXES.iter().any(|p| k.starts_with(p)) && session.role != crate::auth::Role::Admin {
            return Err(CoreError::forbidden("manage_settings"));
        }
        validate(k, v)?;
        let old: Option<String> = tx.query_row("SELECT value FROM settings WHERE key=?1 AND cemetery_id=?2", params![k, cid], |r| r.get(0)).optional()?;
        if old.as_deref() == Some(v.as_str()) {
            continue;
        }
        tx.execute(
            "INSERT INTO settings (key, cemetery_id, value, updated_at, updated_by) VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(key, cemetery_id) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at, updated_by=excluded.updated_by",
            params![k, cid, v, now_ts(), session.user_name],
        )?;
        // do not bloat the log with logo binary data
        let (o, n) = if k == "operator.logo" { (old.map(|_| "<logo>".to_string()), Some("<logo>".to_string())) } else { (old, Some(v.clone())) };
        audit::record(&tx, session, "setting", &format!("{cid}:{k}"), "update", o.as_ref(), n.as_ref(), None)?;
    }
    tx.commit()?;
    Ok(())
}

/// System-level setter for internal bookkeeping (e.g. last backup time). Not audited.
pub fn set_internal(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, cemetery_id, value, updated_at, updated_by) VALUES (?1,'',?2,?3,'system')
         ON CONFLICT(key, cemetery_id) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        params![key, value, now_ts()],
    )?;
    Ok(())
}

fn validate(key: &str, value: &str) -> Result<()> {
    let numeric = [
        "rules.grace_period_days",
        "rules.urn_rest_period_years",
        "rules.expiry_warning_days",
        "security.idle_logout_minutes",
        "backup.keep_count",
        "backup.warning_days",
        "ui.zoom",
    ];
    if numeric.contains(&key) && value.trim().parse::<i64>().map(|n| n < 0).unwrap_or(true) {
        return Err(CoreError::validation(key, "must be a non-negative integer"));
    }
    if matches!(key, "public.search_enabled" | "notify.auto_expiry") && !matches!(value, "true" | "false") {
        return Err(CoreError::validation(key, "true or false"));
    }
    if key == "finance.provider" && !matches!(value, "szamlazz" | "asp") {
        return Err(CoreError::validation(key, "szamlazz or asp"));
    }
    if key == "finance.payment_due_days" && value.trim().parse::<i64>().map(|n| !(0..=90).contains(&n)).unwrap_or(true) {
        return Err(CoreError::validation(key, "0–90 days"));
    }
    if key == "finance.default_vat" {
        crate::store::finance::check_vat(value)?;
    }
    if key == "update.endpoint" && !value.trim().is_empty() && !value.trim().starts_with("https://") {
        return Err(CoreError::validation(key, "the update server must use https"));
    }
    if key == "backup.keep_count" && value.trim().parse::<i64>().unwrap_or(0) < 1 {
        return Err(CoreError::validation(key, "must keep at least one backup"));
    }
    Ok(())
}
