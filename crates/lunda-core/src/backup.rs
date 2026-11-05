//! Encrypted, verifiable backups (FR-NAP-03..06, JOG-05).
//!
//! A backup is a complete SQLCipher database re-encrypted with the
//! **recovery key** (QUESTIONS Q7), so it can be restored on another machine
//! even if the OS keychain is lost. Next to it a manifest (`.json`) stores
//! the SHA-256 checksum, time, schema and app version.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::audit;
use crate::auth::{Permission, Session};
use crate::error::{CoreError, Result};
use crate::settings;
use crate::util::{new_id, now_ts, sha256_hex};

pub const EXT: &str = "ltbak";
const RECOVERY_KEY: &str = "internal.recovery_key";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub app: String,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: String,
    pub kind: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub sha256: String,
    /// last audit hash at backup time – ties the backup to the change log
    pub audit_head: String,
    pub audit_entries: i64,
    /// SHA-256 of the recovery key – lets the UI tell whether a key matches
    pub key_fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: String,
    pub manifest: Manifest,
    pub external: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub checksum_ok: bool,
    pub opened: bool,
    pub integrity_ok: bool,
    pub audit_chain_ok: bool,
    pub manifest: Option<Manifest>,
    pub message: String,
}

/// Formats a 64-hex recovery key for printing: 8 groups of 8.
pub fn format_recovery_key(k: &str) -> String {
    k.as_bytes().chunks(8).map(|c| String::from_utf8_lossy(c).to_uppercase()).collect::<Vec<_>>().join("-")
}

pub fn parse_recovery_key(input: &str) -> Result<String> {
    let k: String = input.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_lowercase();
    if k.len() != 64 {
        return Err(CoreError::new("recovery_key_invalid", "the recovery key must have 64 hexadecimal characters"));
    }
    Ok(k)
}

pub fn fingerprint(key_hex: &str) -> String {
    sha256_hex(format!("lunda-recovery:{key_hex}").as_bytes())[..16].to_string()
}

/// Returns the recovery key, generating one on first use.
pub fn ensure_recovery_key(conn: &Connection) -> Result<(String, bool)> {
    if let Some(k) = settings::get(conn, RECOVERY_KEY, None)? {
        if !k.is_empty() {
            return Ok((k, false));
        }
    }
    let k = crate::db::generate_key();
    settings::set_internal(conn, RECOVERY_KEY, &k)?;
    Ok((k, true))
}

/// Shows the recovery key to an administrator (for printing). Audited.
pub fn reveal_recovery_key(conn: &Connection, s: &Session) -> Result<String> {
    s.require(Permission::ManageSettings)?;
    let (k, _) = ensure_recovery_key(conn)?;
    audit::record_simple(conn, s, "backup", "recovery_key", "reveal", None)?;
    Ok(format_recovery_key(&k))
}

pub fn default_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

/// Backup directory: the configured one (external) or the local default.
pub fn target_dir(conn: &Connection, data_dir: &Path) -> Result<(PathBuf, bool)> {
    let configured = settings::get(conn, "backup.directory", None)?.unwrap_or_default();
    if configured.trim().is_empty() {
        Ok((default_dir(data_dir), false))
    } else {
        Ok((PathBuf::from(configured.trim()), true))
    }
}

/// Creates a backup. `kind`: auto | exit | manual | pre-migration | pre-restore | pre-update
pub fn create(conn: &Connection, s: &Session, data_dir: &Path, kind: &str) -> Result<BackupInfo> {
    let (dir, external) = target_dir(conn, data_dir)?;
    create_in(conn, s, &dir, external, kind)
}

pub fn create_in(conn: &Connection, s: &Session, dir: &Path, external: bool, kind: &str) -> Result<BackupInfo> {
    if kind == "manual" {
        s.require(Permission::Backup)?;
    }
    let result = (|| -> Result<BackupInfo> {
        std::fs::create_dir_all(dir)?;
        let (key, _) = ensure_recovery_key(conn)?;
        let stamp = Utc::now().format("%Y%m%d-%H%M%S%3f");
        let file_name = format!("lunda-temeto-{stamp}-{kind}.{EXT}");
        let path = dir.join(&file_name);
        let tmp = dir.join(format!("{file_name}.part"));
        let _ = std::fs::remove_file(&tmp);
        conn.execute_batch("PRAGMA wal_checkpoint(FULL);").ok();
        let tmp_s = tmp.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!("ATTACH DATABASE '{tmp_s}' AS bak KEY \"x'{key}'\";"))?;
        let export = conn.query_row("SELECT sqlcipher_export('bak')", [], |_| Ok(()));
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        let set_v = conn.execute_batch(&format!("PRAGMA bak.user_version = {version};"));
        conn.execute_batch("DETACH DATABASE bak;")?;
        export?;
        set_v?;
        std::fs::rename(&tmp, &path)?;
        let bytes = std::fs::read(&path)?;
        let chain = audit::verify_chain(conn)?;
        let manifest = Manifest {
            app: "Lunda Temető".into(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: version,
            created_at: now_ts(),
            kind: kind.into(),
            file_name: file_name.clone(),
            size_bytes: bytes.len() as u64,
            sha256: sha256_hex(&bytes),
            audit_head: chain.last_hash,
            audit_entries: chain.entries,
            key_fingerprint: fingerprint(&key),
        };
        std::fs::write(manifest_path(&path), serde_json::to_vec_pretty(&manifest)?)?;
        Ok(BackupInfo { path: path.display().to_string(), manifest, external })
    })();
    let (ok, path, sha, size, msg) = match &result {
        Ok(b) => (true, b.path.clone(), b.manifest.sha256.clone(), b.manifest.size_bytes as i64, String::new()),
        Err(e) => (false, dir.display().to_string(), String::new(), 0, e.message.clone()),
    };
    conn.execute(
        "INSERT INTO backup_log (id, ts, kind, path, sha256, size_bytes, external, ok, message) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![new_id(), now_ts(), kind, path, sha, size, external, ok, msg],
    )?;
    if let Ok(b) = &result {
        audit::record_simple(conn, s, "backup", &b.manifest.file_name, "create", Some(&b.manifest.sha256))?;
        settings::set_internal(conn, "internal.last_backup_at", &b.manifest.created_at)?;
        prune(conn, dir)?;
    }
    result
}

fn manifest_path(p: &Path) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(".json");
    PathBuf::from(s)
}

/// Keeps the newest N automatic backups (manual and pre-* backups are kept).
fn prune(conn: &Connection, dir: &Path) -> Result<()> {
    let keep = settings::get_i64(conn, "backup.keep_count", None, 30)?.max(1) as usize;
    let mut autos: Vec<PathBuf> = list_files(dir)?
        .into_iter()
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            n.ends_with(&format!("-auto.{EXT}")) || n.ends_with(&format!("-exit.{EXT}"))
        })
        .collect();
    autos.sort();
    if autos.len() > keep {
        for p in &autos[..autos.len() - keep] {
            let _ = std::fs::remove_file(p);
            let _ = std::fs::remove_file(manifest_path(p));
        }
    }
    Ok(())
}

fn list_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut v = vec![];
    for e in std::fs::read_dir(dir)? {
        let p = e?.path();
        if p.extension().map(|x| x == EXT).unwrap_or(false) {
            v.push(p);
        }
    }
    Ok(v)
}

/// Lists backups found in a directory (newest first).
pub fn list(dir: &Path) -> Result<Vec<BackupInfo>> {
    let mut out = vec![];
    for p in list_files(dir)? {
        if let Ok(bytes) = std::fs::read(manifest_path(&p)) {
            if let Ok(m) = serde_json::from_slice::<Manifest>(&bytes) {
                out.push(BackupInfo { path: p.display().to_string(), manifest: m, external: false });
            }
        }
    }
    out.sort_by(|a, b| b.manifest.created_at.cmp(&a.manifest.created_at));
    Ok(out)
}

/// FR-NAP-04: verifies checksum, decryption, SQLite integrity and the audit chain.
pub fn verify(path: &Path, recovery_key: Option<&str>) -> Result<VerifyReport> {
    let manifest: Option<Manifest> = std::fs::read(manifest_path(path)).ok().and_then(|b| serde_json::from_slice(&b).ok());
    let bytes = std::fs::read(path)?;
    let sum = sha256_hex(&bytes);
    let checksum_ok = manifest.as_ref().map(|m| m.sha256 == sum).unwrap_or(false);
    let mut rep = VerifyReport { checksum_ok, opened: false, integrity_ok: false, audit_chain_ok: false, manifest: manifest.clone(), message: String::new() };
    if !checksum_ok {
        rep.message = if manifest.is_none() { "manifest_missing".into() } else { "checksum_mismatch".into() };
    }
    let Some(key) = recovery_key else { return Ok(rep) };
    let key = parse_recovery_key(key)?;
    match crate::db::open_readonly(path, &key) {
        Ok(c) => {
            rep.opened = true;
            let ic: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap_or_default();
            rep.integrity_ok = ic == "ok";
            rep.audit_chain_ok = audit::verify_chain(&c).map(|r| r.ok).unwrap_or(false);
        }
        Err(e) => {
            rep.message = if e.code == "db_key_invalid" { "wrong_recovery_key".into() } else { e.message };
        }
    }
    Ok(rep)
}

/// FR-NAP-05: writes the backup's content into `target_db` encrypted with `db_key`.
/// The caller must close the live connection and take a pre-restore backup first.
pub fn restore_into(backup: &Path, recovery_key: &str, target_db: &Path, db_key: &str) -> Result<()> {
    let rk = parse_recovery_key(recovery_key)?;
    let rep = verify(backup, Some(&rk))?;
    if !rep.opened {
        return Err(CoreError::new("wrong_recovery_key", "the backup cannot be opened with this recovery key"));
    }
    if !rep.integrity_ok {
        return Err(CoreError::new("backup_corrupt", "the backup failed the integrity check"));
    }
    // work on a copy: decrypt with the recovery key, re-key to the local database key
    let tmp = target_db.with_extension("restore.part");
    let _ = std::fs::remove_file(&tmp);
    std::fs::copy(backup, &tmp)?;
    {
        let c = Connection::open(&tmp)?;
        c.execute_batch(&format!("PRAGMA key = \"x'{rk}'\"; PRAGMA rekey = \"x'{db_key}'\";"))?;
        c.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))?;
    }
    for ext in ["-wal", "-shm"] {
        let mut p = target_db.as_os_str().to_owned();
        p.push(ext);
        let _ = std::fs::remove_file(PathBuf::from(p));
    }
    std::fs::rename(&tmp, target_db)?;
    Ok(())
}

/// Should an automatic daily backup run now?
pub fn due(conn: &Connection) -> Result<bool> {
    let last: Option<String> = conn.query_row("SELECT max(ts) FROM backup_log WHERE ok=1", [], |r| r.get(0))?;
    Ok(match last.and_then(|l| DateTime::parse_from_rfc3339(&l).ok()) {
        Some(t) => Utc::now() - t.with_timezone(&Utc) > Duration::hours(24),
        None => true,
    })
}

/// FR-NAP-06: warning when there was no successful *external* backup for N days.
pub fn needs_warning(conn: &Connection) -> Result<bool> {
    let days = settings::get_i64(conn, "backup.warning_days", None, 7)?;
    let last: Option<String> = conn.query_row("SELECT max(ts) FROM backup_log WHERE ok=1 AND external=1", [], |r| r.get(0))?;
    Ok(match last.and_then(|l| DateTime::parse_from_rfc3339(&l).ok()) {
        Some(t) => Utc::now() - t.with_timezone(&Utc) > Duration::days(days),
        None => true,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BackupLogRow {
    pub ts: String,
    pub kind: String,
    pub path: String,
    pub sha256: String,
    pub size_bytes: i64,
    pub external: bool,
    pub ok: bool,
    pub message: String,
}

pub fn log(conn: &Connection, limit: i64) -> Result<Vec<BackupLogRow>> {
    let mut st = conn.prepare("SELECT ts, kind, path, sha256, size_bytes, external, ok, message FROM backup_log ORDER BY ts DESC LIMIT ?1")?;
    let rows = st.query_map([limit], |r| {
        Ok(BackupLogRow {
            ts: r.get(0)?,
            kind: r.get(1)?,
            path: r.get(2)?,
            sha256: r.get(3)?,
            size_bytes: r.get(4)?,
            external: r.get(5)?,
            ok: r.get(6)?,
            message: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<_, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn tmpdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("lunda-bak-{}", new_id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn backup_verify_restore_roundtrip() {
        let dir = tmpdir();
        let key = db::generate_key();
        let dbp = dir.join("live.db");
        let conn = db::open(&dbp, &key, |_, _| Ok(())).unwrap();
        let store = crate::Store::new(conn);
        let s = Session::system();
        crate::sample::load_sample(&store, &s).unwrap();
        let n_events: i64 = store.conn.query_row("SELECT count(*) FROM interment_events", [], |r| r.get(0)).unwrap();

        let info = create_in(&store.conn, &s, &dir.join("bak"), true, "manual").unwrap();
        let path = PathBuf::from(&info.path);
        // not readable as plain SQLite
        assert!(!std::fs::read(&path).unwrap().starts_with(b"SQLite format 3"));
        let rk = settings::get(&store.conn, RECOVERY_KEY, None).unwrap().unwrap();
        let rep = verify(&path, Some(&format_recovery_key(&rk))).unwrap();
        assert!(rep.checksum_ok && rep.opened && rep.integrity_ok && rep.audit_chain_ok, "{rep:?}");
        assert!(!needs_warning(&store.conn).unwrap());
        assert!(!due(&store.conn).unwrap());

        // wrong key
        let bad = verify(&path, Some(&db::generate_key())).unwrap();
        assert!(!bad.opened);
        assert_eq!(bad.message, "wrong_recovery_key");

        // restore onto a new machine with a new database key
        let new_key = db::generate_key();
        let target = dir.join("restored.db");
        restore_into(&path, &rk, &target, &new_key).unwrap();
        let c2 = db::open(&target, &new_key, |_, _| Ok(())).unwrap();
        let n2: i64 = c2.query_row("SELECT count(*) FROM interment_events", [], |r| r.get(0)).unwrap();
        assert_eq!(n_events, n2);
        assert!(audit::verify_chain(&c2).unwrap().ok);

        // tampering is detected by the checksum
        let mut bytes = std::fs::read(&path).unwrap();
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0xff;
        std::fs::write(&path, bytes).unwrap();
        let t = verify(&path, Some(&rk)).unwrap();
        assert!(!t.checksum_ok);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn prune_keeps_n_auto_backups() {
        let dir = tmpdir();
        let store = crate::Store::new(db::open(&dir.join("x.db"), &db::generate_key(), |_, _| Ok(())).unwrap());
        let s = Session::system();
        settings::set_many(&store.conn, &s, None, &[("backup.keep_count".into(), "2".into())].into_iter().collect()).unwrap();
        for _ in 0..4 {
            create_in(&store.conn, &s, &dir.join("b"), false, "auto").unwrap();
        }
        create_in(&store.conn, &s, &dir.join("b"), false, "manual").unwrap();
        let l = list(&dir.join("b")).unwrap();
        assert_eq!(l.iter().filter(|b| b.manifest.kind == "auto").count(), 2);
        assert_eq!(l.iter().filter(|b| b.manifest.kind == "manual").count(), 1);
        // local backups do not satisfy the external-backup check
        assert!(needs_warning(&store.conn).unwrap());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn recovery_key_format() {
        let k = db::generate_key();
        let f = format_recovery_key(&k);
        assert_eq!(f.split('-').count(), 8);
        assert_eq!(parse_recovery_key(&f).unwrap(), k);
        assert!(parse_recovery_key("abc").is_err());
    }
}
