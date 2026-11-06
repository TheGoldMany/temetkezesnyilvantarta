//! W4: export/restore of a tenant and loading a desktop backup.

use lunda_core::audit::{compute_hash, GENESIS};
use lunda_core::auth::Session;
use lunda_core::sql::Connection;
use lunda_core::{api, db, Store};
use serde_json::json;
use temisz_server::{migrate_platform, register, tenant_data, Request};

fn platform() -> Store {
    let store = Store::new(Connection::connect(&db::database_url()).unwrap());
    migrate_platform(&store.conn).unwrap();
    store
}

fn new_customer(store: &Store, tag: &str) -> (String, String) {
    let email = format!("{tag}-{}@pelda.hu", lunda_core::util::new_id());
    register(store, "Teszt Önkormányzat", "Teszt Admin", &email, "nagyon-titkos-1", &Request::default()).unwrap();
    let tenant: String = store.conn.query_row("SELECT tenant_id FROM platform.accounts WHERE email = ?1", lunda_core::params![email], |r| r.get(0)).unwrap();
    (tenant, email)
}

#[test]
fn export_covers_every_tenant_table() {
    let store = platform();
    let _ = new_customer(&store, "tables");
    let mut st = store
        .conn
        .prepare("SELECT tablename::text FROM pg_tables WHERE schemaname = current_schema() AND tablename NOT IN ('schema_migrations', 'schema_version')")
        .unwrap();
    let all: Vec<String> = st.query_map([], |r| r.get(0)).unwrap().map(|r| r.unwrap()).collect();
    let missing: Vec<&String> = all.iter().filter(|t| !tenant_data::TABLES.contains(&t.as_str())).collect();
    assert!(missing.is_empty(), "tables missing from the export: {missing:?}");
}

#[test]
fn export_and_load_round_trip() {
    let store = platform();
    let (tenant, _) = new_customer(&store, "export");
    let sys = Session::system();
    api::dispatch(&store, &sys, "sample.load", &json!({})).unwrap();
    let data = tenant_data::export(&store.conn).unwrap();
    let tables: std::collections::BTreeMap<String, serde_json::Value> = serde_json::from_value(data["tables"].clone()).unwrap();
    let plots = tables["plots"].as_array().unwrap().len();
    assert!(plots > 100);
    let schema = tenant_data::load_into_new_schema(&store.conn, &tenant, &tables).unwrap();
    let n: i64 = store.conn.query_row("SELECT count(*) FROM plots", [], |r| r.get(0)).unwrap();
    assert_eq!(n as usize, plots);
    assert!(lunda_core::audit::verify_chain(&store.conn).unwrap().ok, "audit chain survives the copy");
    // the copy is a working register: search and folio allocation continue
    let hits = store.quick_search(&sys, "kovacs", 5).unwrap();
    assert!(!hits.is_empty());
    store.conn.execute_batch(&format!("DROP SCHEMA \"{schema}\" CASCADE")).unwrap();
}

/// A desktop backup built the way the desktop app stored them: schema v1,
/// SQLCipher with the recovery key as raw key, hash-chained audit log.
fn desktop_backup(path: &std::path::Path, key: &str) {
    let c = rusqlite::Connection::open(path).unwrap();
    c.execute_batch(&format!("PRAGMA key = \"x'{key}'\";")).unwrap();
    c.create_collation("HU", lunda_core::text::hu_compare).unwrap();
    c.execute_batch(include_str!("../../lunda-core/migrations/0001_initial.sql")).unwrap();
    c.execute_batch(
        "PRAGMA user_version = 1;
         INSERT INTO users VALUES ('u1','admin','Régi Admin','admin','[]','x',1,0,NULL,0,NULL,'t',NULL,'t',NULL);
         INSERT INTO cemeteries (id,name,created_at,updated_at) VALUES ('c1','Ótemető','t','t');
         INSERT INTO plot_types (id,code,name,category,default_lease_years,rest_period_years,coffin_capacity,urn_capacity,builtin,created_at,updated_at)
             VALUES ('pt1','single','Egyes sírhely','grave',25,25,1,4,1,'t','t');
         INSERT INTO sections (id,cemetery_id,code,created_at,updated_at) VALUES ('s1','c1','A','t','t');
         INSERT INTO plot_rows (id,section_id,number,created_at,updated_at) VALUES ('r1','s1','1','t','t');
         INSERT INTO plots (id,cemetery_id,section_id,row_id,number,identifier,plot_type_id,coffin_capacity,urn_capacity,created_at,updated_at)
             VALUES ('p1','c1','s1','r1','1','A/1/1','pt1',1,4,'t','t');
         INSERT INTO deceased (id,family_name,given_name,death_date,search_text,sort_key,created_at,updated_at)
             VALUES ('d1','Kovács','Ilona','1975-03-01','kovacs ilona','kovacs ilona','t','t');
         INSERT INTO register_sequences VALUES ('c1','all',2);
         INSERT INTO interment_events (id,cemetery_id,scope_key,folio,event_type,container,event_date,deceased_id,plot_id,created_at,updated_at)
             VALUES ('e1','c1','all',1,'coffin_burial','coffin','1975-03-05','d1','p1','t','t');
         INSERT INTO search_index (kind, ref_id, text) VALUES ('deceased','d1','kovacs ilona');",
    )
    .unwrap();
    let mut prev = GENESIS.to_string();
    for (i, action) in ["create", "update"].iter().enumerate() {
        let ts = format!("2020-01-0{}T00:00:00Z", i + 1);
        let h = compute_hash(&prev, &ts, Some("u1"), "admin", "plot", "p1", action, None, Some("{}"), None);
        c.execute(
            "INSERT INTO audit_log (ts,user_id,user_name,entity,entity_id,action,old_value,new_value,reason,prev_hash,hash) VALUES (?1,'u1','admin','plot','p1',?2,NULL,'{}',NULL,?3,?4)",
            rusqlite::params![ts, action, prev, h],
        )
        .unwrap();
        prev = h;
    }
}

#[test]
fn desktop_backup_is_loaded_into_an_empty_account() {
    let store = platform();
    let (tenant, email) = new_customer(&store, "desktop");
    let key = db::generate_key();
    let dir = std::env::temp_dir().join(format!("temisz-desk-{}", lunda_core::util::new_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("regi.ltbak");
    desktop_backup(&path, &key);

    // a wrong key is refused with a clear code
    let err = temisz_desktop_import::read_backup(&path, &db::generate_key()).unwrap_err();
    assert_eq!(err.code, "wrong_recovery_key");

    let tables = temisz_desktop_import::read_backup(&path, &key.to_uppercase()).unwrap();
    let account_user: String =
        store.conn.query_row("SELECT user_id FROM platform.accounts WHERE email = ?1", lunda_core::params![email], |r| r.get(0)).unwrap();
    let s = store.session_for(&account_user).unwrap();
    let rep = tenant_data::import_desktop(&store, &s, &tenant, tables).unwrap();
    assert_eq!(rep["counts"]["plots"], 1);
    assert_eq!(rep["auditChain"]["ok"], true, "{rep}");

    // the register is the desktop one, the web admin can still work with it
    let ev = store.list_events(&Default::default()).unwrap();
    assert_eq!(ev.items[0].folio, 1);
    assert_eq!(ev.items[0].deceased_name, "Kovács Ilona");
    let users = store.list_users(&s).unwrap();
    assert!(users.iter().any(|u| u.username == email && u.active));
    assert!(users.iter().any(|u| u.username == "admin" && !u.active));
    // a second import is refused: the account is no longer empty
    let again = temisz_desktop_import::read_backup(&path, &key).unwrap();
    assert_eq!(tenant_data::import_desktop(&store, &s, &tenant, again).unwrap_err().code, "import_not_empty");
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn rate_limit_blocks_after_the_limit() {
    let store = platform();
    let key = format!("test:{}", lunda_core::util::new_id());
    temisz_server::accounts::rate_limit(&store.conn, &key, 2, 15).unwrap();
    temisz_server::accounts::rate_limit(&store.conn, &key, 2, 15).unwrap();
    assert_eq!(temisz_server::accounts::rate_limit(&store.conn, &key, 2, 15).unwrap_err().code, "rate_limited");
}
