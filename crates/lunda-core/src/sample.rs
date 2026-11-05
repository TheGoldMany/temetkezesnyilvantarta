//! Fictitious sample data (FR-BEA-04 "mintaadatbázis") and large generated
//! data sets for performance tests (NFR-TEL-02). Names are random
//! combinations of common Hungarian names – no real persons.

use crate::sql::params;
use chrono::{Datelike, Duration, NaiveDate};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::Serialize;

use crate::auth::Session;
use crate::error::Result;
use crate::model::*;
use crate::store::Store;
use crate::text::normalize;
use crate::util::{fmt_date, new_id, now_ts, today};

pub const FAMILY: &[&str] = &[
    "Nagy",
    "Kovács",
    "Tóth",
    "Szabó",
    "Horváth",
    "Varga",
    "Kiss",
    "Molnár",
    "Németh",
    "Farkas",
    "Balogh",
    "Papp",
    "Takács",
    "Juhász",
    "Mészáros",
    "Simon",
    "Rácz",
    "Fekete",
    "Szilágyi",
    "Török",
    "Fehér",
    "Balázs",
    "Gál",
    "Szűcs",
    "Kocsis",
    "Pintér",
    "Fodor",
    "Szalai",
    "Sipos",
    "Magyar",
    "Lukács",
    "Gulyás",
    "Bíró",
    "Király",
    "Katona",
    "Jakab",
    "Antal",
    "Sándor",
    "Fazekas",
    "Kelemen",
    "Pásztor",
    "Csonka",
    "Zsigmond",
    "Győri",
    "Dudás",
    "Lengyel",
    "Halász",
    "Csizmadia",
    "Ördög",
    "Szőke",
    "Nyíri",
    "Ács",
    "Erdős",
    "Dzsupin",
    "Tyukodi",
    "Lyka",
];
pub const MALE: &[&str] = &[
    "József", "János", "István", "László", "Ferenc", "Sándor", "Gyula", "Lajos", "Mihály", "Imre", "Károly", "Béla", "Tibor", "Zoltán", "Pál", "Antal",
    "Gábor", "Géza", "Ödön", "Árpád",
];
pub const FEMALE: &[&str] = &[
    "Mária",
    "Erzsébet",
    "Ilona",
    "Katalin",
    "Éva",
    "Anna",
    "Margit",
    "Zsuzsanna",
    "Julianna",
    "Erika",
    "Irén",
    "Rozália",
    "Magdolna",
    "Terézia",
    "Gizella",
    "Ágnes",
    "Etelka",
    "Jolán",
];
const STREETS: &[&str] =
    &["Fő utca", "Kossuth Lajos utca", "Petőfi Sándor utca", "Rákóczi út", "Dózsa György út", "Arany János utca", "Szent István tér", "Béke utca"];
const TOWN: &str = "9999 Mintavölgy";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SampleSummary {
    pub cemetery_id: String,
    pub plots: i64,
    pub events: i64,
}

fn pick<'a>(r: &mut StdRng, v: &[&'a str]) -> &'a str {
    v[r.gen_range(0..v.len())]
}

fn random_deceased(r: &mut StdRng, death: NaiveDate) -> Deceased {
    let male = r.gen_bool(0.5);
    let family = pick(r, FAMILY).to_string();
    let given = if male { pick(r, MALE) } else { pick(r, FEMALE) }.to_string();
    let age = r.gen_range(45..98);
    let birth = NaiveDate::from_ymd_opt(death.year() - age, r.gen_range(1..=12), r.gen_range(1..=28)).unwrap();
    let birth_name = if !male && r.gen_bool(0.6) { format!("{} {}", pick(r, FAMILY), given) } else { String::new() };
    Deceased {
        family_name: family,
        given_name: given,
        name_prefix: if r.gen_bool(0.04) {
            "dr.".into()
        } else if !male && r.gen_bool(0.1) {
            "özv.".into()
        } else {
            String::new()
        },
        birth_name,
        sex: if male { "male" } else { "female" }.into(),
        birth_place: pick(r, &["Mintavölgy", "Kecskemét", "Szeged", "Budapest", "Cegléd", "Kiskunfélegyháza"]).into(),
        birth_date: Some(fmt_date(birth)),
        mother_name: format!("{} {}", pick(r, FAMILY), pick(r, FEMALE)),
        last_residence: format!("{TOWN}, {} {}.", pick(r, STREETS), r.gen_range(1..120)),
        death_place: "Mintavölgy".into(),
        death_date: Some(fmt_date(death - Duration::days(r.gen_range(2..9)))),
        death_cert_no: format!("{}/{}", r.gen_range(100..9999), death.year()),
        ..Default::default()
    }
}

fn random_person(r: &mut StdRng) -> Person {
    Person {
        family_name: pick(r, FAMILY).into(),
        given_name: if r.gen_bool(0.5) { pick(r, MALE) } else { pick(r, FEMALE) }.into(),
        address: format!("{TOWN}, {} {}.", pick(r, STREETS), r.gen_range(1..120)),
        phone: format!("+36 30 {:03} {:04}", r.gen_range(100..999), r.gen_range(1000..9999)),
        ..Default::default()
    }
}

/// Loads the demo cemetery "Mintavölgyi köztemető" through the normal services
/// so every rule, folio and audit entry is exercised.
pub fn load_sample(store: &Store, s: &Session) -> Result<SampleSummary> {
    let mut r = StdRng::seed_from_u64(20261105);
    let cem = store.save_cemetery(
        s,
        &Cemetery {
            name: "Mintavölgyi köztemető".into(),
            address: format!("{TOWN}, Temető utca 1."),
            parcel_number: "0123/4".into(),
            kind: "public".into(),
            owner: "Mintavölgy Község Önkormányzata".into(),
            operator: "Mintavölgyi Településüzemeltető Kft.".into(),
            opening_hours: "Április–szeptember: 7–20 óra, október–március: 8–17 óra".into(),
            notes: "Kitalált mintaadat, kipróbáláshoz.".into(),
            ..Default::default()
        },
    )?;
    let t = |code: &str| store.get_plot_type(code).map(|t| t.id);
    let layout: &[(&str, &str, &str, i32, i32, f64, f64)] = &[
        ("A", "Régi temetőrész", "single", 6, 16, 0.0, 0.0),
        ("B", "Kettős sírok", "double", 4, 10, 300.0, 0.0),
        ("C", "Új temetőrész", "single", 5, 14, 0.0, 230.0),
        ("U", "Urnafal", "urn_niche", 3, 12, 300.0, 175.0),
        ("K", "Sírboltok", "crypt", 1, 5, 300.0, 265.0),
        ("S", "Szóróparcella", "scatter", 1, 1, 480.0, 265.0),
        ("KS", "Közös sír", "common", 1, 1, 520.0, 265.0),
    ];
    let mut plots = 0;
    for (code, name, ty, rows, per, x, y) in layout {
        let res = store.bulk_create(
            s,
            &BulkCreateInput {
                cemetery_id: cem.id.clone(),
                section_code: (*code).into(),
                section_name: (*name).into(),
                row_from: 1,
                row_to: *rows,
                plots_per_row: *per,
                first_plot_number: 1,
                plot_type_id: t(ty)?,
                origin_x: Some(*x),
                origin_y: Some(*y),
            },
        )?;
        plots += res.plots_created;
    }
    for (ty, purchase, renewal) in
        [("single", 85_000, 70_000), ("double", 160_000, 130_000), ("urn_niche", 45_000, 38_000), ("crypt", 600_000, 450_000), ("child", 40_000, 32_000)]
    {
        let tid = t(ty)?;
        for (kind, amount) in [("purchase", purchase), ("renewal", renewal)] {
            store.add_fee_rate(
                s,
                &FeeRate {
                    cemetery_id: cem.id.clone(),
                    plot_type_id: Some(tid.clone()),
                    fee_kind: kind.into(),
                    amount_huf: amount,
                    valid_from: "1990-01-01".into(),
                    ..Default::default()
                },
            )?;
        }
    }

    if let Some(p) = store.find_plot(&cem.id, "C/5/14")? {
        store.add_restriction(
            s,
            &Restriction { plot_id: p.id, kind: "ban".into(), text: "Talajvíz 2 m-en belül – temetési tilalom".into(), ..Default::default() },
        )?;
    }

    // burials in chronological order
    let mut burials: Vec<(NaiveDate, String, &str)> = vec![];
    let mut push_section = |code: &str, rows: i32, per: i32, fill: f64, from: i32, to: i32, ty: &'static str, r: &mut StdRng| {
        for row in 1..=rows {
            for n in 1..=per {
                if r.gen_bool(fill) {
                    let y = r.gen_range(from..=to);
                    let d = NaiveDate::from_ymd_opt(y, r.gen_range(1..=12), r.gen_range(1..=28)).unwrap();
                    burials.push((d, format!("{code}/{row}/{n}"), ty));
                }
            }
        }
    };
    push_section("A", 6, 16, 0.8, 1962, 2004, "coffin_burial", &mut r);
    push_section("B", 4, 10, 0.6, 1985, 2020, "coffin_burial", &mut r);
    push_section("C", 5, 14, 0.45, 2005, 2026, "coffin_burial", &mut r);
    push_section("U", 3, 12, 0.5, 2003, 2026, "urn_placement", &mut r);
    push_section("K", 1, 5, 0.6, 1990, 2024, "coffin_burial", &mut r);
    burials.sort();
    let limit = today();
    let mut events = 0;
    for (date, ident, ty) in burials {
        if date > limit {
            continue;
        }
        let Some(p) = store.find_plot(&cem.id, &ident)? else { continue };
        let container = if ty == "urn_placement" { "urn" } else { "coffin" };
        let input = BurialInput {
            event_type: ty.into(),
            event_date: fmt_date(date),
            container: container.into(),
            deceased: DeceasedRef::New { deceased: random_deceased(&mut r, date) },
            plot_id: Some(p.id.clone()),
            crypt_slot_id: None,
            arranger: Some(PersonRef::New { person: random_person(&mut r) }),
            holder: None,
            holder_relation: Some(pick(&mut r, &["fia", "lánya", "házastársa", "unokája", "testvére"]).into()),
            lease: LeaseChoice { create: true, ..Default::default() },
            historical_folio: None,
            override_reason: None,
            notes: String::new(),
        };
        if store.record_burial(s, &input).is_ok() {
            events += 1;
        }
        // occasional reburial of an urn into the same grave
        if ty == "coffin_burial" && r.gen_bool(0.12) {
            let d2 = date + Duration::days(r.gen_range(400..9000));
            if d2 <= limit {
                let input = BurialInput {
                    event_type: "urn_placement".into(),
                    event_date: fmt_date(d2),
                    container: "urn".into(),
                    deceased: DeceasedRef::New { deceased: random_deceased(&mut r, d2) },
                    plot_id: Some(p.id.clone()),
                    crypt_slot_id: None,
                    arranger: None,
                    holder: None,
                    holder_relation: None,
                    lease: LeaseChoice { create: true, ..Default::default() },
                    historical_folio: None,
                    override_reason: None,
                    notes: String::new(),
                };
                if store.record_burial(s, &input).is_ok() {
                    events += 1;
                }
            }
        }
    }
    // most families renew in time (keeps the sample realistic)
    let expired_now: Vec<(String, String)> = store
        .expiries(s, &ExpiryFilter { cemetery_id: Some(cem.id.clone()), within_days: 0, include_expired: true, ..Default::default() })?
        .into_iter()
        .map(|e| (e.plot_id, e.lease_expiry))
        .collect();
    for (plot_id, expiry) in expired_now {
        if !r.gen_bool(0.75) {
            continue;
        }
        let mut start = expiry;
        loop {
            match store.create_lease(s, &NewLeaseInput { plot_id: plot_id.clone(), kind: "renewal".into(), start_date: start.clone(), ..Default::default() }) {
                Ok(l) if l.expiry_date.as_str() < fmt_date(limit).as_str() => start = l.expiry_date,
                _ => break,
            }
        }
    }
    // a national grave (FR-MEG-08, FR-EML-03)
    if let Some(mut p) = store.find_plot(&cem.id, "A/1/1")? {
        p.protected = true;
        p.protection_kind = Some("national".into());
        p.protection_reason = Some("Nemzeti Sírkert – a falu első tanítójának sírja (kitalált)".into());
        p.further_burial_allowed = false;
        store.update_plot(s, &p, Some("mintaadat"))?;
    }
    // a few pre-purchases and a notice
    for ident in ["C/1/14", "C/2/14"] {
        if let Some(p) = store.find_plot(&cem.id, ident)? {
            let _ = store.create_lease(
                s,
                &NewLeaseInput {
                    plot_id: p.id,
                    person: Some(PersonRef::New { person: random_person(&mut r) }),
                    kind: "prepurchase".into(),
                    ..Default::default()
                },
            );
        }
    }
    let expired: Vec<String> = store
        .expiries(s, &ExpiryFilter { cemetery_id: Some(cem.id.clone()), within_days: 0, include_expired: true, ..Default::default() })?
        .into_iter()
        .take(6)
        .map(|e| e.plot_id)
        .collect();
    if !expired.is_empty() {
        store.record_notices(s, &expired, "letter", &fmt_date(limit - Duration::days(200)), "kézbesítve")?;
    }
    Ok(SampleSummary { cemetery_id: cem.id, plots, events })
}

/// Fast bulk generator for performance tests: inserts directly in one
/// transaction (bypassing per-row audit) – never used on real databases.
pub fn generate_large(store: &Store, plots: usize, deceased: usize) -> Result<String> {
    let mut r = StdRng::seed_from_u64(42);
    let tx = crate::db::tx(&store.conn)?;
    let ts = now_ts();
    let cem = new_id();
    tx.execute("INSERT INTO cemeteries (id, name, created_at, updated_at) VALUES (?1, 'Teljesítményteszt temető', ?2, ?2)", params![cem, ts])?;
    let single: String = tx.query_row("SELECT id FROM plot_types WHERE code='single'", [], |r| r.get(0))?;
    let per_row = 50usize;
    let rows_per_section = 20usize;
    let mut plot_ids = Vec::with_capacity(plots);
    {
        let mut ins_sec = tx.prepare(
            "INSERT INTO sections (id, cemetery_id, code, name, map_x, map_y, map_w, map_h, created_at, updated_at) VALUES (?1,?2,?3,'',?4,?5,?6,?7,?8,?8)",
        )?;
        let mut ins_row = tx.prepare("INSERT INTO plot_rows (id, section_id, number, created_at, updated_at) VALUES (?1,?2,?3,?4,?4)")?;
        let mut ins_plot = tx.prepare(
            "INSERT INTO plots (id, cemetery_id, section_id, row_id, number, identifier, plot_type_id, coffin_capacity, urn_capacity, map_x, map_y, map_w, map_h, created_at, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,1,4,?8,?9,10,20,?10,?10)",
        )?;
        let sections = plots.div_ceil(per_row * rows_per_section);
        let mut made = 0;
        for si in 0..sections {
            let sid = new_id();
            let code = format!("P{si}");
            let sx = (si % 10) as f64 * (per_row as f64 * 15.0 + 40.0);
            let sy = (si / 10) as f64 * (rows_per_section as f64 * 28.0 + 40.0);
            ins_sec.execute(params![sid, cem, code, sx, sy, per_row as f64 * 15.0, rows_per_section as f64 * 28.0, ts])?;
            for ri in 1..=rows_per_section {
                if made >= plots {
                    break;
                }
                let rid = new_id();
                ins_row.execute(params![rid, sid, ri.to_string(), ts])?;
                for pi in 1..=per_row {
                    if made >= plots {
                        break;
                    }
                    let pid = new_id();
                    ins_plot.execute(params![
                        pid,
                        cem,
                        sid,
                        rid,
                        pi.to_string(),
                        format!("{code}/{ri}/{pi}"),
                        single,
                        sx + pi as f64 * 15.0,
                        sy + ri as f64 * 28.0,
                        ts
                    ])?;
                    plot_ids.push(pid);
                    made += 1;
                }
            }
        }
        let mut ins_d = tx.prepare(
            "INSERT INTO deceased (id, family_name, given_name, birth_date, death_date, mother_name, last_residence, search_text, created_at, updated_at, sort_key)
             VALUES (?1,?2,?3,?4,?5,'Minta Anna','Mintavölgy',?6,?7,?7,?8)",
        )?;
        let mut ins_fts = tx.prepare("INSERT INTO search_index (kind, ref_id, text) VALUES ('deceased', ?1, ?2)")?;
        let mut ins_e = tx.prepare(
            "INSERT INTO interment_events (id, cemetery_id, scope_key, folio, event_type, container, event_date, deceased_id, plot_id, created_at, updated_at)
             VALUES (?1,?2,'all',?3,'coffin_burial','coffin',?4,?5,?6,?7,?7)",
        )?;
        let mut ins_l = tx.prepare(
            "INSERT INTO leases (id, plot_id, kind, start_date, duration_years, expiry_date, status, created_at, updated_at)
             VALUES (?1,?2,'purchase',?3,25,?4,'active',?5,?5) ON CONFLICT DO NOTHING",
        )?;
        for i in 0..deceased {
            let did = new_id();
            let fam = pick(&mut r, FAMILY);
            let giv = if r.gen_bool(0.5) { pick(&mut r, MALE) } else { pick(&mut r, FEMALE) };
            let y = r.gen_range(1950..2026);
            let death = NaiveDate::from_ymd_opt(y, r.gen_range(1..=12), r.gen_range(1..=28)).unwrap();
            let birth = NaiveDate::from_ymd_opt(y - r.gen_range(40..95), 1, 1).unwrap();
            let text = normalize(&format!("{fam} {giv}"));
            ins_d.execute(params![did, fam, giv, fmt_date(birth), fmt_date(death), text, ts, crate::text::sort_key(&format!("{fam} {giv}"))])?;
            ins_fts.execute(params![did, text])?;
            let pid = &plot_ids[i % plot_ids.len()];
            ins_e.execute(params![new_id(), cem, (i + 1) as i64, fmt_date(death), did, pid, ts])?;
            ins_l.execute(params![new_id(), pid, fmt_date(death), fmt_date(crate::rules::add_years(death, 25)), ts])?;
        }
        tx.execute("INSERT INTO register_sequences (cemetery_id, scope_key, next_value) VALUES (?1, 'all', ?2)", params![cem, deceased as i64 + 1])?;
    }
    tx.commit()?;
    crate::db::analyze(&store.conn)?;
    Ok(cem)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_loads_consistently() {
        let store = Store::in_memory().unwrap();
        let s = Session::system();
        let sum = load_sample(&store, &s).unwrap();
        assert!(sum.plots >= 249, "{}", sum.plots);
        assert!(sum.events > 100, "{}", sum.events);
        assert!(crate::audit::verify_chain(&store.conn).unwrap().ok);
        let page = store.list_events(&EventFilter { cemetery_id: Some(sum.cemetery_id.clone()), ..Default::default() }).unwrap();
        let folios: Vec<i64> = page.items.iter().map(|e| e.folio).collect();
        assert_eq!(folios, (1..=folios.len() as i64).collect::<Vec<_>>());
    }
}
