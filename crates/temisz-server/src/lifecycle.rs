//! Account lifecycle (W4): closing on request, deletion after 30 days,
//! trial accounts never paid warned at 60 and removed at 90 days of being
//! read-only (customers who have paid before are never deleted automatically:
//! the platform admin decides), daily encrypted exports.
//! Invoices and billing details stay (accounting law), register data and
//! personal data go.

use lunda_core::auth::{Permission, Session};
use lunda_core::sql::{params, Connection};
use lunda_core::{CoreError, Result, Store};
use serde_json::{json, Value};

use crate::{mail, tenant_data};

pub const DELETE_AFTER_CLOSE_DAYS: i64 = 30;
pub const WARN_READ_ONLY_DAYS: i64 = 60;
pub const DELETE_READ_ONLY_DAYS: i64 = 90;

/// The admin closes the account (password confirmed). Sign-in stops at once;
/// the data are deleted after 30 days unless the customer asks us back.
pub fn close(store: &Store, s: &Session, tenant_id: &str, password: &str) -> Result<()> {
    s.require(Permission::ManageSettings)?;
    if s.role != lunda_core::auth::Role::Admin {
        return Err(CoreError::forbidden("admin"));
    }
    store.verify_own_password(s, password)?;
    let conn = &store.conn;
    let t = lunda_core::db::tx(conn)?;
    lunda_core::audit::record_simple(&t, s, "account", tenant_id, "close", None)?;
    t.execute("UPDATE platform.tenants SET status = 'closed', closed_at = now() WHERE id = ?1", params![tenant_id])?;
    t.execute("UPDATE platform.subscriptions SET status = 'canceled', updated_at = now() WHERE tenant_id = ?1", params![tenant_id])?;
    t.execute("DELETE FROM platform.sessions WHERE account_id IN (SELECT id FROM platform.accounts WHERE tenant_id = ?1)", params![tenant_id])?;
    let (to, org): (String, String) =
        t.query_row("SELECT billing_email, name FROM platform.tenants WHERE id = ?1", params![tenant_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
    mail::send(
        &t,
        &to,
        "A TEMISZ-fiók megszűnt",
        &format!(
            "Jó napot kívánunk!\n\nA(z) {org} TEMISZ-fiókját megszüntettük. A nyilvántartás adatait {DELETE_AFTER_CLOSE_DAYS} nap múlva véglegesen töröljük; addig egy válaszlevélben kérheti a fiók visszaállítását. A számlákat a számviteli törvény szerint megőrizzük.{}",
            mail::FOOTER
        ),
        "closed",
    )?;
    t.commit()?;
    Ok(())
}

/// Removes the register (schema) and the people's sign-in data; keeps the
/// tenant row, billing details and invoices.
pub fn delete_data(conn: &Connection, tenant_id: &str) -> Result<()> {
    let schema: String = conn.query_row("SELECT schema_name FROM platform.tenants WHERE id = ?1", params![tenant_id], |r| r.get(0))?;
    let t = lunda_core::db::tx(conn)?;
    t.execute("DELETE FROM platform.sessions WHERE account_id IN (SELECT id FROM platform.accounts WHERE tenant_id = ?1)", params![tenant_id])?;
    t.execute("DELETE FROM platform.tokens WHERE account_id IN (SELECT id FROM platform.accounts WHERE tenant_id = ?1)", params![tenant_id])?;
    t.execute("DELETE FROM platform.accounts WHERE tenant_id = ?1", params![tenant_id])?;
    t.execute("DELETE FROM platform.notifications WHERE tenant_id = ?1", params![tenant_id])?;
    t.execute_batch(&format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"))?;
    t.execute("UPDATE platform.tenants SET status = 'closed', deleted_at = now(), closed_at = coalesce(closed_at, now()) WHERE id = ?1", params![tenant_id])?;
    t.commit()?;
    Ok(())
}

/// Daily: deletions, warnings, and an export of every live tenant to Vercel Blob
/// (when BLOB_READ_WRITE_TOKEN is set; Neon's point-in-time restore is the first line).
pub fn daily(store: &Store) -> Result<Value> {
    let conn = &store.conn;
    let closed: Vec<String> = conn
        .prepare("SELECT id FROM platform.tenants WHERE status = 'closed' AND deleted_at IS NULL AND closed_at < now() - make_interval(days => ?1)")?
        .query_map(params![DELETE_AFTER_CLOSE_DAYS as i32], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    for t in &closed {
        delete_data(conn, t)?;
    }
    let warn: Vec<(String, String, String)> = conn
        .prepare(
            "SELECT id, billing_email, name FROM platform.tenants
             WHERE status = 'read_only' AND read_only_since < now() - make_interval(days => ?1) AND NOT EXISTS (SELECT 1 FROM platform.invoices i WHERE i.tenant_id = platform.tenants.id AND i.kind = 'invoice')",
        )?
        .query_map(params![WARN_READ_ONLY_DAYS as i32], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut warned = 0;
    for (id, to, org) in &warn {
        if conn.execute("INSERT INTO platform.notifications (tenant_id, key) VALUES (?1, 'deletion_warning') ON CONFLICT DO NOTHING", params![id])? == 1 {
            mail::send(
                conn,
                to,
                "A TEMISZ-fiók adatai 30 nap múlva törlődnek",
                &format!(
                    "Jó napot kívánunk!\n\nA(z) {org} TEMISZ-fiókja {WARN_READ_ONLY_DAYS} napja csak olvasható. Ha {} napon belül nem fizet elő, a nyilvántartás adatait véglegesen töröljük. Addig az adatok letölthetők (Beállítások › Mentés és adatletöltés).{}",
                    DELETE_READ_ONLY_DAYS - WARN_READ_ONLY_DAYS,
                    mail::FOOTER
                ),
                "deletion_warning",
            )?;
            warned += 1;
        }
    }
    let expired: Vec<String> = conn
        .prepare("SELECT id FROM platform.tenants WHERE status = 'read_only' AND read_only_since < now() - make_interval(days => ?1) AND NOT EXISTS (SELECT 1 FROM platform.invoices i WHERE i.tenant_id = platform.tenants.id AND i.kind = 'invoice')")?
        .query_map(params![DELETE_READ_ONLY_DAYS as i32], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    for t in &expired {
        delete_data(conn, t)?;
    }
    // former paying customers stay; the platform admin is told once
    let stuck: Vec<(String, String)> = conn
        .prepare(
            "SELECT id, name FROM platform.tenants WHERE status = 'read_only' AND read_only_since < now() - make_interval(days => ?1)
               AND EXISTS (SELECT 1 FROM platform.invoices i WHERE i.tenant_id = platform.tenants.id AND i.kind = 'invoice')",
        )?
        .query_map(params![WARN_READ_ONLY_DAYS as i32], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let admins: Vec<String> =
        std::env::var("SUPERADMIN_EMAILS").unwrap_or_default().split(',').map(|e| e.trim().to_string()).filter(|e| !e.is_empty()).collect();
    for (id, org) in &stuck {
        if conn.execute("INSERT INTO platform.notifications (tenant_id, key) VALUES (?1, 'admin_read_only_long') ON CONFLICT DO NOTHING", params![id])? == 1 {
            for to in &admins {
                mail::send(
                    conn,
                    to,
                    &format!("TEMISZ: {org} {WARN_READ_ONLY_DAYS} napja csak olvasható"),
                    &format!("A(z) {org} korábban fizető ügyfél, a fiókja {WARN_READ_ONLY_DAYS} napja csak olvasható (nincs kifizetve a díjbekérő). Az adatait a rendszer nem törli magától: egyeztessen az ügyféllel, és döntsön a fiókról az Ügyfelek oldalon."),
                    "admin_notice",
                )?;
            }
        }
    }
    let exported = export_all(store)?;
    Ok(json!({ "deletedClosed": closed.len(), "deletionWarnings": warned, "deletedUnpaid": expired.len(), "exported": exported }))
}

/// Key of the backup encryption: BACKUP_KEY, else CRON_SECRET (keep it: old
/// backups can only be opened with the key they were made with).
pub fn backup_key() -> Option<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let secret =
        std::env::var("BACKUP_KEY").ok().filter(|k| !k.trim().is_empty()).or_else(|| std::env::var("CRON_SECRET").ok().filter(|k| !k.trim().is_empty()))?;
    Some(Sha256::digest(format!("temisz-backup:{}", secret.trim()).as_bytes()).into())
}

const SEAL_MAGIC: &[u8] = b"TEMISZ-BACKUP-1\n";

/// AES-256-GCM: magic, 12-byte nonce, ciphertext with tag.
pub fn seal(key: &[u8; 32], plain: &[u8]) -> Vec<u8> {
    use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
    let cipher = aes_gcm::Aes256Gcm::new(key.into());
    let nonce = aes_gcm::Aes256Gcm::generate_nonce(&mut OsRng);
    let mut out = SEAL_MAGIC.to_vec();
    out.extend_from_slice(&nonce);
    out.extend(cipher.encrypt(&nonce, plain).expect("in-memory encryption"));
    out
}

pub fn open_sealed(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    use aes_gcm::aead::{Aead, KeyInit};
    let bad = || CoreError::new("backup_unreadable", "not a TEMISZ backup or wrong key");
    let rest = data.strip_prefix(SEAL_MAGIC).ok_or_else(bad)?;
    if rest.len() < 12 {
        return Err(bad());
    }
    let (nonce, ct) = rest.split_at(12);
    aes_gcm::Aes256Gcm::new(key.into()).decrypt(nonce.into(), ct).map_err(|_| bad())
}

fn export_all(store: &Store) -> Result<usize> {
    let Ok(token) = std::env::var("BLOB_READ_WRITE_TOKEN") else { return Ok(0) };
    // personal data never leave unencrypted
    let Some(key) = backup_key() else {
        eprintln!("backup skipped: set BACKUP_KEY (or CRON_SECRET)");
        return Ok(0);
    };
    let tenants: Vec<(String, String)> = store
        .conn
        .prepare("SELECT id, schema_name FROM platform.tenants WHERE deleted_at IS NULL AND status <> 'closed'")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let day = chrono::Utc::now().format("%d");
    let mut n = 0;
    for (id, schema) in tenants {
        store.conn.use_schema(&schema)?;
        let data = seal(&key, tenant_data::export(&store.conn)?.to_string().as_bytes());
        // backups/<tenant>/<day of month>.json.enc – a rolling month of daily copies
        let url = format!("https://blob.vercel-storage.com/backups/{id}/{day}.json.enc");
        let res = ureq::put(&url)
            .header("Authorization", &format!("Bearer {token}"))
            .header("x-api-version", "7")
            .header("x-add-random-suffix", "0")
            .header("x-allow-overwrite", "1")
            .header("x-content-type", "application/octet-stream")
            .send(&data[..]);
        if res.is_ok() {
            n += 1;
        } else if let Err(e) = res {
            eprintln!("backup upload failed for {id}: {e}");
        }
    }
    Ok(n)
}
