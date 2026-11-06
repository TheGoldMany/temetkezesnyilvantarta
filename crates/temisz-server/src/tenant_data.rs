//! Whole-tenant data as JSON: daily exports, imports of desktop backups.
//!
//! Rows go in and out through Postgres' own JSON functions
//! (`row_to_json`, `json_populate_recordset`), so every column type is
//! handled the same way in both directions.

use std::collections::BTreeMap;

use lunda_core::auth::Session;
use lunda_core::sql::{params, Connection};
use lunda_core::{db, CoreError, Result, Store};
use serde_json::{json, Value};

/// Tenant tables in foreign-key order (self references are checked at the
/// end of each INSERT statement, so one statement per table is enough).
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
    // finance (W7)
    "charges",
    "invoices",
    "invoice_lines",
    "payments",
    "cash_sequences",
    "cash_entries",
    "cash_closings",
    "bank_transactions",
    "reminders",
    // operations (W8–W11)
    "facilities",
    "contractors",
    "bookings",
    "work_notices",
    "work_orders",
    "case_sequences",
    "cases",
    "plot_conditions",
    "notices",
    "monuments",
    "external_placements",
    "attachments",
    "audit_log",
    "print_log",
    "backup_log",
    "search_index",
];

/// Every table of the connection's current tenant schema.
pub fn export(conn: &Connection) -> Result<Value> {
    let mut out = serde_json::Map::new();
    out.insert("format".into(), json!("temisz-tenant-v1"));
    out.insert("schemaVersion".into(), json!(db::user_version(conn)?));
    out.insert("exportedAt".into(), json!(lunda_core::util::now_ts()));
    let mut tables = serde_json::Map::new();
    for t in TABLES {
        let text: String = conn.query_row(&format!("SELECT coalesce(json_agg(row_to_json(x)), '[]')::text FROM {t} x"), [], |r| r.get(0))?;
        tables.insert(t.to_string(), serde_json::from_str(&text)?);
    }
    out.insert("tables".into(), Value::Object(tables));
    Ok(Value::Object(out))
}

/// Loads tables into a fresh tenant schema and returns its name. The seeded
/// defaults are replaced by the imported rows.
pub fn load_into_new_schema(conn: &Connection, tenant_id: &str, tables: &BTreeMap<String, Value>) -> Result<String> {
    let schema = format!("{}_{}", db::tenant_schema(tenant_id)?, &lunda_core::util::new_id()[..8]);
    db::open_tenant(conn, &schema)?;
    // seeded rows that the import brings its own version of
    conn.execute("DELETE FROM settings", [])?;
    conn.execute("DELETE FROM plot_types", [])?;
    for t in TABLES {
        let Some(rows) = tables.get(*t) else { continue };
        if rows.as_array().map(|a| a.is_empty()).unwrap_or(true) {
            continue;
        }
        // only the columns the data carries: an older export lacks newer
        // columns, which then take their defaults
        let mut have = std::collections::BTreeSet::new();
        for r in rows.as_array().into_iter().flatten() {
            if let Some(o) = r.as_object() {
                have.extend(o.keys().cloned());
            }
        }
        let mut st = conn.prepare(
            "SELECT attname::text FROM pg_attribute WHERE attrelid = CAST(CAST(?1 AS text) AS regclass) AND attnum > 0 AND NOT attisdropped ORDER BY attnum",
        )?;
        let cols: Vec<String> = st.query_map(params![t], |r| r.get(0))?.collect::<std::result::Result<Vec<String>, _>>()?;
        let cols: Vec<String> = cols.into_iter().filter(|c| have.contains(c)).map(|c| format!("\"{c}\"")).collect();
        let list = cols.join(", ");
        conn.execute(&format!("INSERT INTO {t} ({list}) SELECT {list} FROM json_populate_recordset(NULL::{t}, CAST(?1 AS json))"), params![rows.to_string()])?;
    }
    conn.execute_batch("SELECT setval(pg_get_serial_sequence('audit_log', 'seq'), coalesce((SELECT max(seq) FROM audit_log), 0) + 1, false)")?;
    db::analyze(conn)?;
    Ok(schema)
}

/// Replaces an empty tenant's register with a desktop backup. The current web
/// users are kept; the desktop users come along deactivated (their names stay
/// in the change log). The tenant then points at the new schema.
pub fn import_desktop(store: &Store, s: &Session, tenant_id: &str, mut tables: BTreeMap<String, Value>) -> Result<Value> {
    let conn = &store.conn;
    let cemeteries: i64 = conn.query_row("SELECT count(*) FROM cemeteries WHERE voided_at IS NULL", [], |r| r.get(0))?;
    if cemeteries > 0 {
        return Err(CoreError::new("import_not_empty", "the desktop backup can only be loaded into an empty account"));
    }
    let old_schema = conn.schema().ok_or_else(|| CoreError::new("tenant_invalid", "no tenant schema"))?;
    // desktop users: deactivated, never able to sign in
    if let Some(Value::Array(users)) = tables.get_mut("users") {
        for u in users.iter_mut() {
            u["active"] = json!(0);
        }
    }
    let counts: Value = tables.iter().map(|(k, v)| (k.clone(), json!(v.as_array().map(|a| a.len()).unwrap_or(0)))).collect::<serde_json::Map<_, _>>().into();
    let t = db::tx(conn)?;
    let new_schema = load_into_new_schema(&t, tenant_id, &tables)?;
    // keep the web users (their ids are referenced by the sign-in accounts)
    t.execute_batch(&format!("INSERT INTO \"{new_schema}\".users SELECT * FROM \"{old_schema}\".users ON CONFLICT DO NOTHING"))?;
    t.use_schema(&new_schema)?;
    lunda_core::audit::record_simple(&t, s, "backup", "desktop", "restore", Some(&format!("Asztali mentés betöltve: {counts}")))?;
    t.execute("UPDATE platform.tenants SET schema_name = ?2 WHERE id = ?1", params![tenant_id, new_schema])?;
    t.execute_batch(&format!("DROP SCHEMA \"{old_schema}\" CASCADE"))?;
    t.commit()?;
    store.conn.use_schema(&new_schema)?;
    Ok(json!({ "counts": counts, "auditChain": lunda_core::audit::verify_chain(&store.conn)? }))
}
