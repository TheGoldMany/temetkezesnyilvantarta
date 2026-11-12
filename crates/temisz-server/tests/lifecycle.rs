//! W4: export/restore of a tenant and loading a desktop backup.

use lunda_core::audit::{compute_hash, GENESIS};
use lunda_core::auth::Session;
use lunda_core::sql::Connection;
use lunda_core::{api, db, Store};
use serde_json::json;
use temisz_server::{migrate_platform, register, tenant_data, Request};

/// The Barion tests point BARION_API_URL at their own stand-in server: one at a time.
static BARION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

/// A cemetery of the customer (without a subscription yet) and the admin's session.
fn cemetery_of(store: &Store, email: &str, name: &str) -> (String, Session) {
    let (user, schema): (String, String) = store
        .conn
        .query_row(
            "SELECT a.user_id, t.schema_name FROM platform.accounts a JOIN platform.tenants t ON t.id = a.tenant_id WHERE a.email = ?1",
            lunda_core::params![email],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    store.conn.use_schema(&schema).unwrap();
    let s = store.session_for(&user).unwrap();
    let c = api::dispatch(store, &s, "cemeteries.save", &json!({ "cemetery": { "name": name } })).unwrap();
    (c["id"].as_str().unwrap().to_string(), s)
}

fn details(store: &Store, tenant: &str, email: &str) {
    temisz_server::billing::save_details(
        &store.conn,
        tenant,
        &json!({ "name": "Teszt Község Önkormányzata", "postcode": "9999", "city": "Tesztfalva", "address": "Fő tér 1.", "email": email }),
    )
    .unwrap();
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
    let (cem, s) = cemetery_of(&store, &email, "Bankfalvi temető");
    billing::save_details(
        &store.conn,
        &tenant,
        &json!({ "name": "Bankfalva Község Önkormányzata", "postcode": "9999", "city": "Bankfalva", "address": "Fő tér 1.", "email": email }),
    )
    .unwrap();
    temisz_server::subscriptions::start_transfer(&store, &s, &tenant, &cem, "yearly").unwrap();
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

fn use_tenant(store: &Store, tenant: &str) {
    let schema: String = store.conn.query_row("SELECT schema_name FROM platform.tenants WHERE id = ?1", lunda_core::params![tenant], |r| r.get(0)).unwrap();
    store.conn.use_schema(&schema).unwrap();
}

/// Transfer: no trial, a proforma at once, the cemetery works while it is
/// open but nothing can be exported until it is paid; unpaid after the grace
/// period it is locked, and ordering again does not reopen it. An order
/// cancelled without paying ends at once, and the next order works only
/// after payment.
#[test]
fn transfer_subscription_and_unpaid_proformas() {
    // the daily job works on every customer: one test at a time
    let _serial = BARION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    use temisz_server::{billing, subscriptions};
    let store = platform();
    let (tenant, email) = new_customer(&store, "sub");
    let (cem, s) = cemetery_of(&store, &email, "Előfizetői temető");
    let daily = || {
        subscriptions::daily(&store, "https://www.temisz.hu/app").unwrap();
        use_tenant(&store, &tenant);
    };
    let open_proforma = |cem: &str| -> String {
        store
            .conn
            .query_row(
                "SELECT id FROM platform.invoices WHERE tenant_id = ?1 AND cemetery_id = ?2 AND kind = 'proforma' AND status = 'open'",
                lunda_core::params![tenant, cem],
                |r| r.get(0),
            )
            .unwrap()
    };
    // without a subscription the cemetery can only be read
    assert_eq!(subscriptions::locked_cemeteries(&store, &tenant).unwrap(), vec![cem.clone()]);
    assert_eq!(subscriptions::start_transfer(&store, &s, &tenant, &cem, "monthly").unwrap_err().code, "billing_details_missing");
    details(&store, &tenant, &email);
    subscriptions::start_transfer(&store, &s, &tenant, &cem, "monthly").unwrap();
    let l = subscriptions::limits(&store, &tenant).unwrap();
    assert!(l.locked.is_empty(), "works while the proforma is open");
    assert_eq!(l.unpaid, vec![cem.clone()], "no export before the first payment");
    assert!(l.marked.is_empty() && !l.any_paid, "an order is not a trial: prints are not marked");
    assert_eq!(subscriptions::start_transfer(&store, &s, &tenant, &cem, "yearly").unwrap_err().code, "already_subscribed");
    // not paid in time: locked
    store
        .conn
        .execute("UPDATE platform.invoices SET due_date = CURRENT_DATE - 30 WHERE tenant_id = ?1 AND kind = 'proforma'", lunda_core::params![tenant])
        .unwrap();
    daily();
    assert_eq!(subscriptions::locked_cemeteries(&store, &tenant).unwrap(), vec![cem.clone()]);
    // ordering again does not reopen it
    assert_eq!(subscriptions::start_transfer(&store, &s, &tenant, &cem, "monthly").unwrap_err().code, "unpaid_proforma");
    // the transfer arrives: final invoice, writable again, exports allowed
    let proforma = open_proforma(&cem);
    billing::mark_paid(&store.conn, &proforma).unwrap();
    let l = subscriptions::limits(&store, &tenant).unwrap();
    assert!(l.locked.is_empty() && l.unpaid.is_empty() && l.any_paid);
    let sub = subscriptions::get(&store.conn, &tenant, &cem).unwrap().unwrap();
    assert_eq!(sub.status, "active");
    assert!(sub.paid && sub.period_end > sub.period_start);
    // paying twice is refused
    assert_eq!(billing::mark_paid(&store.conn, &proforma).unwrap_err().code, "invoice_not_open");
    // cancelled: works until the paid period ends, and the cancellation can be undone
    subscriptions::cancel(&store, &s, &tenant, &cem, false).unwrap();
    assert!(subscriptions::locked_cemeteries(&store, &tenant).unwrap().is_empty());
    subscriptions::cancel(&store, &s, &tenant, &cem, true).unwrap();
    assert_eq!(subscriptions::get(&store.conn, &tenant, &cem).unwrap().unwrap().status, "active");

    // a second cemetery: ordered, used, cancelled without paying
    let c2 = api::dispatch(&store, &s, "cemeteries.save", &json!({ "cemetery": { "name": "Második temető" } })).unwrap()["id"].as_str().unwrap().to_string();
    assert_eq!(subscriptions::start_transfer(&store, &s, &tenant, &c2, "monthly").unwrap()["prepay"], false);
    assert!(subscriptions::locked_cemeteries(&store, &tenant).unwrap().is_empty());
    subscriptions::cancel(&store, &s, &tenant, &c2, false).unwrap();
    assert_eq!(subscriptions::locked_cemeteries(&store, &tenant).unwrap(), vec![c2.clone()], "an unpaid order ends at once");
    // the next order is not another free period: it works only once it is paid
    assert_eq!(subscriptions::start_transfer(&store, &s, &tenant, &c2, "monthly").unwrap()["prepay"], true);
    assert_eq!(subscriptions::locked_cemeteries(&store, &tenant).unwrap(), vec![c2.clone()]);
    daily();
    assert_eq!(subscriptions::locked_cemeteries(&store, &tenant).unwrap(), vec![c2.clone()]);
    billing::mark_paid(&store.conn, &open_proforma(&c2)).unwrap();
    assert!(subscriptions::locked_cemeteries(&store, &tenant).unwrap().is_empty());
    // an archived cemetery's subscription ends, its open proforma is void
    let c3 = api::dispatch(&store, &s, "cemeteries.save", &json!({ "cemetery": { "name": "Harmadik temető" } })).unwrap()["id"].as_str().unwrap().to_string();
    subscriptions::start_transfer(&store, &s, &tenant, &c3, "monthly").unwrap();
    api::dispatch(&store, &s, "records.void", &json!({ "entity": "cemetery", "id": c3, "reason": "tévesen felvéve" })).unwrap();
    subscriptions::cemetery_archived(&store.conn, &tenant, &c3).unwrap();
    assert_eq!(subscriptions::get(&store.conn, &tenant, &c3).unwrap().unwrap().status, "lapsed");
    let open: i64 = store
        .conn
        .query_row("SELECT count(*) FROM platform.invoices WHERE cemetery_id = ?1 AND status = 'open'", lunda_core::params![c3], |r| r.get(0))
        .unwrap();
    assert_eq!(open, 0);
}

/// The sample cemetery is a free demonstration for 30 days: writable, prints
/// marked, no export; then read only.
#[test]
fn sample_cemetery_is_a_demo() {
    let _serial = BARION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    use temisz_server::subscriptions;
    let store = platform();
    let (tenant, email) = new_customer(&store, "demo");
    let (cem, _) = cemetery_of(&store, &email, "Minta temető");
    subscriptions::register_demo(&store.conn, &tenant, &cem, "Minta temető").unwrap();
    let l = subscriptions::limits(&store, &tenant).unwrap();
    assert!(l.locked.is_empty());
    assert_eq!((l.unpaid, l.marked), (vec![cem.clone()], vec![cem.clone()]));
    store
        .conn
        .execute("UPDATE platform.cemetery_subscriptions SET trial_ends_at = now() - interval '1 hour' WHERE cemetery_id = ?1", lunda_core::params![cem])
        .unwrap();
    assert_eq!(subscriptions::locked_cemeteries(&store, &tenant).unwrap(), vec![cem.clone()]);
    subscriptions::daily(&store, "https://www.temisz.hu/app").unwrap();
    assert_eq!(subscriptions::get(&store.conn, &tenant, &cem).unwrap().unwrap().status, "lapsed");
    assert!(temisz_server::billing::is_read_method("plots.list") && temisz_server::billing::is_read_method("export.file"));
    assert!(!temisz_server::billing::is_read_method("burial.record") && !temisz_server::billing::is_read_method("users.create"));
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

/// Card payment through Barion against a stand-in Barion server: the
/// callback alone settles nothing; Barion's own answer does, exactly once.
#[test]
fn card_payment_settles_the_proforma_once() {
    let _serial = BARION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    use std::sync::{Arc, Mutex};
    use temisz_server::{barion, billing};
    // stand-in Barion: Payment/Start answers a payment; paymentstate answers the current state
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let state = Arc::new(Mutex::new(("Prepared".to_string(), 0i64)));
    let seen_key = Arc::new(Mutex::new(String::new()));
    // unique per run: payments are kept between local test runs
    let pid: String = lunda_core::util::new_id().replace('-', "");
    let pid_mock = pid.clone();
    {
        let state = state.clone();
        let seen_key = seen_key.clone();
        std::thread::spawn(move || {
            let pid = pid_mock;
            for mut rq in server.incoming_requests() {
                let mut body = String::new();
                let _ = rq.as_reader().read_to_string(&mut body);
                if let Some(h) = rq.headers().iter().find(|h| h.field.equiv("x-pos-key")) {
                    *seen_key.lock().unwrap() = h.value.to_string();
                }
                let (status, total) = state.lock().unwrap().clone();
                let answer = if rq.url().starts_with("/v2/Payment/Start") {
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
                    state.lock().unwrap().1 = v["Transactions"][0]["Total"].as_i64().unwrap();
                    json!({ "PaymentId": pid, "Status": "Prepared", "GatewayUrl": "https://secure.test.barion.com/Pay?id=x", "Errors": [] })
                } else {
                    json!({ "PaymentId": pid, "Status": status, "Total": total, "Currency": "HUF", "Errors": [] })
                };
                let _ = rq.respond(tiny_http::Response::from_string(answer.to_string()));
            }
        });
    }
    std::env::set_var("BARION_POS_KEY", "11111111-2222-3333-4444-555555555555");
    std::env::set_var("BARION_PAYEE", "penz@temisz.hu");
    std::env::set_var("BARION_API_URL", format!("http://127.0.0.1:{port}"));

    let store = platform();
    let (tenant, email) = new_customer(&store, "card");
    let (cem, s) = cemetery_of(&store, &email, "Kártyás temető");
    billing::save_details(
        &store.conn,
        &tenant,
        &json!({ "name": "Kártyás Község", "postcode": "9999", "city": "Kártyás", "address": "Fő tér 1.", "email": email }),
    )
    .unwrap();
    temisz_server::subscriptions::start_transfer(&store, &s, &tenant, &cem, "monthly").unwrap();
    let (proforma, gross): (String, i64) = store
        .conn
        .query_row(
            "SELECT id, gross_huf FROM platform.invoices WHERE tenant_id = ?1 AND kind = 'proforma' AND status = 'open'",
            lunda_core::params![tenant],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    // another customer cannot pay (or see) this proforma
    let (other_tenant, _) = new_customer(&store, "card-other");
    assert_eq!(barion::start(&store.conn, &s, &other_tenant, &proforma, "https://x/app", "https://x").unwrap_err().code, "not_found");

    let started = barion::start(&store.conn, &s, &tenant, &proforma, "https://www.temisz.hu/app", "https://www.temisz.hu").unwrap();
    assert!(started["gatewayUrl"].as_str().unwrap().starts_with("https://"));
    assert_eq!(*seen_key.lock().unwrap(), "11111111-2222-3333-4444-555555555555");
    assert_eq!(state.lock().unwrap().1, gross, "the full proforma amount is charged");
    let payment = started["paymentId"].as_str().unwrap().to_string();
    let paid = |store: &lunda_core::Store| -> String {
        store.conn.query_row("SELECT status FROM platform.invoices WHERE id = ?1", lunda_core::params![proforma], |r| r.get(0)).unwrap()
    };

    // a callback while Barion still says "in progress" settles nothing
    state.lock().unwrap().0 = "Started".into();
    assert_eq!(barion::refresh(&store, &payment).unwrap(), "Started");
    assert_eq!(paid(&store), "open");
    // Barion reports less than the proforma: not settled
    *state.lock().unwrap() = ("Succeeded".into(), gross - 1);
    barion::refresh(&store, &payment).unwrap();
    assert_eq!(paid(&store), "open");
    store.conn.execute("UPDATE platform.card_payments SET status = 'Started' WHERE payment_id = ?1", lunda_core::params![payment]).unwrap();
    // succeeded in full: final invoice paid by card, the account stays open
    *state.lock().unwrap() = ("Succeeded".into(), gross);
    assert_eq!(barion::refresh(&store, &payment).unwrap(), "Succeeded");
    assert_eq!(paid(&store), "paid");
    let invoices: i64 = store
        .conn
        .query_row(
            "SELECT count(*) FROM platform.invoices WHERE proforma_id = ?1 AND kind = 'invoice' AND payment_method = 'card'",
            lunda_core::params![proforma],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(invoices, 1);
    // repeated callbacks change nothing
    barion::refresh(&store, &payment).unwrap();
    barion::refresh(&store, &payment).unwrap();
    let again: i64 =
        store.conn.query_row("SELECT count(*) FROM platform.invoices WHERE proforma_id = ?1", lunda_core::params![proforma], |r| r.get(0)).unwrap();
    assert_eq!(again, 1);
    // a forged payment id is ignored
    assert_eq!(barion::refresh(&store, "deadbeef").unwrap_err().code, "not_found");
    assert_eq!(billing::effective_status(&store.conn, &tenant).unwrap(), "active");
    // the callback carries the id in the query or in the body
    assert_eq!(barion::payment_id_from("paymentId=abc", b"").as_deref(), Some("abc"));
    assert_eq!(barion::payment_id_from("", b"PaymentId=def").as_deref(), Some("def"));
}

/// A resident pays an operator's invoice online into the operator's own
/// Barion (stand-in server): the key is stored encrypted, the link shows only
/// what is needed, the payment is recorded on the invoice exactly once.
#[test]
fn resident_pays_an_invoice_online() {
    let _serial = BARION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    use lunda_core::store::finance::{ChargeInput, InvoiceInput};
    use std::sync::{Arc, Mutex};
    use temisz_server::resident_pay;
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let state = Arc::new(Mutex::new(("Prepared".to_string(), 0i64)));
    let seen = Arc::new(Mutex::new((String::new(), String::new())));
    let pid: String = lunda_core::util::new_id().replace('-', "");
    {
        let (state, seen, pid) = (state.clone(), seen.clone(), pid.clone());
        std::thread::spawn(move || {
            for mut rq in server.incoming_requests() {
                let mut body = String::new();
                let _ = rq.as_reader().read_to_string(&mut body);
                let key = rq.headers().iter().find(|h| h.field.equiv("x-pos-key")).map(|h| h.value.to_string()).unwrap_or_default();
                let answer = if rq.url().starts_with("/v2/Payment/Start") {
                    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
                    *seen.lock().unwrap() = (key, v["Transactions"][0]["Payee"].as_str().unwrap_or("").to_string());
                    state.lock().unwrap().1 = v["Transactions"][0]["Total"].as_i64().unwrap();
                    json!({ "PaymentId": pid, "Status": "Prepared", "GatewayUrl": "https://secure.test.barion.com/Pay?id=x", "Errors": [] })
                } else {
                    let (status, total) = state.lock().unwrap().clone();
                    json!({ "PaymentId": pid, "Status": status, "Total": total, "Currency": "HUF", "Errors": [] })
                };
                let _ = rq.respond(tiny_http::Response::from_string(answer.to_string()));
            }
        });
    }
    std::env::set_var("BARION_API_URL", format!("http://127.0.0.1:{port}"));
    std::env::set_var("CRON_SECRET", "teszt-titok-a-kulcsokhoz");

    let store = platform();
    let (tenant, email) = new_customer(&store, "resident");
    let user: String = store.conn.query_row("SELECT user_id FROM platform.accounts WHERE email = ?1", lunda_core::params![email], |r| r.get(0)).unwrap();
    let schema: String = store.conn.query_row("SELECT schema_name FROM platform.tenants WHERE id = ?1", lunda_core::params![tenant], |r| r.get(0)).unwrap();
    store.conn.use_schema(&schema).unwrap();
    let s = store.session_for(&user).unwrap();
    lunda_core::sample::load_sample(&store, &s).unwrap();
    let cem: String = store.conn.query_row("SELECT id FROM cemeteries LIMIT 1", [], |r| r.get(0)).unwrap();
    let rate = store
        .add_fee_rate(
            &s,
            &lunda_core::model::FeeRate {
                cemetery_id: cem.clone(),
                fee_kind: "renewal".into(),
                name: "Sírhely újraváltása".into(),
                amount_huf: 40_000,
                vat_rate: "27".into(),
                valid_from: "2020-01-01".into(),
                ..Default::default()
            },
        )
        .unwrap();
    let charge = store
        .add_charge(&s, &ChargeInput { cemetery_id: cem.clone(), payer_name: Some("Kovács János".into()), fee_rate_id: Some(rate.id), ..Default::default() })
        .unwrap();
    let inv = store
        .create_invoice(
            &s,
            &InvoiceInput {
                cemetery_id: cem.clone(),
                charge_ids: vec![charge.id],
                buyer_name: "Kovács János".into(),
                buyer_postcode: "9999".into(),
                buyer_city: "Mintavölgy".into(),
                buyer_address: "Fő utca 1.".into(),
                buyer_email: "kovacs@pelda.hu".into(),
                payment_method: "transfer".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(inv.gross_huf, 50_800);

    // no link before the operator connects its Barion
    assert_eq!(resident_pay::pay_link(&store, &s, &tenant, &inv.id, "https://www.temisz.hu").unwrap_err().code, "online_payment_off");
    let bad = json!({ "posKey": "nem-kulcs", "payee": "temeto@pelda.hu", "termsUrl": "https://temeto.pelda.hu/aszf", "live": false });
    assert_eq!(resident_pay::settings_save(&store.conn, &s, &tenant, &bad).unwrap_err().code, "validation");
    let key = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let saved = resident_pay::settings_save(
        &store.conn,
        &s,
        &tenant,
        &json!({ "posKey": key, "payee": "Temeto@Pelda.hu", "termsUrl": "https://temeto.pelda.hu/aszf", "live": false }),
    )
    .unwrap();
    assert_eq!(saved["configured"], true);
    assert!(saved.get("posKey").is_none(), "the key is never returned");
    let enc: Vec<u8> =
        store.conn.query_row("SELECT pos_key_enc FROM platform.tenant_barion WHERE tenant_id = ?1", lunda_core::params![tenant], |r| r.get(0)).unwrap();
    assert!(!String::from_utf8_lossy(&enc).contains(key), "stored encrypted");
    // saving again without a key keeps the stored one
    resident_pay::settings_save(
        &store.conn,
        &s,
        &tenant,
        &json!({ "posKey": "", "payee": "temeto@pelda.hu", "termsUrl": "https://temeto.pelda.hu/aszf", "live": false }),
    )
    .unwrap();

    let link = resident_pay::pay_link(&store, &s, &tenant, &inv.id, "https://www.temisz.hu").unwrap();
    let token = link["token"].as_str().unwrap().to_string();
    assert_eq!(link["url"], format!("https://www.temisz.hu/fizetes?t={token}"));
    let again = resident_pay::pay_link(&store, &s, &tenant, &inv.id, "https://www.temisz.hu").unwrap();
    assert_eq!(again["token"], token, "one link per invoice");

    let info = resident_pay::public_info(&store, &token).unwrap();
    assert_eq!((info["state"].as_str(), info["dueHuf"].as_i64(), info["payer"].as_str()), (Some("open"), Some(50_800), Some("Kovács J.")));
    assert!(info.get("buyerEmail").is_none() && !info.to_string().contains("Fő utca"), "no address or e-mail on the page");
    assert_eq!(resident_pay::public_start(&store, &token, "https://www.temisz.hu", false).unwrap_err().code, "terms_required");
    let started = resident_pay::public_start(&store, &token, "https://www.temisz.hu", true).unwrap();
    assert!(started["gatewayUrl"].as_str().unwrap().starts_with("https://"));
    assert_eq!(*seen.lock().unwrap(), (key.to_string(), "temeto@pelda.hu".to_string()), "the operator's own key and wallet");

    let paid = |store: &lunda_core::Store| -> (i64, String) {
        store.conn.use_schema(&schema).unwrap();
        let i = store.get_invoice(&inv.id).unwrap();
        (i.paid_huf, i.status)
    };
    // in progress: nothing recorded
    state.lock().unwrap().0 = "Started".into();
    assert_eq!(temisz_server::barion::refresh(&store, &pid).unwrap(), "Started");
    assert_eq!(paid(&store).0, 0);
    // succeeded: recorded once, by card, the invoice is paid
    state.lock().unwrap().0 = "Succeeded".into();
    assert_eq!(temisz_server::barion::refresh(&store, &pid).unwrap(), "Succeeded");
    temisz_server::barion::refresh(&store, &pid).unwrap();
    assert_eq!(paid(&store), (50_800, "paid".to_string()));
    let n: i64 =
        store.conn.query_row("SELECT count(*) FROM payments WHERE invoice_id = ?1 AND method = 'card'", lunda_core::params![inv.id], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
    // the page now says paid, and a new payment cannot start
    assert_eq!(resident_pay::public_info(&store, &token).unwrap()["state"], "paid");
    assert_eq!(resident_pay::public_start(&store, &token, "https://www.temisz.hu", true).unwrap_err().code, "invoice_not_open");
    // unknown links are not found
    assert_eq!(resident_pay::public_info(&store, "00000000000000000000000000000000").unwrap_err().code, "not_found");
}

/// One person in two organisations: invited with an address that already
/// signs in elsewhere, the same password opens both; a new password, set in
/// one, works in the other; a new organisation keeps the sign-in.
#[test]
fn one_person_in_several_organisations() {
    use temisz_server::{accounts, create_org, login, Current, LoginStep};
    let store = platform();
    let (_, email) = new_customer(&store, "multi");
    let (tenant_b, admin_b) = new_customer(&store, "multib");
    let req = Request::default();
    // the admin of B invites the person from A
    let b = accounts::account_by_email(&store.conn, &admin_b).unwrap().unwrap();
    store.conn.use_schema(&b.schema).unwrap();
    let s_b = store.session_for(&b.user_id).unwrap();
    let v = accounts::invite(&store, &s_b, &tenant_b, &json!({ "email": email.to_uppercase(), "displayName": "Kettős Kata", "role": "clerk" }), &req).unwrap();
    assert_eq!(v["joinedExisting"], true);
    assert_eq!(
        accounts::invite(&store, &s_b, &tenant_b, &json!({ "email": email, "displayName": "Kata", "role": "clerk" }), &req).unwrap_err().code,
        "already_member"
    );
    let orgs = accounts::memberships(&store.conn, &email).unwrap();
    assert_eq!(orgs.len(), 2);
    // the existing password signs in to B too
    let in_b = orgs.iter().find(|m| m.tenant_id == tenant_b).unwrap();
    store.conn.use_schema(&in_b.schema).unwrap();
    store.login(&email, "nagyon-titkos-1").unwrap();
    assert!(matches!(login(&store, &email, "nagyon-titkos-1", &req).unwrap(), LoginStep::Session(_)));
    // a new password in B also opens A
    store.conn.use_schema(&in_b.schema).unwrap();
    let me_b = store.session_for(&in_b.user_id).unwrap();
    store.change_password(&me_b, &in_b.user_id, Some("nagyon-titkos-1"), "masik-titkos-22").unwrap();
    accounts::sync_password(&store.conn, &in_b.id).unwrap();
    let in_a = orgs.iter().find(|m| m.tenant_id != tenant_b).unwrap();
    store.conn.use_schema(&in_a.schema).unwrap();
    assert_eq!(store.login(&email, "nagyon-titkos-1").unwrap_err().code, "invalid_credentials");
    store.login(&email, "masik-titkos-22").unwrap();
    // a confirmed address and two-factor sign-in carry over to a new organisation
    store
        .conn
        .execute(
            "UPDATE platform.accounts SET totp_secret = 'X', totp_enabled_at = now(), email_verified_at = now() WHERE id = ?1",
            lunda_core::params![in_a.id],
        )
        .unwrap();
    store.conn.use_schema(&in_a.schema).unwrap();
    let me_a = store.session_for(&in_a.user_id).unwrap();
    // a third organisation with the same sign-in
    let c = Current { session: me_a, tenant_id: in_a.tenant_id.clone(), account_id: in_a.id.clone(), mfa_setup: false };
    let new_acc = create_org(&store, &c, "Harmadik Egyházközség", &req).unwrap();
    let acc = accounts::account_by_id(&store.conn, &new_acc).unwrap();
    assert_eq!(acc.organization, "Harmadik Egyházközség");
    assert!(acc.totp_enabled && acc.verified);
    store.conn.use_schema(&acc.schema).unwrap();
    let s = store.login(&email, "masik-titkos-22").unwrap();
    assert_eq!(s.role.as_str(), "admin");
    assert_eq!(accounts::memberships(&store.conn, &email).unwrap().len(), 3);
}

/// A customer created by an older version gets the newer tenant migrations
/// when it is next opened (sign-in), instead of failing on a missing column.
#[test]
fn older_tenant_is_migrated_when_opened() {
    use temisz_server::{accounts, login};
    let store = platform();
    let (_, email) = new_customer(&store, "old");
    let acc = accounts::account_by_email(&store.conn, &email).unwrap().unwrap();
    // as it was before the per-cemetery limit existed
    store.conn.use_schema(&acc.schema).unwrap();
    store.conn.execute_batch("ALTER TABLE users DROP COLUMN cemetery_ids; DELETE FROM schema_version WHERE version >= 6").unwrap();
    // a new server process (cold start) signs in
    let fresh = Store::new(Connection::connect(&db::database_url()).unwrap());
    login(&fresh, &email, "nagyon-titkos-1", &Request::default()).unwrap();
    let n: i64 = fresh.conn.query_row("SELECT count(*) FROM users WHERE cemetery_ids = '[]'", [], |r| r.get(0)).unwrap();
    assert_eq!(n, 1);
}

/// Card subscription against a stand-in Barion: the card check reserves a
/// small amount and releases it, the trial runs 15 days with no export,
/// then the saved card is charged (final invoice, period). A failed charge
/// is retried; a charge Barion decides later is settled once, never charged
/// twice; a cancelled subscription is not charged again. An unfinished card
/// check leaves a working cemetery alone, and a trial that ended unpaid is
/// not followed by another free period.
#[test]
fn card_subscription_trial_then_automatic_charges() {
    let _guard = BARION_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use std::sync::{Arc, Mutex};
    use temisz_server::{barion, billing, subscriptions};
    struct Mock {
        check_status: String,
        finished_with: Vec<i64>,
        charges: Vec<i64>,
        start_status: String,
        renew_state: String,
        cards: Vec<String>,
    }
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let port = server.server_addr().to_ip().unwrap().port();
    let mock = Arc::new(Mutex::new(Mock {
        check_status: "Prepared".into(),
        finished_with: vec![],
        charges: vec![],
        start_status: "Succeeded".into(),
        renew_state: "InProgress".into(),
        cards: vec![],
    }));
    let run = lunda_core::util::new_id().replace('-', "");
    let fee = billing::gross(billing::MONTHLY_NET);
    {
        let mock = mock.clone();
        let run = run.clone();
        std::thread::spawn(move || {
            let mut n = 0;
            for mut rq in server.incoming_requests() {
                let mut body = String::new();
                let _ = rq.as_reader().read_to_string(&mut body);
                let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
                let mut m = mock.lock().unwrap();
                let url = rq.url().to_string();
                n += 1;
                let answer = if url.starts_with("/v2/Payment/Start") && v["InitiateRecurrence"] == true {
                    assert_eq!(v["PaymentType"], "Reservation");
                    m.cards.push(v["RecurrenceId"].as_str().unwrap().to_string());
                    json!({ "PaymentId": format!("{run}-c0c0-{n}"), "Status": "Prepared", "GatewayUrl": "https://secure.test.barion.com/Pay?id=c", "Errors": [] })
                } else if url.starts_with("/v2/Payment/Start") {
                    if m.cards.iter().any(|c| Some(c.as_str()) == v["RecurrenceId"].as_str()) {
                        let total = v["Transactions"][0]["Total"].as_i64().unwrap();
                        m.charges.push(total);
                    }
                    json!({ "PaymentId": format!("{run}-a0a0-{n}"), "Status": m.start_status, "Errors": [] })
                } else if url.starts_with("/v2/Payment/FinishReservation") {
                    m.finished_with.push(v["Transactions"][0]["Total"].as_i64().unwrap());
                    json!({ "Status": "Succeeded", "Errors": [] })
                } else {
                    let pid = url.trim_start_matches("/v4/payment/").split('/').next().unwrap_or("").to_string();
                    if pid.contains("-c0c0-") {
                        json!({ "PaymentId": pid, "Status": m.check_status, "Total": 100, "Currency": "HUF", "TraceId": "TRACE-1",
                                "Transactions": [{ "TransactionId": "T1", "POSTransactionId": "check-x", "Total": 100 }], "Errors": [] })
                    } else {
                        json!({ "PaymentId": pid, "Status": m.renew_state, "Total": 57150, "Currency": "HUF", "Errors": [] })
                    }
                };
                let _ = rq.respond(tiny_http::Response::from_string(answer.to_string()));
            }
        });
    }
    std::env::set_var("BARION_POS_KEY", "11111111-2222-3333-4444-555555555555");
    std::env::set_var("BARION_PAYEE", "penz@temisz.hu");
    std::env::set_var("BARION_API_URL", format!("http://127.0.0.1:{port}"));

    let store = platform();
    // card subscriptions left over from earlier local runs are not charged again
    store.conn.execute("UPDATE platform.cemetery_subscriptions SET status = 'lapsed' WHERE payment_method = 'card'", []).unwrap();
    let (tenant, email) = new_customer(&store, "cardsub");
    let (cem, s) = cemetery_of(&store, &email, "Kártyás temető");
    details(&store, &tenant, &email);
    let daily = || {
        subscriptions::daily(&store, "https://www.temisz.hu/app").unwrap();
        use_tenant(&store, &tenant);
    };
    let sub = |c: &str| subscriptions::get(&store.conn, &tenant, c).unwrap();
    let start = |c: &str| -> String {
        let v = subscriptions::start_card(&store, &s, &tenant, c, "monthly", "https://www.temisz.hu/app", "https://www.temisz.hu").unwrap();
        assert!(v["gatewayUrl"].as_str().unwrap().starts_with("https://"));
        v["paymentId"].as_str().unwrap().to_string()
    };
    let end_period_today = |c: &str| {
        store
            .conn
            .execute("UPDATE platform.cemetery_subscriptions SET current_period_end = CURRENT_DATE WHERE cemetery_id = ?1", lunda_core::params![c])
            .unwrap();
    };
    let end_trial = |c: &str| {
        store
            .conn
            .execute("UPDATE platform.cemetery_subscriptions SET trial_ends_at = now() - interval '1 hour' WHERE cemetery_id = ?1", lunda_core::params![c])
            .unwrap();
    };
    // offered only once Barion enabled token payments for the shop
    std::env::remove_var("BARION_RECURRING");
    assert_eq!(subscriptions::start_card(&store, &s, &tenant, &cem, "monthly", "https://x/app", "https://x").unwrap_err().code, "card_unavailable");
    std::env::set_var("BARION_RECURRING", "1");

    let pid = start(&cem);
    // the check is not finished yet: nothing changed
    assert_eq!(barion::refresh(&store, &pid).unwrap(), "Prepared");
    assert!(sub(&cem).is_none());
    assert_eq!(subscriptions::locked_cemeteries(&store, &tenant).unwrap(), vec![cem.clone()]);
    // reserved: released at once (0 Ft), the trial starts
    mock.lock().unwrap().check_status = "Reserved".into();
    assert_eq!(barion::refresh(&store, &pid).unwrap(), "Succeeded");
    assert_eq!(mock.lock().unwrap().finished_with, vec![0], "nothing is charged by the check");
    assert_eq!(sub(&cem).unwrap().status, "trial");
    let l = subscriptions::limits(&store, &tenant).unwrap();
    assert!(l.locked.is_empty());
    assert_eq!((l.unpaid, l.marked), (vec![cem.clone()], vec![cem.clone()]), "trial: no export, marked prints");
    // a second callback changes nothing
    barion::refresh(&store, &pid).unwrap();
    assert!(mock.lock().unwrap().charges.is_empty());
    // the trial ends: the period fee is charged with the saved card
    end_trial(&cem);
    daily();
    assert_eq!(mock.lock().unwrap().charges, vec![fee]);
    let paid = sub(&cem).unwrap();
    assert_eq!(paid.status, "active");
    assert!(paid.paid && paid.period_end.is_some());
    let invoices = |c: &str| -> i64 {
        store
            .conn
            .query_row(
                "SELECT count(*) FROM platform.invoices WHERE cemetery_id = ?1 AND kind = 'invoice' AND payment_method = 'card'",
                lunda_core::params![c],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(invoices(&cem), 1);
    assert!(subscriptions::limits(&store, &tenant).unwrap().unpaid.is_empty(), "paid: export allowed");
    // the next period: the card is refused, retried the next day, stays writable meanwhile
    mock.lock().unwrap().start_status = "Failed".into();
    end_period_today(&cem);
    daily();
    assert_eq!(sub(&cem).unwrap().status, "past_due");
    assert!(subscriptions::locked_cemeteries(&store, &tenant).unwrap().is_empty());
    mock.lock().unwrap().start_status = "Succeeded".into();
    daily();
    assert_eq!(sub(&cem).unwrap().status, "active");
    assert_eq!(invoices(&cem), 2);
    // Barion does not decide at once: nothing is charged again while it is pending …
    mock.lock().unwrap().start_status = "InProgress".into();
    end_period_today(&cem);
    daily();
    let started = mock.lock().unwrap().charges.len();
    daily();
    daily();
    assert_eq!(mock.lock().unwrap().charges.len(), started, "a pending charge is not repeated");
    assert_eq!(invoices(&cem), 2);
    // … and when it succeeds later, it is settled once
    mock.lock().unwrap().renew_state = "Succeeded".into();
    daily();
    daily();
    assert_eq!(mock.lock().unwrap().charges.len(), started);
    assert_eq!(invoices(&cem), 3);
    assert_eq!(sub(&cem).unwrap().status, "active");
    mock.lock().unwrap().start_status = "Succeeded".into();
    // cancelled: nothing more is charged, locked after the paid period
    let charges = mock.lock().unwrap().charges.len();
    subscriptions::cancel(&store, &s, &tenant, &cem, false).unwrap();
    end_period_today(&cem);
    daily();
    assert_eq!(mock.lock().unwrap().charges.len(), charges);
    assert_eq!(subscriptions::locked_cemeteries(&store, &tenant).unwrap(), vec![cem.clone()]);

    // an unfinished card check leaves a working cemetery as it was
    let c2 = api::dispatch(&store, &s, "cemeteries.save", &json!({ "cemetery": { "name": "Minta temető" } })).unwrap()["id"].as_str().unwrap().to_string();
    subscriptions::register_demo(&store.conn, &tenant, &c2, "Minta temető").unwrap();
    mock.lock().unwrap().check_status = "Prepared".into();
    let pid2 = start(&c2);
    barion::refresh(&store, &pid2).unwrap();
    assert_eq!(sub(&c2).unwrap().status, "demo");
    assert!(!subscriptions::locked_cemeteries(&store, &tenant).unwrap().contains(&c2));
    // finished: the sample cemetery becomes a card subscription with its own trial
    mock.lock().unwrap().check_status = "Reserved".into();
    barion::refresh(&store, &pid2).unwrap();
    assert_eq!(sub(&c2).unwrap().status, "trial");
    // that trial is cancelled and runs out unpaid …
    subscriptions::cancel(&store, &s, &tenant, &c2, false).unwrap();
    end_trial(&c2);
    daily();
    assert_eq!(sub(&c2).unwrap().status, "lapsed");
    // … so a new cemetery gets no second free period: the card is charged at once
    let c3 = api::dispatch(&store, &s, "cemeteries.save", &json!({ "cemetery": { "name": "Új temető" } })).unwrap()["id"].as_str().unwrap().to_string();
    let before = mock.lock().unwrap().charges.len();
    mock.lock().unwrap().start_status = "Failed".into();
    let pid3 = start(&c3);
    assert_eq!(barion::refresh(&store, &pid3).unwrap(), "ChargeFailed");
    assert_eq!(sub(&c3).unwrap().status, "lapsed", "no payment, no use");
    mock.lock().unwrap().start_status = "Succeeded".into();
    let pid4 = start(&c3);
    assert_eq!(barion::refresh(&store, &pid4).unwrap(), "Succeeded");
    let s3 = sub(&c3).unwrap();
    assert_eq!((s3.status.as_str(), s3.paid, s3.trial_ends.is_none()), ("active", true, true));
    assert_eq!(mock.lock().unwrap().charges.len(), before + 2);
    std::env::remove_var("BARION_RECURRING");
}

/// temisz.hu/bemutato: an uploaded table is kept encrypted, only the admin
/// gets it back, wrong file types and missing consent are refused.
#[test]
fn demo_request_upload_is_stored_sealed() {
    use base64::Engine;
    use temisz_server::demo_requests;
    // the key is process-wide: not while another test seals with it
    let _serial = BARION_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    std::env::set_var("SECRETS_KEY", "teszt-titok");
    let store = platform();
    let file = b"nev;sirhely\nKovacs Janos;A/1/1\n".to_vec();
    let town = format!("Próbafalva {}", lunda_core::util::new_id());
    let req = |body: serde_json::Value| Request {
        method: "POST".into(),
        path: "/api/public/demo-request".into(),
        headers: vec![("content-type".into(), "application/json".into())],
        body: body.to_string().into_bytes(),
        ip: format!("198.51.100.{}", rand_octet()),
    };
    let ok = json!({ "settlement": town, "name": "Jegyző Judit", "role": "jegyző", "email": "jegyzo@pelda.hu", "consent": true,
                     "fileName": "temeto.csv", "content": base64::engine::general_purpose::STANDARD.encode(&file) });
    let mut no_consent = ok.clone();
    no_consent["consent"] = json!(false);
    assert_eq!(demo_requests::submit(&store.conn, &req(no_consent)).unwrap_err().code, "consent_required");
    let mut exe = ok.clone();
    exe["fileName"] = json!("virus.exe");
    assert_eq!(demo_requests::submit(&store.conn, &req(exe)).unwrap_err().code, "file_type");
    demo_requests::submit(&store.conn, &req(ok)).unwrap();
    let raw: Vec<u8> =
        store.conn.query_row("SELECT file_data FROM platform.demo_requests WHERE settlement = ?1", lunda_core::params![town], |r| r.get(0)).unwrap();
    assert!(!String::from_utf8_lossy(&raw).contains("Kovacs"), "stored encrypted");
    let list = demo_requests::list(&store.conn).unwrap();
    let item = list.as_array().unwrap().iter().find(|x| x["settlement"] == town.as_str()).unwrap().clone();
    let back = demo_requests::download(&store.conn, item["id"].as_str().unwrap()).unwrap();
    assert_eq!(base64::engine::general_purpose::STANDARD.decode(back["data"].as_str().unwrap()).unwrap(), file);
    demo_requests::delete(&store.conn, item["id"].as_str().unwrap()).unwrap();
}

fn rand_octet() -> u8 {
    (lunda_core::util::new_id().bytes().map(|b| b as u32).sum::<u32>() % 250) as u8 + 1
}
