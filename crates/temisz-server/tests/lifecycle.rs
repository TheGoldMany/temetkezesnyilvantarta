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

/// Transfers are matched from TEMISZ's bank statement: a certain match pays
/// the proforma and issues the invoice, a doubtful one is only suggested.
#[test]
fn bank_statement_settles_proformas() {
    use temisz_server::billing;
    let store = platform();
    let (tenant, email) = new_customer(&store, "bank");
    let user: String = store.conn.query_row("SELECT user_id FROM platform.accounts WHERE email = ?1", lunda_core::params![email], |r| r.get(0)).unwrap();
    let s = store.session_for(&user).unwrap();
    billing::save_details(
        &store.conn,
        &tenant,
        &json!({ "name": "Bankfalva Község Önkormányzata", "postcode": "9999", "city": "Bankfalva", "address": "Fő tér 1.", "email": email }),
    )
    .unwrap();
    billing::subscribe(&store, &s, &tenant, "yearly", "transfer").unwrap();
    let (proforma, number, gross): (String, String, i64) = store
        .conn
        .query_row(
            "SELECT id, number, gross_huf FROM platform.invoices WHERE tenant_id = ?1 AND kind = 'proforma' AND status = 'open'",
            lunda_core::params![tenant],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    let code = &tenant[..8];
    // unique per run: the statement lines are kept between test runs
    let gift: String = lunda_core::util::new_id().chars().filter(|c| c.is_ascii_alphabetic()).take(6).collect();
    // a line for someone else, and the customer's transfer with the customer code
    let csv = format!(
        "Könyvelés dátuma;Összeg;Partner neve;Közlemény\n2026.11.05;12 000;Valaki Más;ajándék {gift}\n2026.11.06;{gross};BANKFALVA KÖZSÉG ÖNK;TEMISZ {code}\n"
    );
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(csv.as_bytes());
    let r = billing::import_statement(&store.conn, "kivonat.csv", &b64, "admin").unwrap();
    assert_eq!(r["matched"], 1, "{r}");
    let status: String = store.conn.query_row("SELECT status FROM platform.invoices WHERE id = ?1", lunda_core::params![proforma], |r| r.get(0)).unwrap();
    assert_eq!(status, "paid");
    let invoices: i64 = store
        .conn
        .query_row("SELECT count(*) FROM platform.invoices WHERE proforma_id = ?1 AND kind = 'invoice'", lunda_core::params![proforma], |r| r.get(0))
        .unwrap();
    assert_eq!(invoices, 1, "final invoice issued for {number}");
    // the same statement again adds nothing
    let again = billing::import_statement(&store.conn, "kivonat.csv", &b64, "admin").unwrap();
    assert_eq!(again["added"], 0);
    // the unrelated line stays for a decision, without a suggestion
    let lines = billing::bank_lines(&store.conn).unwrap();
    let other = lines.iter().find(|l| l["reference"].as_str().unwrap_or("").ends_with(&gift)).unwrap();
    assert!(other["suggestedId"].is_null());
    billing::resolve_line(&store.conn, other["id"].as_str().unwrap(), None, "admin").unwrap();
    assert!(billing::bank_lines(&store.conn).unwrap().iter().all(|l| !l["reference"].as_str().unwrap_or("").ends_with(&gift)));
}

/// The daily job e-mails holders whose lease expires within half a year,
/// once, and sends the office a summary.
#[test]
fn daily_job_sends_expiry_reminders() {
    let store = platform();
    let (tenant, _) = new_customer(&store, "reminder");
    let sys = Session::system();
    api::dispatch(&store, &sys, "sample.load", &json!({})).unwrap();
    let in_100 = lunda_core::util::fmt_date(lunda_core::util::today() + chrono::Duration::days(100));
    let plot: String = store
        .conn
        .query_row(
            "SELECT l.plot_id FROM leases l JOIN plot_holders h ON h.plot_id = l.plot_id AND h.role = 'primary' AND h.end_date IS NULL
             WHERE l.status = 'active' AND l.voided_at IS NULL LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    store.conn.execute("UPDATE leases SET expiry_date = ?2 WHERE plot_id = ?1 AND status = 'active'", lunda_core::params![plot, in_100]).unwrap();
    let address = format!("jogosult-{}@pelda.hu", &tenant[..8]);
    store
        .conn
        .execute(
            "UPDATE persons SET email = ?2 WHERE id = (SELECT person_id FROM plot_holders WHERE plot_id = ?1 AND role = 'primary' AND end_date IS NULL LIMIT 1)",
            lunda_core::params![plot, address],
        )
        .unwrap();
    // nothing happens until the operator switches the reminders on
    temisz_server::reminders::daily(&store).unwrap();
    let count = |kind: &str| -> i64 {
        store
            .conn
            .query_row("SELECT count(*) FROM platform.emails WHERE tenant_id = ?1 AND kind = ?2", lunda_core::params![tenant, kind], |r| r.get(0))
            .unwrap()
    };
    assert_eq!(count("expiry_reminder"), 0);
    store.conn.execute("INSERT INTO settings (key, cemetery_id, value, updated_at) VALUES ('notify.auto_expiry', '', 'true', 'x') ON CONFLICT (key, cemetery_id) DO UPDATE SET value = 'true'", []).unwrap();
    temisz_server::reminders::daily(&store).unwrap();
    let to: String = store
        .conn
        .query_row("SELECT to_address FROM platform.emails WHERE tenant_id = ?1 AND kind = 'expiry_reminder'", lunda_core::params![tenant], |r| r.get(0))
        .unwrap();
    assert_eq!(to, address);
    assert_eq!(count("reminder_summary"), 1);
    // the next day nothing new goes to the same holder
    temisz_server::reminders::daily(&store).unwrap();
    assert_eq!(count("expiry_reminder"), 1);
}

/// The subscription from the trial to payment, including the loophole of
/// ordering again to reopen an account whose proforma was never paid.
#[test]
fn subscription_lifecycle_and_unpaid_proformas() {
    use temisz_server::billing;
    let store = platform();
    let (tenant, email) = new_customer(&store, "sub");
    let user: String = store.conn.query_row("SELECT user_id FROM platform.accounts WHERE email = ?1", lunda_core::params![email], |r| r.get(0)).unwrap();
    let s = store.session_for(&user).unwrap();
    // billing details are needed before ordering
    assert_eq!(billing::subscribe(&store, &s, &tenant, "monthly", "transfer").unwrap_err().code, "billing_details_missing");
    billing::save_details(
        &store.conn,
        &tenant,
        &json!({ "name": "Előfizető Község Önkormányzata", "postcode": "9999", "city": "Előfizető", "address": "Fő tér 1.", "email": email }),
    )
    .unwrap();
    // ordered during the trial: the trial runs on, the paid period starts at its end
    billing::subscribe(&store, &s, &tenant, "monthly", "transfer").unwrap();
    assert_eq!(billing::effective_status(&store.conn, &tenant).unwrap(), "trial");
    assert_eq!(billing::subscribe(&store, &s, &tenant, "yearly", "transfer").unwrap_err().code, "already_subscribed");
    // the trial ends before the transfer arrives: work goes on
    store.conn.execute("UPDATE platform.tenants SET trial_ends_at = now() - interval '1 hour' WHERE id = ?1", lunda_core::params![tenant]).unwrap();
    assert_eq!(billing::effective_status(&store.conn, &tenant).unwrap(), "active");
    // not paid in time: read-only
    store
        .conn
        .execute("UPDATE platform.invoices SET due_date = CURRENT_DATE - 30 WHERE tenant_id = ?1 AND kind = 'proforma'", lunda_core::params![tenant])
        .unwrap();
    billing::daily(&store, "https://www.temisz.hu/app").unwrap();
    assert_eq!(billing::effective_status(&store.conn, &tenant).unwrap(), "read_only");
    let sub: String =
        store.conn.query_row("SELECT status FROM platform.subscriptions WHERE tenant_id = ?1", lunda_core::params![tenant], |r| r.get(0)).unwrap();
    assert_eq!(sub, "past_due");
    // ordering again does not reopen it
    let err = billing::subscribe(&store, &s, &tenant, "monthly", "transfer").unwrap_err();
    assert_eq!(err.code, "unpaid_proforma");
    assert_eq!(billing::effective_status(&store.conn, &tenant).unwrap(), "read_only");
    // the transfer arrives: final invoice, editable again
    let proforma: String = store
        .conn
        .query_row("SELECT id FROM platform.invoices WHERE tenant_id = ?1 AND kind = 'proforma' AND status = 'open'", lunda_core::params![tenant], |r| r.get(0))
        .unwrap();
    billing::mark_paid(&store.conn, &proforma).unwrap();
    assert_eq!(billing::effective_status(&store.conn, &tenant).unwrap(), "active");
    let (status, start, end): (String, String, String) = store
        .conn
        .query_row(
            "SELECT status, to_char(current_period_start, 'YYYY-MM-DD'), to_char(current_period_end, 'YYYY-MM-DD') FROM platform.subscriptions WHERE tenant_id = ?1",
            lunda_core::params![tenant],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(status, "active");
    assert!(end > start);
    // paying twice is refused
    assert_eq!(billing::mark_paid(&store.conn, &proforma).unwrap_err().code, "invoice_not_open");
}

/// A trial without an order ends read-only; reading and the data export still work.
#[test]
fn trial_without_order_becomes_read_only() {
    use temisz_server::billing;
    let store = platform();
    let (tenant, _) = new_customer(&store, "trialend");
    store.conn.execute("UPDATE platform.tenants SET trial_ends_at = now() - interval '1 hour' WHERE id = ?1", lunda_core::params![tenant]).unwrap();
    assert_eq!(billing::effective_status(&store.conn, &tenant).unwrap(), "read_only");
    assert!(billing::is_read_method("plots.list") && billing::is_read_method("persons.dataExport") && billing::is_read_method("export.file"));
    assert!(!billing::is_read_method("burial.record") && !billing::is_read_method("users.create"));
}

/// A listed platform-admin address gives nothing until it is confirmed and
/// protected by two-factor sign-in.
#[test]
fn superadmin_needs_confirmed_address_and_two_factor() {
    use temisz_server::{accounts, is_superadmin};
    let store = platform();
    let (_, email) = new_customer(&store, "root");
    // only this test reads SUPERADMIN_EMAILS
    std::env::set_var("SUPERADMIN_EMAILS", format!("valaki@pelda.hu, {email}"));
    let acc = accounts::account_by_email(&store.conn, &email).unwrap().unwrap();
    assert!(!is_superadmin(&acc, false), "unconfirmed address");
    store.conn.execute("UPDATE platform.accounts SET email_verified_at = now() WHERE email = ?1", lunda_core::params![email]).unwrap();
    let acc = accounts::account_by_email(&store.conn, &email).unwrap().unwrap();
    assert!(!is_superadmin(&acc, false), "no two-factor sign-in");
    store.conn.execute("UPDATE platform.accounts SET totp_enabled_at = now(), totp_secret = 'X' WHERE email = ?1", lunda_core::params![email]).unwrap();
    let acc = accounts::account_by_email(&store.conn, &email).unwrap().unwrap();
    assert!(is_superadmin(&acc, false));
}

/// Links in e-mails never point to a foreign host.
#[test]
fn mail_links_use_our_address() {
    use temisz_server::accounts::base_url;
    let req = |host: &str| Request { headers: vec![("host".into(), host.into())], ..Default::default() };
    if std::env::var("PUBLIC_URL").is_err() {
        assert_eq!(base_url(&req("evil.example")), "https://www.temisz.hu/app");
        assert_eq!(base_url(&req("www.temisz.hu")), "https://www.temisz.hu");
        assert_eq!(base_url(&req("127.0.0.1:1430")), "http://127.0.0.1:1430");
    }
}

/// Daily backups are encrypted; only the right key opens them.
#[test]
fn backups_are_sealed() {
    use temisz_server::lifecycle::{open_sealed, seal};
    let key = [7u8; 32];
    let sealed = seal(&key, b"{\"persons\":[\"Kiss Anna\"]}");
    assert!(!String::from_utf8_lossy(&sealed).contains("Kiss Anna"));
    assert_eq!(open_sealed(&key, &sealed).unwrap(), b"{\"persons\":[\"Kiss Anna\"]}");
    assert_eq!(open_sealed(&[8u8; 32], &sealed).unwrap_err().code, "backup_unreadable");
}

/// Sign-ins and failures are logged with IP address and browser, per organisation.
#[test]
fn security_log_records_sign_ins() {
    use temisz_server::accounts::{security_event, security_log};
    let store = platform();
    let (tenant, email) = new_customer(&store, "seclog");
    let req = Request { ip: "203.0.113.7".into(), headers: vec![("user-agent".into(), "Mozilla/5.0 Firefox/140.0".into())], ..Default::default() };
    security_event(&store.conn, &req, &email.to_uppercase(), "login_failed");
    security_event(&store.conn, &req, &email, "login");
    security_event(&store.conn, &req, "senki@pelda.hu", "login_failed");
    let log = security_log(&store.conn, &tenant).unwrap();
    let rows = log.as_array().unwrap();
    assert_eq!(rows.len(), 2, "{log}");
    assert_eq!(rows[0]["kind"], "login");
    assert_eq!(rows[1]["kind"], "login_failed");
    assert_eq!(rows[0]["ip"], "203.0.113.7");
    assert!(rows[0]["userAgent"].as_str().unwrap().contains("Firefox"));
}
