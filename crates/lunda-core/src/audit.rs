//! Immutable, hash-chained change log (FR-NAP-01).
//!
//! hash(n) = SHA-256(prev_hash ␟ ts ␟ user_id ␟ user_name ␟ entity ␟ entity_id ␟ action ␟ old ␟ new ␟ reason)
//! The first entry chains to 64 zeros. Any modification, deletion or
//! reordering of entries breaks the chain and is detected by [`verify_chain`].
//! UPDATE/DELETE on the table are additionally blocked by triggers.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::auth::Session;
use crate::error::Result;
use crate::util::{now_ts, sha256_hex};

pub const GENESIS: &str = "0000000000000000000000000000000000000000000000000000000000000000";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub seq: i64,
    pub ts: String,
    pub user_id: Option<String>,
    pub user_name: String,
    pub entity: String,
    pub entity_id: String,
    pub action: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub reason: Option<String>,
    pub prev_hash: String,
    pub hash: String,
}

#[allow(clippy::too_many_arguments)]
pub fn compute_hash(
    prev_hash: &str,
    ts: &str,
    user_id: Option<&str>,
    user_name: &str,
    entity: &str,
    entity_id: &str,
    action: &str,
    old: Option<&str>,
    new: Option<&str>,
    reason: Option<&str>,
) -> String {
    let sep = "\u{1f}";
    let payload = [
        prev_hash,
        ts,
        user_id.unwrap_or(""),
        user_name,
        entity,
        entity_id,
        action,
        old.unwrap_or("\u{0}"),
        new.unwrap_or("\u{0}"),
        reason.unwrap_or("\u{0}"),
    ]
    .join(sep);
    sha256_hex(payload.as_bytes())
}

fn last_hash(conn: &Connection) -> Result<String> {
    Ok(conn
        .query_row("SELECT hash FROM audit_log ORDER BY seq DESC LIMIT 1", [], |r| r.get(0))
        .optional()?
        .unwrap_or_else(|| GENESIS.to_string()))
}

/// Appends an entry. Must be called inside the same transaction as the change.
pub fn record<O: Serialize, N: Serialize>(
    conn: &Connection,
    session: &Session,
    entity: &str,
    entity_id: &str,
    action: &str,
    old: Option<&O>,
    new: Option<&N>,
    reason: Option<&str>,
) -> Result<String> {
    let old_s = old.map(serde_json::to_string).transpose()?;
    let new_s = new.map(serde_json::to_string).transpose()?;
    let ts = now_ts();
    let prev = last_hash(conn)?;
    let reason = reason.map(str::trim).filter(|r| !r.is_empty());
    let hash = compute_hash(
        &prev,
        &ts,
        session.user_id.as_deref(),
        &session.user_name,
        entity,
        entity_id,
        action,
        old_s.as_deref(),
        new_s.as_deref(),
        reason,
    );
    conn.execute(
        "INSERT INTO audit_log (ts, user_id, user_name, entity, entity_id, action, old_value, new_value, reason, prev_hash, hash)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![ts, session.user_id, session.user_name, entity, entity_id, action, old_s, new_s, reason, prev, hash],
    )?;
    Ok(hash)
}

/// Convenience for events without payload (login, view of personal data, print...).
pub fn record_simple(conn: &Connection, session: &Session, entity: &str, entity_id: &str, action: &str, detail: Option<&str>) -> Result<String> {
    record::<(), String>(conn, session, entity, entity_id, action, None, detail.map(|d| d.to_string()).as_ref(), None)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChainReport {
    pub ok: bool,
    pub entries: i64,
    /// first broken sequence number, if any
    pub broken_at: Option<i64>,
    pub last_hash: String,
}

/// Recomputes the full chain (FR-NAP-01 verification).
pub fn verify_chain(conn: &Connection) -> Result<ChainReport> {
    let mut stmt = conn.prepare(
        "SELECT seq, ts, user_id, user_name, entity, entity_id, action, old_value, new_value, reason, prev_hash, hash
         FROM audit_log ORDER BY seq",
    )?;
    let mut rows = stmt.query([])?;
    let mut prev = GENESIS.to_string();
    let mut n = 0;
    while let Some(r) = rows.next()? {
        n += 1;
        let seq: i64 = r.get(0)?;
        let stored_prev: String = r.get(10)?;
        let stored_hash: String = r.get(11)?;
        let h = compute_hash(
            &prev,
            &r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?.as_deref(),
            &r.get::<_, String>(3)?,
            &r.get::<_, String>(4)?,
            &r.get::<_, String>(5)?,
            &r.get::<_, String>(6)?,
            r.get::<_, Option<String>>(7)?.as_deref(),
            r.get::<_, Option<String>>(8)?.as_deref(),
            r.get::<_, Option<String>>(9)?.as_deref(),
        );
        if stored_prev != prev || stored_hash != h {
            return Ok(ChainReport { ok: false, entries: n, broken_at: Some(seq), last_hash: prev });
        }
        prev = stored_hash;
    }
    Ok(ChainReport { ok: true, entries: n, broken_at: None, last_hash: prev })
}

pub fn list(conn: &Connection, entity: Option<&str>, entity_id: Option<&str>, limit: i64, before_seq: Option<i64>) -> Result<Vec<AuditEntry>> {
    let mut stmt = conn.prepare(
        "SELECT seq, ts, user_id, user_name, entity, entity_id, action, old_value, new_value, reason, prev_hash, hash
         FROM audit_log
         WHERE (?1 IS NULL OR entity = ?1) AND (?2 IS NULL OR entity_id = ?2) AND (?3 IS NULL OR seq < ?3)
         ORDER BY seq DESC LIMIT ?4",
    )?;
    let rows = stmt.query_map(params![entity, entity_id, before_seq, limit], |r| {
        Ok(AuditEntry {
            seq: r.get(0)?,
            ts: r.get(1)?,
            user_id: r.get(2)?,
            user_name: r.get(3)?,
            entity: r.get(4)?,
            entity_id: r.get(5)?,
            action: r.get(6)?,
            old_value: r.get(7)?,
            new_value: r.get(8)?,
            reason: r.get(9)?,
            prev_hash: r.get(10)?,
            hash: r.get(11)?,
        })
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn seed(conn: &Connection, n: usize) {
        let s = Session::system();
        for i in 0..n {
            record(conn, &s, "plot", &format!("p{i}"), "create", None::<&()>, Some(&serde_json::json!({"i": i})), None).unwrap();
        }
    }

    #[test]
    fn chain_verifies() {
        let c = open_in_memory().unwrap();
        seed(&c, 20);
        let r = verify_chain(&c).unwrap();
        assert!(r.ok);
        assert_eq!(r.entries, 20);
        let first: String = c.query_row("SELECT prev_hash FROM audit_log ORDER BY seq LIMIT 1", [], |r| r.get(0)).unwrap();
        assert_eq!(first, GENESIS);
    }

    #[test]
    fn triggers_block_update_and_delete() {
        let c = open_in_memory().unwrap();
        seed(&c, 3);
        assert!(c.execute("UPDATE audit_log SET action='x' WHERE seq=2", []).is_err());
        assert!(c.execute("DELETE FROM audit_log WHERE seq=2", []).is_err());
    }

    #[test]
    fn tampering_is_detected() {
        let c = open_in_memory().unwrap();
        seed(&c, 5);
        // simulate an attacker who bypasses the triggers
        c.execute_batch("DROP TRIGGER audit_log_no_update; UPDATE audit_log SET new_value='{\"i\":99}' WHERE seq=3;").unwrap();
        let r = verify_chain(&c).unwrap();
        assert!(!r.ok);
        assert_eq!(r.broken_at, Some(3));
    }

    #[test]
    fn deletion_is_detected() {
        let c = open_in_memory().unwrap();
        seed(&c, 5);
        c.execute_batch("DROP TRIGGER audit_log_no_delete; DELETE FROM audit_log WHERE seq=2;").unwrap();
        let r = verify_chain(&c).unwrap();
        assert!(!r.ok);
        assert_eq!(r.broken_at, Some(3));
    }

    #[test]
    fn hash_depends_on_every_field() {
        let base = compute_hash(GENESIS, "t", Some("u"), "n", "e", "id", "a", Some("o"), Some("n"), Some("r"));
        assert_ne!(base, compute_hash(GENESIS, "t", Some("u"), "n", "e", "id", "a", Some("o"), Some("n"), None));
        assert_ne!(base, compute_hash(GENESIS, "t", None, "n", "e", "id", "a", Some("o"), Some("n"), Some("r")));
        assert_ne!(base, compute_hash(GENESIS, "t2", Some("u"), "n", "e", "id", "a", Some("o"), Some("n"), Some("r")));
    }
}
