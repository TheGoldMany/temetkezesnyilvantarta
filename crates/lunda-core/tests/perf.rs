//! NFR-TEL-02: with 100 000 deceased and 50 000 plots, search answers under
//! 300 ms. Run in release mode for meaningful numbers:
//!   cargo test -p lunda-core --release --test perf -- --ignored --nocapture
//! (debug builds use a relaxed limit so the test also runs in `cargo test`).

use std::time::Instant;

use lunda_core::auth::Session;
use lunda_core::model::{ExpiryFilter, PlotFilter};
use lunda_core::{db, sample, Store};

const LIMIT_MS: u128 = if cfg!(debug_assertions) { 1500 } else { 300 };

fn encrypted_store() -> (Store, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("lunda-perf-{}", lunda_core::util::new_id()));
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::open(&dir.join("perf.db"), &db::generate_key(), |_, _| Ok(())).unwrap();
    (Store::new(conn), dir)
}

#[test]
#[ignore = "slow: run with --release -- --ignored (CI does)"]
fn search_100k_deceased_under_300ms() {
    let (store, dir) = encrypted_store();
    let t = Instant::now();
    let cem = sample::generate_large(&store, 50_000, 100_000).unwrap();
    eprintln!("generated 50 000 plots / 100 000 deceased in {:?}", t.elapsed());
    let s = Session::system();

    let queries = ["kovacs", "Kovács József", "szabo maria", "gyor", "nagy 1975", "tóth ilo", "P3/12/4", "zsigmond", "ács"];
    let mut worst = 0u128;
    for q in queries {
        // warm once, then measure (the first query also loads pages from disk)
        let _ = store.quick_search(&s, q, 30).unwrap();
        let t = Instant::now();
        let hits = store.quick_search(&s, q, 30).unwrap();
        let ms = t.elapsed().as_millis();
        worst = worst.max(ms);
        eprintln!("search {q:>16}: {:>4} ms, {} hits", ms, hits.len());
        assert!(!hits.is_empty(), "no hits for {q}");
    }
    assert!(worst < LIMIT_MS, "slowest search {worst} ms >= {LIMIT_MS} ms");

    // plot list of one section and the full map data set
    let t = Instant::now();
    let page = store.plot_summaries(&PlotFilter { cemetery_id: Some(cem.clone()), light: true, ..Default::default() }).unwrap();
    let map_ms = t.elapsed().as_millis();
    eprintln!("all 50 000 plots for the map: {map_ms} ms");
    assert_eq!(page.total, 50_000);
    assert!(map_ms < if cfg!(debug_assertions) { 6000 } else { 1000 }, "map data {map_ms} ms");
    let t = Instant::now();
    let page = store.plot_summaries(&PlotFilter { cemetery_id: Some(cem.clone()), limit: Some(100), ..Default::default() }).unwrap();
    eprintln!("full plot list with names (first 100 of {}): {:?}", page.total, t.elapsed());

    let t = Instant::now();
    let exp = store.expiries(&s, &ExpiryFilter { cemetery_id: Some(cem), within_days: 365, include_expired: false, ..Default::default() }).unwrap();
    eprintln!("expiries next 365 days: {} rows in {:?}", exp.len(), t.elapsed());
    std::fs::remove_dir_all(dir).ok();
}
