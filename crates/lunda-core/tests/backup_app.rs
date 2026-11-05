//! FR-NAP-03..06 and UC-06 through the application layer used by the desktop shell.

use lunda_core::app::App;
use lunda_core::{backup, db};
use serde_json::json;

fn tmp() -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("lunda-app-{}", lunda_core::util::new_id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn backup_restore_and_new_machine_restore() {
    let dir = tmp();
    let key = db::generate_key();
    let mut app = App::open(dir.clone(), Some(key.clone()), None);
    assert!(app.status().db_ready);
    app.setup_admin("admin", "Admin", "admin-jelszo").unwrap();
    app.call("sample.load", &json!({})).unwrap();
    // daily backup is due on a fresh database, then not again
    assert!(app.backup_if_due().unwrap());
    assert!(!app.backup_if_due().unwrap());
    // no external directory configured -> warning stays (FR-NAP-06)
    assert!(app.status().backup_warning);
    let ext = dir.join("pendrive");
    app.call("settings.set", &json!({"values": {"backup.directory": ext.display().to_string()}})).unwrap();
    let b: serde_json::Value = app.call("backup.create", &json!({})).unwrap();
    assert!(!app.status().backup_warning);
    let path = b["path"].as_str().unwrap().to_string();
    let rk: String = serde_json::from_value(app.call("backup.recoveryKey", &json!({})).unwrap()).unwrap();
    let rep = app.call("backup.verify", &json!({"path": path, "recoveryKey": rk})).unwrap();
    assert_eq!(rep["checksumOk"], true);
    assert_eq!(rep["auditChainOk"], true);

    // change data after the backup, then restore: the change disappears
    let before: i64 = app.store.as_ref().unwrap().conn.query_row("SELECT count(*) FROM interment_events", [], |r| r.get(0)).unwrap();
    app.call("cemeteries.save", &json!({"cemetery": {"name": "Utólag felvett temető"}})).unwrap();
    app.call("backup.restore", &json!({"path": path, "recoveryKey": rk})).unwrap();
    // the session ends after a restore
    assert!(app.status().session.is_none());
    app.login("admin", "admin-jelszo").unwrap();
    let cems = app.call("cemeteries.list", &json!({})).unwrap();
    assert_eq!(cems.as_array().unwrap().len(), 1);
    let after: i64 = app.store.as_ref().unwrap().conn.query_row("SELECT count(*) FROM interment_events", [], |r| r.get(0)).unwrap();
    assert_eq!(before, after);
    // a pre-restore backup of the replaced state exists
    let list: Vec<backup::BackupInfo> = serde_json::from_value(app.call("backup.list", &json!({})).unwrap()).unwrap();
    assert!(list.iter().any(|b| b.manifest.kind == "pre-restore"));

    // new computer: no key in the keychain -> database cannot be opened
    drop(app);
    let dir2 = tmp();
    let mut fresh = App::open(dir2.clone(), Some(db::generate_key()), None);
    fresh.restore_fresh(std::path::Path::new(&path), &rk, &db::generate_key()).unwrap();
    fresh.login("admin", "admin-jelszo").unwrap();
    assert_eq!(fresh.call("cemeteries.list", &json!({})).unwrap().as_array().unwrap().len(), 1);
    // the restored database keeps the same recovery key
    let rk2: String = serde_json::from_value(fresh.call("backup.recoveryKey", &json!({})).unwrap()).unwrap();
    assert_eq!(rk, rk2);
    // restore_fresh is refused once users exist (must log in and use backup.restore)
    assert_eq!(fresh.restore_fresh(std::path::Path::new(&path), &rk, &db::generate_key()).unwrap_err().code, "forbidden");

    // missing key on an existing database is reported, not silently recreated
    drop(fresh);
    let missing = App::open(dir2.clone(), None, None);
    assert_eq!(missing.db_error.as_ref().unwrap().code, "db_key_missing");
    std::fs::remove_dir_all(dir).ok();
    std::fs::remove_dir_all(dir2).ok();
}

#[test]
fn idle_session_expires() {
    let dir = tmp();
    let mut app = App::open(dir.clone(), Some(db::generate_key()), None);
    app.setup_admin("admin", "Admin", "admin-jelszo").unwrap();
    app.call("settings.set", &json!({"values": {"security.idle_logout_minutes": "1"}})).unwrap();
    app.last_activity = std::time::Instant::now() - std::time::Duration::from_secs(120);
    assert_eq!(app.call("dashboard", &json!({})).unwrap_err().code, "session_expired");
    std::fs::remove_dir_all(dir).ok();
}

