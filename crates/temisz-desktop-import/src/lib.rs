//! Reads a backup of the retired desktop app: a SQLCipher database encrypted
//! with the 64-hex recovery key (raw key), schema version 1. Every table is
//! returned as a JSON array of row objects, ready for
//! `json_populate_recordset` on the Postgres side.

use std::collections::BTreeMap;
use std::path::Path;

use lunda_core::{CoreError, Result};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use serde_json::{json, Map, Value};

/// Desktop tables in an order that satisfies foreign keys.
pub const TABLES: &[&str] = &[
    "settings",
    "users",
    "cemeteries",
    "plot_types",
    "sections",
    "plot_rows",
    "plots",
    "crypt_slots",
    "plot_restrictions",
    "deceased",
    "persons",
    "register_sequences",
    "interment_events",
    "plot_holders",
    "fee_rates",
    "leases",
    "notices",
    "monuments",
    "external_placements",
    "attachments",
    "audit_log",
    "print_log",
    "backup_log",
    "search_index",
];

pub fn parse_recovery_key(input: &str) -> Result<String> {
    let k: String = input.chars().filter(|c| c.is_ascii_hexdigit()).collect::<String>().to_lowercase();
    if k.len() != 64 {
        return Err(CoreError::new("recovery_key_invalid", "the recovery key must have 64 hexadecimal characters"));
    }
    Ok(k)
}

fn db_err(e: rusqlite::Error) -> CoreError {
    let m = e.to_string();
    if m.contains("file is not a database") || m.contains("not a database") {
        CoreError::new("wrong_recovery_key", "the backup cannot be opened with this recovery key")
    } else {
        CoreError::new("backup_corrupt", m)
    }
}

/// Opens the backup with the recovery key and reads every table.
pub fn read_backup(path: &Path, recovery_key: &str) -> Result<BTreeMap<String, Value>> {
    let key = parse_recovery_key(recovery_key)?;
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).map_err(db_err)?;
    conn.execute_batch(&format!("PRAGMA key = \"x'{key}'\";")).map_err(db_err)?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).map_err(db_err)?;
    conn.create_collation("HU", lunda_core::text::hu_compare).map_err(db_err)?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(db_err)?;
    if version != 1 {
        return Err(CoreError::new("backup_version", format!("unsupported backup schema version {version}")).with("version", version));
    }
    let mut out = BTreeMap::new();
    for t in TABLES {
        let exists: i64 = conn.query_row("SELECT count(*) FROM sqlite_master WHERE name = ?1", [t], |r| r.get(0)).map_err(db_err)?;
        if exists == 0 {
            continue;
        }
        let mut st = conn.prepare(&format!("SELECT * FROM {t}")).map_err(db_err)?;
        let cols: Vec<String> = st.column_names().map(String::from).collect();
        let mut rows = st.query([]).map_err(db_err)?;
        let mut arr = vec![];
        while let Some(r) = rows.next().map_err(db_err)? {
            let mut obj = Map::new();
            for (i, c) in cols.iter().enumerate() {
                let v = match r.get_ref(i).map_err(db_err)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(f) => json!(f),
                    ValueRef::Text(s) => json!(String::from_utf8_lossy(s)),
                    // bytea text form for json_populate_recordset
                    ValueRef::Blob(b) => json!(format!("\\x{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>())),
                };
                obj.insert(c.clone(), v);
            }
            arr.push(Value::Object(obj));
        }
        out.insert(t.to_string(), Value::Array(arr));
    }
    Ok(out)
}
