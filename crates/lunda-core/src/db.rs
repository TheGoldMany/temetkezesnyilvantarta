//! Encrypted database: opening, keying, pragmas, migrations, seed data.

use rusqlite::{params, Connection, OpenFlags};
use std::path::Path;

use crate::error::{CoreError, Result};
use crate::text::hu_compare;
use crate::util::{new_id, now_ts};

/// Ordered list of migrations. Never edit a released migration – add a new one.
pub const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/0001_initial.sql"))];

pub fn latest_version() -> i64 {
    MIGRATIONS.last().map(|m| m.0).unwrap_or(0)
}

/// Generates a random 256-bit database key as hex.
pub fn generate_key() -> String {
    use rand::RngCore;
    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    hex::encode(k)
}

fn apply_key(conn: &Connection, key_hex: &str) -> Result<()> {
    if key_hex.len() != 64 || !key_hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(CoreError::new("db_key_invalid", "key must be 64 hex characters"));
    }
    // raw key form: x'..' – no KDF needed, key is already random 256-bit
    conn.execute_batch(&format!("PRAGMA key = \"x'{key_hex}'\";"))?;
    Ok(())
}

fn configure(conn: &Connection) -> Result<()> {
    // Verifies the key: fails with "file is not a database" on a wrong key.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    // Decrypted page cache: SQLCipher decrypts every page it reads, so a
    // generous cache (64 MB) keeps large cemeteries fast (NFR-TEL-02).
    conn.pragma_update(None, "cache_size", -65536)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.create_collation("HU", hu_compare)?;
    Ok(())
}

/// Opens (or creates) an encrypted database file and runs pending migrations.
/// `before_migrate` is invoked with the current version when a migration is
/// pending on an existing database (used to take a backup, NFR-KAR-01).
pub fn open(path: &Path, key_hex: &str, before_migrate: impl FnOnce(&Connection, i64) -> Result<()>) -> Result<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    apply_key(&conn, key_hex)?;
    configure(&conn)?;
    let v = user_version(&conn)?;
    if v > 0 && v < latest_version() {
        before_migrate(&conn, v)?;
    }
    if v > latest_version() {
        return Err(CoreError::new("db_too_new", "database was created by a newer version").with("version", v));
    }
    migrate(&conn)?;
    Ok(conn)
}

/// Opens an encrypted database without migrating (used to verify backups).
pub fn open_readonly(path: &Path, key_hex: &str) -> Result<Connection> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    apply_key(&conn, key_hex)?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))?;
    conn.create_collation("HU", hu_compare)?;
    Ok(conn)
}

/// In-memory encrypted database for tests.
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.create_collation("HU", hu_compare)?;
    migrate(&conn)?;
    Ok(conn)
}

/// Nestable transaction based on SAVEPOINTs: outside a transaction it acts
/// like BEGIN, inside one (e.g. an import dry run) it nests. Rolls back on
/// drop unless committed.
pub struct Tx<'a> {
    conn: &'a Connection,
    name: String,
    done: bool,
}

static TX_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn tx(conn: &Connection) -> Result<Tx<'_>> {
    let name = format!("sp{}", TX_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    conn.execute_batch(&format!("SAVEPOINT {name}"))?;
    Ok(Tx { conn, name, done: false })
}

impl Tx<'_> {
    pub fn commit(mut self) -> Result<()> {
        self.conn.execute_batch(&format!("RELEASE {}", self.name))?;
        self.done = true;
        Ok(())
    }
}

impl std::ops::Deref for Tx<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn
    }
}

impl Drop for Tx<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.conn.execute_batch(&format!("ROLLBACK TO {0}; RELEASE {0}", self.name));
        }
    }
}

pub fn user_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    let current = user_version(conn)?;
    for (version, sql) in MIGRATIONS {
        if *version <= current {
            continue;
        }
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        if *version == 1 {
            seed_initial(&tx)?;
        }
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}

/// Built-in plot types (FR-TEM-04) and default settings (FR-BEA-01, QUESTIONS Q3/Q13).
fn seed_initial(conn: &Connection) -> Result<()> {
    let ts = now_ts();
    // code, name, category, lease years, rest period, coffins, urns, w, l, order
    let types: &[(&str, &str, &str, i64, i64, i64, i64, Option<i64>, Option<i64>, i64)] = &[
        ("single", "Egyes sírhely", "grave", 25, 25, 1, 4, Some(100), Some(210), 10),
        ("double", "Kettős sírhely", "grave", 25, 25, 2, 8, Some(190), Some(210), 20),
        ("child", "Gyermeksír", "grave", 25, 25, 1, 2, Some(60), Some(130), 30),
        ("crypt", "Sírbolt (kripta)", "crypt", 60, 25, 4, 8, Some(250), Some(300), 40),
        ("urn_grave", "Urnasírhely", "urn", 10, 0, 0, 4, Some(60), Some(60), 50),
        ("urn_niche", "Urnafülke", "urn", 10, 0, 0, 2, Some(40), Some(40), 60),
        ("urn_crypt", "Urnasírbolt", "urn", 20, 0, 0, 8, Some(100), Some(100), 70),
        ("urn_garden", "Urnakert", "urn", 10, 0, 0, 1, Some(40), Some(40), 80),
        ("scatter", "Szóróparcella", "scatter", 0, 0, 0, 0, None, None, 90),
        ("common", "Közös sír", "common", 0, 0, 0, 0, None, None, 100),
        ("honorary", "Díszsírhely", "grave", 25, 25, 2, 4, Some(190), Some(210), 110),
    ];
    for t in types {
        conn.execute(
            "INSERT INTO plot_types (id, code, name, category, default_lease_years, rest_period_years,
               coffin_capacity, urn_capacity, width_cm, length_cm, builtin, sort_order, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,1,?11,?12,?12)",
            params![new_id(), t.0, t.1, t.2, t.3, t.4, t.5, t.6, t.7, t.8, t.9, ts],
        )?;
    }
    for (k, v) in crate::settings::DEFAULTS {
        conn.execute(
            "INSERT INTO settings (key, cemetery_id, value, updated_at) VALUES (?1, '', ?2, ?3)",
            params![k, v, ts],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_file_requires_key() {
        let dir = std::env::temp_dir().join(format!("lunda-test-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("db.sqlite");
        let key = generate_key();
        {
            let c = open(&path, &key, |_, _| Ok(())).unwrap();
            assert_eq!(user_version(&c).unwrap(), latest_version());
            let n: i64 = c.query_row("SELECT count(*) FROM plot_types", [], |r| r.get(0)).unwrap();
            assert!(n >= 10);
        }
        // the file must not be readable as plain SQLite
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3"));
        // wrong key fails
        let err = open(&path, &generate_key(), |_, _| Ok(())).unwrap_err();
        assert_eq!(err.code, "db_key_invalid");
        // correct key reopens
        let c = open(&path, &key, |_, _| panic!("no migration expected")).unwrap();
        let mode: String = c.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
        drop(c);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn trigram_fts_is_available() {
        let c = open_in_memory().unwrap();
        c.execute("INSERT INTO search_index(kind, ref_id, text) VALUES ('d','1','kovacs janos')", []).unwrap();
        let n: i64 = c
            .query_row("SELECT count(*) FROM search_index WHERE text LIKE '%vacs%'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn hu_collation_in_sql() {
        let c = open_in_memory().unwrap();
        let v: Vec<String> = c
            .prepare("SELECT x FROM (SELECT 'Csonka' x UNION SELECT 'Cukor' UNION SELECT 'Dávid') ORDER BY x COLLATE HU")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(v, vec!["Cukor", "Csonka", "Dávid"]);
    }
}
