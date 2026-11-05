//! UC-05: import from Excel/CSV with dry run, paper folios and the check report.

mod common;

use common::*;
use lunda_core::io::{guess_mapping, import_rows, migration_check, read_csv, ImportOptions};
use lunda_core::model::EventFilter;

const CSV: &str = "Folyószám;Temetés napja;Elhunyt neve;Születési idő;Anyja neve;Lakóhely;Halálozás ideje;Sírhely;Jogosult;Jogosult címe;Megváltás ideje
101;1965.03.02;Kovács István;1901;Nagy Mária;Mintavölgy;1965.02.27;B/1/1;Kovács Péter;Mintavölgy, Fő u. 1.;25
102;1966.04.10;özv. Szabó Jánosné;1890.04.05;Tóth Ilona;Mintavölgy;1966.04.07;B/1/2;Szabó Anna;Mintavölgy, Petőfi u. 3.;25
103;rossz dátum;Hibás Henrik;1900.01.01;X;Y;1967.01.01;B/1/3;;;
104;1970.01.10;Nagy Gizella;1910.01.01;Kiss Teréz;Mintavölgy;1970.01.08;B/1/1;;;
";

fn options(fx: &Fx, dry: bool, skip: bool, headers: &[String]) -> ImportOptions {
    ImportOptions {
        cemetery_id: fx.cemetery.id.clone(),
        mapping: guess_mapping(headers),
        keep_folio: true,
        dry_run: dry,
        skip_invalid: skip,
        create_leases: true,
        ..Default::default()
    }
}

#[test]
fn dry_run_changes_nothing_and_reports_errors() {
    let fx = fixture();
    let t = read_csv(CSV.as_bytes()).unwrap();
    let m = guess_mapping(&t.headers);
    assert_eq!(m["folio"], 0);
    assert_eq!(m["eventDate"], 1);
    assert_eq!(m["fullName"], 2);
    assert_eq!(m["plotIdentifier"], 7);
    assert_eq!(m["holderName"], 8);
    let before: i64 = fx.store.conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    let rep = import_rows(&fx.store, &fx.admin, &t, &options(&fx, true, false, &t.headers)).unwrap();
    assert!(!rep.committed);
    assert_eq!(rep.rows_total, 4);
    assert_eq!(rep.rows_ok, 3);
    assert_eq!(rep.rows_failed, 1);
    assert_eq!(rep.errors[0].row, 3);
    assert_eq!(rep.errors[0].field, "eventDate");
    assert!(rep.warnings.iter().any(|w| w.code == "year_only"));
    assert_eq!(rep.preview[1]["name"], "özv. Szabó Jánosné");
    // nothing persisted, not even audit entries
    let after: i64 = fx.store.conn.query_row("SELECT count(*) FROM audit_log", [], |r| r.get(0)).unwrap();
    assert_eq!(before, after);
    assert_eq!(fx.store.list_events(&EventFilter::default()).unwrap().total, 0);
    assert!(fx.store.find_plot(&fx.cemetery.id, "B/1/1").unwrap().is_none());
}

#[test]
fn all_or_nothing_without_skip() {
    let fx = fixture();
    let t = read_csv(CSV.as_bytes()).unwrap();
    let rep = import_rows(&fx.store, &fx.admin, &t, &options(&fx, false, false, &t.headers)).unwrap();
    assert!(!rep.committed);
    assert_eq!(fx.store.list_events(&EventFilter::default()).unwrap().total, 0);
}

#[test]
fn import_keeps_paper_folios_and_creates_plots_holders_leases() {
    let fx = fixture();
    let t = read_csv(CSV.as_bytes()).unwrap();
    let rep = import_rows(&fx.store, &fx.admin, &t, &options(&fx, false, true, &t.headers)).unwrap();
    assert!(rep.committed, "{rep:?}");
    assert_eq!(rep.rows_ok, 3);
    assert_eq!(rep.plots_created, 2);
    let ev = fx.store.list_events(&EventFilter::default()).unwrap().items;
    let folios: Vec<i64> = ev.iter().map(|e| e.folio).collect();
    assert_eq!(folios, vec![101, 102, 104]);
    assert!(ev.iter().all(|e| e.historical));
    // second coffin in a single grave in 1970 (rest period running) is kept as history with a note
    let raw = fx.store.get_event(&ev[2].id).unwrap();
    assert!(raw.override_reason.unwrap().contains("Papír alapú"));
    // holder and lease
    let p = fx.plot("B/1/2");
    let d = fx.store.plot_detail(&fx.admin, &p.id).unwrap();
    assert_eq!(d.holders[0].person_name, "Szabó Anna");
    assert_eq!(d.current_lease.unwrap().expiry_date, "1991-04-10");
    // new entries continue after the highest paper folio
    let r = fx.burial("A/1/1", "coffin_burial", "2026-01-05", "Új", "Ünige").unwrap();
    assert_eq!(r.folio, 105);
    // check report (UC-05 step 4)
    let c = migration_check(&fx.store, Some(&fx.cemetery.id)).unwrap();
    assert!(c.deceased_without_plot.is_empty());
    assert!(c.occupied_plots_without_lease.is_empty(), "{:?}", c.occupied_plots_without_lease);
}

#[test]
fn import_requires_permission() {
    let fx = fixture();
    let t = read_csv(CSV.as_bytes()).unwrap();
    let clerk_without_import = lunda_core::auth::Session { permissions: vec![lunda_core::auth::Permission::RecordEvents], ..session(lunda_core::auth::Role::Clerk) };
    assert_eq!(import_rows(&fx.store, &clerk_without_import, &t, &options(&fx, true, false, &t.headers)).unwrap_err().code, "forbidden");
}

#[test]
fn full_export_formats() {
    let fx = fixture();
    fx.burial("A/1/1", "coffin_burial", "2020-01-01", "Export", "Elek").unwrap();
    let dir = std::env::temp_dir().join(format!("lunda-exp-{}", lunda_core::util::new_id()));
    for fmt in ["xlsx", "json", "csv"] {
        let f = lunda_core::io::export(&fx.store, &fx.admin, "full", fmt, &serde_json::json!({}), &dir).unwrap();
        assert!(std::path::Path::new(&f.path).exists(), "{fmt}");
    }
    let j = lunda_core::io::export(&fx.store, &fx.admin, "full", "json", &serde_json::json!({}), &dir).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&j.path).unwrap()).unwrap();
    assert_eq!(v["data"]["deceased"][0]["family_name"], "Export");
    assert!(v["data"]["users"].as_array().unwrap().iter().all(|u| u.get("password_hash").is_none()));
    assert!(!String::from_utf8_lossy(&std::fs::read(&j.path).unwrap()).contains("internal.recovery_key"));
    // reader cannot export personal data
    let reader = session(lunda_core::auth::Role::Reader);
    assert_eq!(lunda_core::io::export(&fx.store, &reader, "full", "json", &serde_json::json!({}), &dir).unwrap_err().code, "forbidden");
    assert!(lunda_core::io::export(&fx.store, &reader, "report_yearly", "xlsx", &serde_json::json!({}), &dir).is_ok());
    std::fs::remove_dir_all(dir).ok();
}
