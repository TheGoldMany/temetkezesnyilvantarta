//! Tenant database: connecting, schema selection, migrations, seed data.
//!
//! Every customer (tenant) has its own Postgres schema with the same tables.
//! The connection's `search_path` points at it, so the service layer's SQL
//! never names a schema and cannot reach another customer's data.

use crate::error::{CoreError, Result};
use crate::sql::{params, Connection};
use crate::util::{new_id, now_ts};

pub use crate::sql::{tx, Tx};

/// Ordered list of tenant-schema migrations. Never edit a released migration – add a new one.
pub const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/pg/0001_initial.sql")), (2, include_str!("../migrations/pg/0002_finance.sql"))];

/// Hungarian collation for ORDER BY (ICU).
pub const HU: &str = "\"hu-x-icu\"";

pub fn latest_version() -> i64 {
    MIGRATIONS.last().map(|m| m.0).unwrap_or(0)
}

/// Generates a random 256-bit secret as hex.
pub fn generate_key() -> String {
    use rand::RngCore;
    let mut k = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut k);
    hex::encode(k)
}

/// Default database for local development and tests.
/// Neon's Vercel integration sets both; the direct (unpooled) connection is
/// needed because each request switches the search_path to a tenant schema.
pub fn database_url() -> String {
    std::env::var("DATABASE_URL_UNPOOLED").or_else(|_| std::env::var("DATABASE_URL")).unwrap_or_else(|_| "postgres://postgres@127.0.0.1:5433/postgres".into())
}

/// Valid tenant schema name: `t_` + lowercase hex/underscore.
pub fn tenant_schema(tenant_id: &str) -> Result<String> {
    let clean: String = tenant_id.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
    if clean.is_empty() || clean.len() > 40 {
        return Err(CoreError::new("tenant_invalid", "invalid tenant id"));
    }
    Ok(format!("t_{clean}"))
}

/// Points the connection at a tenant schema, creating and migrating it when needed.
pub fn open_tenant(conn: &Connection, schema: &str) -> Result<()> {
    conn.execute_batch("CREATE EXTENSION IF NOT EXISTS pg_trgm")?;
    conn.execute_batch(&format!("CREATE SCHEMA IF NOT EXISTS \"{schema}\""))?;
    conn.use_schema(schema)?;
    migrate(conn)
}

/// Fresh throw-away tenant schema for tests (dropped with the connection).
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::connect(&database_url())?;
    let schema = format!("test_{}", new_id().replace('-', ""));
    open_tenant(&conn, &schema)?;
    conn.set_temp_schema(true);
    Ok(conn)
}

/// Refreshes planner statistics after a bulk load (import, sample data), so
/// the next queries are not planned as if the tables were still empty.
pub fn analyze(conn: &Connection) -> Result<()> {
    if conn.in_transaction() {
        return Ok(());
    }
    conn.execute_batch("ANALYZE plots, plot_rows, sections, interment_events, deceased, persons, plot_holders, leases, notices, search_index")?;
    Ok(())
}

pub fn user_version(conn: &Connection) -> Result<i64> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version BIGINT NOT NULL)")?;
    Ok(conn.query_row("SELECT coalesce(max(version), 0) FROM schema_version", [], |r| r.get(0))?)
}

pub fn migrate(conn: &Connection) -> Result<()> {
    let current = user_version(conn)?;
    if current > latest_version() {
        return Err(CoreError::new("db_too_new", "database was created by a newer version").with("version", current));
    }
    if current == latest_version() {
        return Ok(());
    }
    // two requests may open a new tenant at once: serialise per schema
    let t = tx(conn)?;
    t.execute_batch("SELECT pg_advisory_xact_lock(7381002, hashtext(current_schema()))")?;
    let current = user_version(&t)?;
    for (version, sql) in MIGRATIONS {
        if *version <= current {
            continue;
        }
        t.execute_batch(sql)?;
        if *version == 1 {
            seed_initial(&t)?;
        }
        t.execute("INSERT INTO schema_version (version) VALUES (?1)", params![version])?;
    }
    t.commit()?;
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
        conn.execute("INSERT INTO settings (key, cemetery_id, value, updated_at) VALUES (?1, '', ?2, ?3)", params![k, v, ts])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_and_seeds() {
        let c = open_in_memory().unwrap();
        assert_eq!(user_version(&c).unwrap(), latest_version());
        let n: i64 = c.query_row("SELECT count(*) FROM plot_types", [], |r| r.get(0)).unwrap();
        assert!(n >= 10);
    }

    #[test]
    fn trigram_search_is_available() {
        let c = open_in_memory().unwrap();
        c.execute("INSERT INTO search_index(kind, ref_id, text) VALUES ('d','1','kovacs janos')", []).unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM search_index WHERE text LIKE '%vacs%'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn hu_collation_in_sql() {
        let c = open_in_memory().unwrap();
        let v: Vec<String> = c
            .prepare(&format!("SELECT x FROM (VALUES ('Csonka'), ('Cukor'), ('Dávid')) t(x) ORDER BY x COLLATE {HU}"))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(v, vec!["Cukor", "Csonka", "Dávid"]);
    }

    #[test]
    fn delete_is_forbidden() {
        let c = open_in_memory().unwrap();
        c.execute("INSERT INTO cemeteries (id, name, created_at, updated_at) VALUES ('c1', 'Teszt', 'now', 'now')", []).unwrap();
        let e = c.execute("DELETE FROM cemeteries", []).unwrap_err();
        assert!(e.to_string().contains("physical delete forbidden"), "{e}");
    }

    #[test]
    fn cached_statements_follow_the_schema_switch() {
        // One connection serving two tenants: a statement prepared for A must
        // read B's table after switching (Postgres re-parses on search_path change).
        let c = Connection::connect(&database_url()).unwrap();
        let a = format!("test_{}", new_id().replace('-', ""));
        let b = format!("test_{}", new_id().replace('-', ""));
        open_tenant(&c, &a).unwrap();
        c.execute("INSERT INTO settings (key, cemetery_id, value, updated_at) VALUES ('who', '', 'A', 'now')", []).unwrap();
        let q = "SELECT value FROM settings WHERE key = 'who'";
        let v: String = c.query_row(q, [], |r| r.get(0)).unwrap();
        assert_eq!(v, "A");
        open_tenant(&c, &b).unwrap();
        c.execute("INSERT INTO settings (key, cemetery_id, value, updated_at) VALUES ('who', '', 'B', 'now')", []).unwrap();
        let v: String = c.query_row(q, [], |r| r.get(0)).unwrap();
        assert_eq!(v, "B");
        c.use_schema(&a).unwrap();
        let v: String = c.query_row(q, [], |r| r.get(0)).unwrap();
        assert_eq!(v, "A");
        c.execute_batch(&format!("DROP SCHEMA {a} CASCADE; DROP SCHEMA {b} CASCADE")).unwrap();
    }

    #[test]
    fn tenants_are_isolated() {
        let a = open_in_memory().unwrap();
        let b = open_in_memory().unwrap();
        a.execute("INSERT INTO settings (key, cemetery_id, value, updated_at) VALUES ('x.only_a', '', 'a', 'now')", []).unwrap();
        let n: i64 = b.query_row("SELECT count(*) FROM settings WHERE key = 'x.only_a'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }
}
