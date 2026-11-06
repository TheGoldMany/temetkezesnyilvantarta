//! Integration tests: register book, folio, burial rules, leases, expiry (UC-01..03).

mod common;

use common::*;
use lunda_core::audit::verify_chain;
use lunda_core::auth::Role;
use lunda_core::model::*;
use lunda_core::rules::PlotStatus;
use lunda_core::util::{fmt_date, today};

#[test]
fn folio_is_continuous_gap_free_and_immutable() {
    let fx = fixture();
    for i in 1..=10 {
        let r = fx.burial(&format!("A/1/{i}"), "coffin_burial", "2020-03-01", "Teszt", &format!("Elek{i}")).unwrap();
        assert_eq!(r.folio, i as i64);
    }
    // a refused burial must not consume a folio
    let err = fx.burial("A/1/1", "coffin_burial", "2021-01-01", "Teszt", "Tiltott").unwrap_err();
    assert_eq!(err.code, "burial_not_allowed");
    let r = fx.burial("A/2/1", "coffin_burial", "2021-01-01", "Teszt", "Tizenegy").unwrap();
    assert_eq!(r.folio, 11);

    // voiding keeps the folio
    fx.store.void_event(&fx.admin, &r.event.id, Some("téves rögzítés")).unwrap();
    let r2 = fx.burial("A/2/1", "coffin_burial", "2021-01-02", "Teszt", "Tizenkettő").unwrap();
    assert_eq!(r2.folio, 12);

    let all = fx.store.list_events(&EventFilter { include_voided: true, ..Default::default() }).unwrap();
    let folios: Vec<i64> = all.items.iter().map(|e| e.folio).collect();
    assert_eq!(folios, (1..=12).collect::<Vec<_>>());

    // database refuses to change or delete
    assert!(fx.store.conn.execute("UPDATE interment_events SET folio=99 WHERE folio=3", []).is_err());
    assert!(fx.store.conn.execute("DELETE FROM interment_events WHERE folio=3", []).is_err());
    // voided entries are final
    assert!(fx.store.conn.execute("UPDATE interment_events SET voided_at=NULL WHERE folio=11", []).is_err());
    assert!(verify_chain(&fx.store.conn).unwrap().ok);
}

#[test]
fn historical_folio_advances_counter_and_rejects_duplicates() {
    let fx = fixture();
    let p = fx.plot("A/1/1");
    let mk = |folio: Option<i64>, plot: &str| BurialInput {
        event_type: "coffin_burial".into(),
        event_date: "1985-06-01".into(),
        container: "coffin".into(),
        deceased: DeceasedRef::New { deceased: Fx::deceased("Régi", "Rozália", "1985-05-28") },
        plot_id: Some(fx.plot(plot).id),
        crypt_slot_id: None,
        arranger: Some(PersonRef::New { person: Fx::person("Régi", "Rudolf") }),
        holder: None,
        holder_relation: None,
        lease: LeaseChoice::default(),
        historical_folio: folio,
        override_reason: None,
        notes: String::new(),
        cemetery_id: None,
        receiver_name: String::new(),
        receiver_address: String::new(),
        destination: String::new(),
    };
    let _ = p;
    let r = fx.store.record_burial(&fx.admin, &mk(Some(4500), "A/1/1")).unwrap();
    assert_eq!(r.folio, 4500);
    assert!(r.event.historical);
    let dup = fx.store.record_burial(&fx.admin, &mk(Some(4500), "A/1/2")).unwrap_err();
    assert_eq!(dup.code, "folio_taken");
    let next = fx.store.record_burial(&fx.admin, &mk(None, "A/1/3")).unwrap();
    assert_eq!(next.folio, 4501);
}

#[test]
fn yearly_folio_scope_restarts_each_year() {
    let fx = fixture();
    let mut c = fx.cemetery.clone();
    c.folio_scope = "yearly".into();
    fx.store.save_cemetery(&fx.admin, &c).unwrap();
    assert_eq!(fx.burial("A/1/1", "coffin_burial", "2024-02-01", "Év", "Egy").unwrap().folio, 1);
    assert_eq!(fx.burial("A/1/2", "coffin_burial", "2024-03-01", "Év", "Kettő").unwrap().folio, 2);
    assert_eq!(fx.burial("A/1/3", "coffin_burial", "2025-01-10", "Év", "Három").unwrap().folio, 1);
    // scope cannot change once there are events
    c.folio_scope = "continuous".into();
    assert_eq!(fx.store.save_cemetery(&fx.admin, &c).unwrap_err().code, "folio_scope_locked");
}

#[test]
fn uc01_new_burial_creates_holder_lease_and_audit() {
    let fx = fixture();
    let plot = fx.plot("A/1/5");
    let preview = fx.store.preview_burial(&plot.id, "2026-01-10", "coffin").unwrap();
    assert!(preview.verdict.allowed);
    assert!(!preview.has_primary_holder);
    let lp = preview.lease.unwrap();
    assert_eq!(lp.kind, "purchase");
    assert_eq!(lp.expiry_date, "2051-01-10");
    assert_eq!(lp.fee_huf, 100_000);

    let r = fx.burial("A/1/5", "coffin_burial", "2026-01-10", "Kovács", "János").unwrap();
    let lease = r.lease.unwrap();
    assert_eq!(lease.expiry_date, "2051-01-10");
    assert_eq!(lease.fee_huf, 100_000);
    let detail = fx.store.plot_detail(&fx.admin, &plot.id).unwrap();
    assert_eq!(detail.status, PlotStatus::Occupied);
    assert_eq!(detail.occupants.len(), 1);
    assert_eq!(detail.holders.len(), 1);
    assert_eq!(detail.holders[0].role, "primary");
    assert_eq!(detail.holders[0].person_name, "Eltemettető Éva");
    let log = lunda_core::audit::list(&fx.store.conn, Some("event"), Some(&r.event.id), 10, None).unwrap();
    assert_eq!(log.len(), 1);
    assert!(verify_chain(&fx.store.conn).unwrap().ok);
}

#[test]
fn uc01_mandatory_register_columns_are_validated() {
    let fx = fixture();
    let mut d = Fx::deceased("Hiányos", "Hugó", "2026-01-01");
    d.mother_name = String::new();
    let input = BurialInput {
        event_type: "coffin_burial".into(),
        event_date: "2026-01-05".into(),
        container: "coffin".into(),
        deceased: DeceasedRef::New { deceased: d },
        plot_id: Some(fx.plot("A/1/1").id),
        crypt_slot_id: None,
        arranger: Some(PersonRef::New { person: Fx::person("X", "Y") }),
        holder: None,
        holder_relation: None,
        lease: LeaseChoice::default(),
        historical_folio: None,
        override_reason: None,
        notes: String::new(),
        cemetery_id: None,
        receiver_name: String::new(),
        receiver_address: String::new(),
        destination: String::new(),
    };
    let e = fx.store.record_burial(&fx.admin, &input).unwrap_err();
    assert_eq!(e.code, "validation");
    assert_eq!(e.params["field"], "motherName");
}

#[test]
fn unknown_identity_requires_authority_and_case_number() {
    let fx = fixture();
    let mut d = Deceased { unknown_identity: true, death_date: Some("2026-01-01".into()), ..Default::default() };
    let mk = |d: Deceased| BurialInput {
        event_type: "coffin_burial".into(),
        event_date: "2026-01-05".into(),
        container: "coffin".into(),
        deceased: DeceasedRef::New { deceased: d },
        plot_id: Some(fx.plot("A/1/1").id),
        crypt_slot_id: None,
        arranger: Some(PersonRef::New { person: Fx::person("Önkormányzat", "") }),
        holder: None,
        holder_relation: None,
        lease: LeaseChoice::default(),
        historical_folio: None,
        override_reason: None,
        notes: String::new(),
        cemetery_id: None,
        receiver_name: String::new(),
        receiver_address: String::new(),
        destination: String::new(),
    };
    assert_eq!(fx.store.record_burial(&fx.admin, &mk(d.clone())).unwrap_err().params["field"], "authorityName");
    d.authority_name = "Kecskeméti Rendőrkapitányság".into();
    d.case_number = "12345/2026".into();
    assert!(fx.store.record_burial(&fx.admin, &mk(d)).is_ok());
}

#[test]
fn uc02_reburial_rules_and_extension() {
    let fx = fixture();
    fx.burial("A/1/1", "coffin_burial", "2015-04-01", "Nagy", "Ferenc").unwrap();
    let plot = fx.plot("A/1/1");

    // second coffin into a single grave within the rest period: blocked with reason
    let p = fx.store.preview_burial(&plot.id, "2026-01-10", "coffin").unwrap();
    assert!(!p.verdict.allowed);
    assert_eq!(p.verdict.blockers[0].code, "rest_period_not_elapsed");
    assert_eq!(p.verdict.blockers[0].params["until"], "2040-04-01");

    // an urn may be placed; lease is extended to burial + rest period (urn rest period = 0 → no extension)
    let p = fx.store.preview_burial(&plot.id, "2026-01-10", "urn").unwrap();
    assert!(p.verdict.allowed, "{:?}", p.verdict);
    assert!(p.has_primary_holder);
    assert!(p.lease.is_none());
    let r = fx.burial("A/1/1", "urn_placement", "2026-01-10", "Nagy", "Ferencné").unwrap();
    assert!(r.lease.is_none());

    // override without permission is refused; with permission and reason it is recorded
    let clerk = session(Role::Clerk);
    let mut input = BurialInput {
        event_type: "reburial".into(),
        event_date: "2026-02-01".into(),
        container: "coffin".into(),
        deceased: DeceasedRef::New { deceased: Fx::deceased("Nagy", "Ilona", "2026-01-28") },
        plot_id: Some(plot.id.clone()),
        crypt_slot_id: None,
        arranger: None,
        holder: None,
        holder_relation: None,
        lease: LeaseChoice { create: true, ..Default::default() },
        historical_folio: None,
        override_reason: Some("mélyített sír, hatósági engedély 12/2026".into()),
        notes: String::new(),
        cemetery_id: None,
        receiver_name: String::new(),
        receiver_address: String::new(),
        destination: String::new(),
    };
    assert_eq!(fx.store.record_burial(&clerk, &input).unwrap_err().code, "burial_not_allowed");
    let r = fx.store.record_burial(&fx.admin, &input).unwrap();
    assert!(r.event.override_reason.is_some());
    // the lease is extended to 2026-02-01 + 25 years with a pro-rata renewal fee
    let l = r.lease.unwrap();
    assert_eq!(l.kind, "renewal");
    assert_eq!(l.expiry_date, "2051-02-01");
    assert!(l.fee_huf > 0 && l.fee_huf < 80_000, "{}", l.fee_huf);
    input.override_reason = None;
    let _ = input;

    // only one active lease remains
    let leases = fx.store.leases_for_plot(&plot.id).unwrap();
    assert_eq!(leases.iter().filter(|l| l.status == "active").count(), 1);
}

#[test]
fn reburial_after_rest_period_extends_lease() {
    let fx = fixture();
    fx.burial("A/2/2", "coffin_burial", "1995-04-01", "Régi", "Lajos").unwrap();
    let plot = fx.plot("A/2/2");
    let p = fx.store.preview_burial(&plot.id, "2026-01-10", "coffin").unwrap();
    assert!(p.verdict.allowed);
    assert_eq!(p.verdict.warnings.iter().filter(|w| w.code == "lease_expired").count(), 1);
    let lp = p.lease.unwrap();
    assert_eq!(lp.kind, "renewal");
    assert_eq!(lp.expiry_date, "2051-01-10");
    assert_eq!(lp.fee_huf, 80_000); // full period: renewal rate
}

#[test]
fn restriction_ban_blocks_burial() {
    let fx = fixture();
    let p = fx.plot("A/3/3");
    fx.store
        .add_restriction(&fx.admin, &Restriction { plot_id: p.id.clone(), kind: "ban".into(), text: "talajvíz 2 m-en belül".into(), ..Default::default() })
        .unwrap();
    let e = fx.burial("A/3/3", "coffin_burial", "2026-01-01", "Víz", "Viktor").unwrap_err();
    assert_eq!(e.code, "burial_not_allowed");
    assert!(e.params["reasons"].contains("burial_ban"));
}

#[test]
fn permissions_are_enforced() {
    let fx = fixture();
    let reader = session(Role::Reader);
    let finance = session(Role::Finance);
    let p = fx.plot("A/1/1");
    let input = BurialInput {
        event_type: "coffin_burial".into(),
        event_date: "2026-01-05".into(),
        container: "coffin".into(),
        deceased: DeceasedRef::New { deceased: Fx::deceased("Jog", "Jenő", "2026-01-01") },
        plot_id: Some(p.id.clone()),
        crypt_slot_id: None,
        arranger: Some(PersonRef::New { person: Fx::person("X", "Y") }),
        holder: None,
        holder_relation: None,
        lease: LeaseChoice::default(),
        historical_folio: None,
        override_reason: None,
        notes: String::new(),
        cemetery_id: None,
        receiver_name: String::new(),
        receiver_address: String::new(),
        destination: String::new(),
    };
    assert_eq!(fx.store.record_burial(&reader, &input).unwrap_err().code, "forbidden");
    assert_eq!(fx.store.record_burial(&finance, &input).unwrap_err().code, "forbidden");
    let r = fx.store.record_burial(&session(Role::Clerk), &input).unwrap();
    // clerk cannot void; correction needs correct_entries
    assert_eq!(fx.store.void_event(&session(Role::Clerk), &r.event.id, Some("x y z")).unwrap_err().code, "forbidden");
    // reader sees no personal data of holders
    let d = fx.store.plot_detail(&reader, &p.id).unwrap();
    assert!(d.holders[0].person_address.is_empty());
    let d = fx.store.plot_detail(&fx.admin, &p.id).unwrap();
    assert!(!d.holders[0].person_address.is_empty());
}

#[test]
fn correction_requires_reason_and_keeps_original_in_log() {
    let fx = fixture();
    let r = fx.burial("A/1/1", "coffin_burial", "2026-01-05", "Hibás", "Név").unwrap();
    let mut d = fx.store.get_deceased(&r.event.deceased_id).unwrap();
    d.given_name = "Helyes".into();
    assert_eq!(fx.store.update_deceased(&fx.admin, &d, None).unwrap_err().code, "reason_required");
    assert_eq!(fx.store.update_deceased(&session(Role::Clerk), &d, Some("elírás")).unwrap_err().code, "forbidden");
    fx.store.update_deceased(&fx.admin, &d, Some("anyakönyvi kivonat alapján")).unwrap();
    let log = lunda_core::audit::list(&fx.store.conn, Some("deceased"), Some(&d.id), 10, None).unwrap();
    assert_eq!(log[0].action, "correct");
    assert!(log[0].old_value.as_ref().unwrap().contains("\"Név\""));
    assert_eq!(log[0].reason.as_deref(), Some("anyakönyvi kivonat alapján"));

    let mut ev = fx.store.get_event(&r.event.id).unwrap();
    ev.event_date = "2026-01-06".into();
    ev.folio = 999; // ignored – folio cannot change
    let ev2 = fx.store.correct_event(&fx.admin, &ev, Some("dátum javítása")).unwrap();
    assert_eq!(ev2.folio, r.folio);
    assert_eq!(ev2.event_date, "2026-01-06");
}

#[test]
fn urn_release_and_relocation() {
    let fx = fixture();
    let r = fx.burial("A/1/4", "urn_placement", "2024-01-01", "Urna", "Ubul").unwrap();
    let plot = fx.plot("A/1/4");
    let mut input = RemovalInput {
        event_type: "urn_release".into(),
        event_date: "2026-01-02".into(),
        source_event_id: r.event.id.clone(),
        receiver_name: String::new(),
        receiver_address: String::new(),
        destination: "családi ház kertje".into(),
        target_plot_id: None,
        historical_folio: None,
        notes: String::new(),
        holder_consent: false,
        permit_number: String::new(),
    };
    assert_eq!(fx.store.record_removal(&fx.admin, &input).unwrap_err().params["field"], "receiverName");
    input.receiver_name = "Urna Ubulné".into();
    let rel = fx.store.record_removal(&fx.admin, &input).unwrap();
    assert_eq!(rel.folio, r.folio + 1);
    assert!(fx.store.occupants(&plot.id).unwrap().is_empty());
    // cannot release twice
    assert_eq!(fx.store.record_removal(&fx.admin, &input).unwrap_err().code, "not_active_placement");

    // relocation of a coffin into another plot: the holder's consent (35. §)
    let c = fx.burial("A/2/5", "coffin_burial", "2000-01-01", "Költöző", "Kálmán").unwrap();
    let target = fx.plot("A/3/9");
    let mut reloc = RemovalInput {
        event_type: "relocation".into(),
        event_date: "2026-01-03".into(),
        source_event_id: c.event.id.clone(),
        receiver_name: String::new(),
        receiver_address: String::new(),
        destination: String::new(),
        target_plot_id: Some(target.id.clone()),
        historical_folio: None,
        notes: String::new(),
        holder_consent: false,
        permit_number: String::new(),
    };
    assert_eq!(fx.store.record_removal(&fx.admin, &reloc).unwrap_err().params["code"], "consent_required");
    reloc.holder_consent = true;
    let mv = fx.store.record_removal(&fx.admin, &reloc).unwrap();
    assert!(mv.notes.contains("hozzájárult"));
    // within 15 years of the burial a coffin needs the public health permit (36. §)
    let young = fx.burial("A/2/6", "coffin_burial", "2020-01-01", "Fiatal", "Ferenc").unwrap();
    let mut early =
        RemovalInput { source_event_id: young.event.id.clone(), target_plot_id: None, destination: "Kisvölgyi köztemető".into(), ..reloc.clone() };
    assert_eq!(fx.store.record_removal(&fx.admin, &early).unwrap_err().params["code"], "permit_required");
    early.permit_number = "BK-NEF/123-2/2026".into();
    let ex = fx.store.record_removal(&fx.admin, &early).unwrap();
    assert!(ex.notes.contains("BK-NEF/123-2/2026"));
    assert_eq!(mv.plot_id.as_deref(), Some(target.id.as_str()));
    assert!(fx.store.occupants(&fx.plot("A/2/5").id).unwrap().is_empty());
    assert_eq!(fx.store.occupants(&target.id).unwrap().len(), 1);
}

#[test]
fn crypt_book_slots() {
    let fx = fixture();
    let crypt = fx.store.get_plot_type("crypt").unwrap();
    fx.store
        .bulk_create(
            &fx.admin,
            &BulkCreateInput {
                cemetery_id: fx.cemetery.id.clone(),
                section_code: "K".into(),
                row_from: 1,
                row_to: 1,
                plots_per_row: 2,
                first_plot_number: 1,
                plot_type_id: crypt.id.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    let p = fx.plot("K/1/1");
    assert_eq!(fx.store.crypt_slots(&p.id).unwrap().len(), 4);
    for (i, name) in ["Első", "Második", "Harmadik", "Negyedik"].iter().enumerate() {
        let r = fx.burial("K/1/1", "coffin_burial", &format!("202{}-01-01", i), "Kripta", name).unwrap();
        assert!(r.event.crypt_slot_id.is_some());
    }
    let slots = fx.store.crypt_slots(&p.id).unwrap();
    assert!(slots.iter().all(|s| s.occupant_name.is_some()));
    assert_eq!(slots[0].occupant_name.as_deref(), Some("Kripta Első"));
    // fifth coffin: full and within rest period
    assert_eq!(fx.burial("K/1/1", "coffin_burial", "2026-01-01", "Kripta", "Ötödik").unwrap_err().code, "burial_not_allowed");
    // crypt book lists only crypt events
    let book = fx.store.list_events(&EventFilter { crypts_only: true, ..Default::default() }).unwrap();
    assert_eq!(book.total, 4);
}

#[test]
fn holder_change_keeps_history() {
    let fx = fixture();
    fx.burial("A/1/1", "coffin_burial", "2020-01-01", "Öröklés", "Ödön").unwrap();
    let p = fx.plot("A/1/1");
    let hs = fx
        .store
        .change_holder(
            &fx.admin,
            &HolderChangeInput {
                plot_id: p.id.clone(),
                person: Some(PersonRef::New { person: Fx::person("Örökös", "Ottó") }),
                role: "primary".into(),
                relation: "fia".into(),
                change_date: "2025-05-05".into(),
                legal_basis: "öröklés".into(),
                document_ref: "hagyatékátadó végzés 1/2025".into(),
            },
        )
        .unwrap();
    assert_eq!(hs.len(), 2);
    let current: Vec<_> = hs.iter().filter(|h| h.end_date.is_none()).collect();
    assert_eq!(current.len(), 1);
    assert_eq!(current[0].person_name, "Örökös Ottó");
    let old = hs.iter().find(|h| h.end_date.is_some()).unwrap();
    assert_eq!(old.end_date.as_deref(), Some("2025-05-05"));
    // the lease follows the holder
    let l = fx.store.leases_for_plot(&p.id).unwrap().into_iter().find(|l| l.status == "active").unwrap();
    assert_eq!(l.person_name.as_deref(), Some("Örökös Ottó"));
}

#[test]
fn legal_minimum_use_period_is_enforced() {
    let fx = fixture();
    let p = fx.plot("A/1/9");
    let person = PersonRef::New { person: Fx::person("Rövid", "Rezső") };
    let lease = |years: i32| {
        fx.store.create_lease(
            &fx.admin,
            &NewLeaseInput {
                plot_id: p.id.clone(),
                kind: "prepurchase".into(),
                person: Some(person.clone()),
                duration_years: Some(years),
                ..Default::default()
            },
        )
    };
    let e = lease(20).unwrap_err();
    assert_eq!(e.code, "lease_too_short");
    assert_eq!(e.params["min"], "25");
    lease(25).unwrap();
}

#[test]
fn uc03_expiry_notice_and_resale() {
    let fx = fixture();
    fx.store.save_cemetery(&fx.admin, &fx.cemetery).unwrap();
    lunda_core::settings::set_many(&fx.store.conn, &fx.admin, None, &[("rules.grace_period_days".to_string(), "30".to_string())].into_iter().collect())
        .unwrap();
    // burial in 1995 -> lease expired in 2020
    fx.burial("A/1/7", "coffin_burial", "1995-03-01", "Lejárt", "Lenke").unwrap();
    // lease expiring soon
    let soon = today() + chrono::Duration::days(40);
    fx.store.conn.execute("UPDATE leases SET expiry_date=?1 WHERE plot_id=(SELECT id FROM plots WHERE identifier='A/1/8') ", [fmt_date(soon)]).unwrap();
    fx.burial("A/1/8", "coffin_burial", "2001-03-01", "Hamarosan", "Hédi").unwrap();
    fx.store.conn.execute("UPDATE leases SET expiry_date=?1 WHERE plot_id=(SELECT id FROM plots WHERE identifier='A/1/8')", [fmt_date(soon)]).unwrap();

    let list = fx.store.expiries(&fx.admin, &ExpiryFilter { within_days: 90, include_expired: true, ..Default::default() }).unwrap();
    let idents: Vec<_> = list.iter().map(|r| r.identifier.as_str()).collect();
    assert_eq!(idents, vec!["A/1/7", "A/1/8"]);
    assert_eq!(list[0].status, PlotStatus::Expired);
    assert!(list[0].days_left < 0);
    assert_eq!(list[0].holder_name.as_deref(), Some("Eltemettető Éva"));

    let only_future = fx.store.expiries(&fx.admin, &ExpiryFilter { within_days: 90, include_expired: false, ..Default::default() }).unwrap();
    assert_eq!(only_future.len(), 1);

    // resale before notice is refused
    let p7 = fx.plot("A/1/7");
    assert_eq!(fx.store.mark_resellable(&fx.admin, &p7.id, Some("türelmi idő lejárt")).unwrap_err().code, "not_notified");

    // notices sent 60 days ago
    let notice_date = fmt_date(today() - chrono::Duration::days(60));
    assert_eq!(fx.store.record_notices(&fx.admin, std::slice::from_ref(&p7.id), "letter", &notice_date, "kézbesítve").unwrap(), 1);
    assert_eq!(fx.store.plot_status(&fx.plot("A/1/7")).unwrap(), PlotStatus::Notified);
    let list = fx.store.expiries(&fx.admin, &ExpiryFilter { within_days: 90, include_expired: true, only_not_notified: true, ..Default::default() }).unwrap();
    assert_eq!(list.len(), 1);

    // reason (confirmation) required
    assert_eq!(fx.store.mark_resellable(&fx.admin, &p7.id, None).unwrap_err().code, "reason_required");
    // 145/1999. Korm. r. 18. § (5a): a public notice a month before the reuse
    assert_eq!(fx.store.mark_resellable(&fx.admin, &p7.id, Some("türelmi idő lejárt, megerősítve")).unwrap_err().code, "public_notice_required");
    let board = fmt_date(today() - chrono::Duration::days(10));
    fx.store.record_notices(&fx.admin, std::slice::from_ref(&p7.id), "board", &board, "hirdetmény a temetőben és a honlapon").unwrap();
    assert_eq!(fx.store.mark_resellable(&fx.admin, &p7.id, Some("türelmi idő lejárt, megerősítve")).unwrap_err().code, "public_notice_running");
    fx.store
        .conn
        .execute(
            "UPDATE notices SET notice_date = ?2 WHERE plot_id = ?1 AND kind = 'board'",
            lunda_core::params![p7.id, fmt_date(today() - chrono::Duration::days(31))],
        )
        .unwrap();
    assert_eq!(fx.store.mark_resellable(&fx.admin, &p7.id, Some("türelmi idő lejárt, megerősítve")).unwrap(), PlotStatus::Resellable);
    // burial into resellable plot is blocked
    assert_eq!(fx.burial("A/1/7", "coffin_burial", &fmt_date(today()), "Új", "Ügyfél").unwrap_err().code, "burial_not_allowed");
    // resale: new purchase by a new holder
    let l = fx
        .store
        .create_lease(
            &fx.admin,
            &NewLeaseInput {
                plot_id: p7.id.clone(),
                person: Some(PersonRef::New { person: Fx::person("Vevő", "Vilma") }),
                kind: "purchase".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(l.person_name.as_deref(), Some("Vevő Vilma"));
    let st = fx.store.plot_status(&fx.plot("A/1/7")).unwrap();
    assert_eq!(st, PlotStatus::Occupied); // old remains are still recorded as occupants
    let holders = fx.store.holders_for_plot(&p7.id).unwrap();
    assert_eq!(holders.iter().filter(|h| h.end_date.is_none()).count(), 1);
}

#[test]
fn prepurchase_and_renewal_and_cancel() {
    let fx = fixture();
    let p = fx.plot("A/3/1");
    let l = fx
        .store
        .create_lease(
            &fx.admin,
            &NewLeaseInput {
                plot_id: p.id.clone(),
                person: Some(PersonRef::New { person: Fx::person("Előre", "Erzsébet") }),
                kind: "prepurchase".into(),
                start_date: "2020-01-01".into(),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(l.expiry_date, "2045-01-01");
    assert_eq!(fx.store.plot_status(&p).unwrap(), PlotStatus::Reserved);
    // a reserved plot cannot be purchased again
    let again = fx.store.create_lease(
        &fx.admin,
        &NewLeaseInput {
            plot_id: p.id.clone(),
            person: Some(PersonRef::New { person: Fx::person("Más", "Valaki") }),
            kind: "purchase".into(),
            ..Default::default()
        },
    );
    assert_eq!(again.unwrap_err().code, "plot_not_free");
    // renewal starts at the old expiry
    let r = fx.store.create_lease(&fx.admin, &NewLeaseInput { plot_id: p.id.clone(), kind: "renewal".into(), ..Default::default() }).unwrap();
    assert_eq!(r.start_date, "2045-01-01");
    assert_eq!(r.expiry_date, "2070-01-01");
    assert_eq!(r.fee_huf, 80_000);
    // cancel: pro-rata refund of the renewal fee
    let c = fx.store.cancel_lease(&fx.admin, &r.id, "2057-07-02", Some("lemondás")).unwrap();
    assert_eq!(c.status, "cancelled");
    let refund = c.refund_huf.unwrap();
    assert!((39_000..=41_000).contains(&refund), "{refund}");
}

#[test]
fn national_grave_is_fee_exempt() {
    let fx = fixture();
    let mut p = fx.plot("A/2/9");
    p.protected = true;
    p.protection_kind = Some("national".into());
    p.protection_reason = Some("Nemzeti Sírkert".into());
    assert_eq!(fx.store.update_plot(&fx.admin, &p, None).unwrap_err().code, "reason_required");
    fx.store.update_plot(&fx.admin, &p, Some("NÖB határozat")).unwrap();
    let r = fx.burial("A/2/9", "coffin_burial", "2026-01-05", "Híres", "Hős").unwrap();
    let l = r.lease.unwrap();
    assert!(l.fee_exempt);
    assert_eq!(l.fee_huf, 0);
}

#[test]
fn physical_delete_is_forbidden_everywhere() {
    let fx = fixture();
    fx.burial("A/1/1", "coffin_burial", "2020-01-01", "Törölhetetlen", "Tamás").unwrap();
    for t in ["cemeteries", "sections", "plot_rows", "plots", "deceased", "persons", "leases", "plot_holders"] {
        let e = fx.store.conn.execute(&format!("DELETE FROM {t}"), []);
        assert!(e.is_err(), "{t} allowed delete");
    }
}

#[test]
fn quick_search_is_accent_and_case_insensitive() {
    let fx = fixture();
    fx.burial("A/1/1", "coffin_burial", "2010-01-01", "Kovács", "József").unwrap();
    fx.burial("A/1/2", "coffin_burial", "2012-01-01", "Szőke", "Ágnes").unwrap();
    let s = &fx.admin;
    let h = fx.store.quick_search(s, "kovacs jozs", 20).unwrap();
    assert_eq!(h[0].title, "Kovács József");
    assert_eq!(h[0].plot_identifier.as_deref(), Some("A/1/1"));
    assert_eq!(fx.store.quick_search(s, "SZOKE", 20).unwrap()[0].title, "Szőke Ágnes");
    assert_eq!(fx.store.quick_search(s, "őke", 20).unwrap()[0].title, "Szőke Ágnes");
    // years
    assert_eq!(fx.store.quick_search(s, "kovács 2010", 20).unwrap().iter().filter(|h| h.kind == "deceased").count(), 1);
    assert_eq!(fx.store.quick_search(s, "kovács 2011", 20).unwrap().iter().filter(|h| h.kind == "deceased").count(), 0);
    // plot identifier
    let h = fx.store.quick_search(s, "a/1/1", 20).unwrap();
    assert_eq!(h[0].kind, "plot");
    assert_eq!(h[0].title, "A/1/1");
    // holder names
    assert!(fx.store.quick_search(s, "eltemetteto", 20).unwrap().iter().any(|h| h.kind == "person"));
}

#[test]
fn bulk_create_and_summaries() {
    let fx = fixture();
    let page = fx.store.plot_summaries(&PlotFilter { cemetery_id: Some(fx.cemetery.id.clone()), ..Default::default() }).unwrap();
    assert_eq!(page.total, 30);
    assert_eq!(page.items[0].identifier, "A/1/1");
    assert_eq!(page.items[9].identifier, "A/1/10");
    assert!(page.items.iter().all(|p| p.status == PlotStatus::Free));
    // idempotent
    let r = fx
        .store
        .bulk_create(
            &fx.admin,
            &BulkCreateInput {
                cemetery_id: fx.cemetery.id.clone(),
                section_code: "a".into(),
                row_from: 1,
                row_to: 4,
                plots_per_row: 10,
                first_plot_number: 1,
                plot_type_id: fx.store.get_plot_type("single").unwrap().id,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(r.plots_created, 10);
    assert_eq!(r.rows_created, 1);
    let free = fx.store.plot_summaries(&PlotFilter { statuses: vec!["free".into()], limit: Some(5), ..Default::default() }).unwrap();
    assert_eq!(free.items.len(), 5);
    assert_eq!(free.total, 40);
}

#[test]
fn users_login_lockout() {
    let fx = fixture();
    fx.store.create_initial_admin("gondnok", "Gondnok Géza", "gondnok-titka-77").unwrap();
    assert_eq!(fx.store.create_initial_admin("x", "X", "gondnok-titka-77").unwrap_err().code, "already_initialized");
    let s = fx.store.login("gondnok", "gondnok-titka-77").unwrap();
    assert_eq!(s.role, Role::Admin);
    for i in 0..4 {
        let e = fx.store.login("gondnok", "rossz").unwrap_err();
        assert_eq!(e.code, "invalid_credentials", "attempt {i}");
    }
    assert_eq!(fx.store.login("gondnok", "rossz").unwrap_err().code, "user_locked");
    // even the right password is refused while locked
    assert_eq!(fx.store.login("gondnok", "gondnok-titka-77").unwrap_err().code, "user_locked");
    fx.store.unlock_user(&s, &s.user_id.clone().unwrap()).unwrap();
    assert!(fx.store.login("gondnok", "gondnok-titka-77").is_ok());
    assert_eq!(fx.store.login("nincs", "akarmi").unwrap_err().code, "invalid_credentials");
    // last admin cannot be demoted
    let uid = s.user_id.clone().unwrap();
    assert_eq!(fx.store.update_user(&s, &uid, "G", "clerk", &[], true).unwrap_err().code, "last_admin");
}

#[test]
fn dashboard_and_reports() {
    let fx = fixture();
    fx.burial("A/1/1", "coffin_burial", "2024-01-01", "Stat", "Egy").unwrap();
    fx.burial("A/1/2", "urn_placement", "2024-05-01", "Stat", "Kettő").unwrap();
    let d = fx.store.dashboard(None).unwrap();
    assert_eq!(d.plots_total, 30);
    assert_eq!(d.deceased_total, 2);
    assert_eq!(d.recent_events.len(), 2);
    let y = fx.store.yearly_stats(None).unwrap();
    assert_eq!(y[0].year, 2024);
    assert_eq!(y[0].total, 2);
    let u = fx.store.utilization(None).unwrap();
    assert_eq!(u[0].total, 30);
    assert_eq!(u[0].by_status["occupied"], 2);
}

#[test]
fn all_documents_render_from_sample_data() {
    let store = lunda_core::Store::in_memory().unwrap();
    let s = lunda_core::auth::Session::system();
    let sum = lunda_core::sample::load_sample(&store, &s).unwrap();
    let dir = std::env::temp_dir().join(format!("lunda-pdf-{}", lunda_core::util::new_id()));
    let cem = sum.cemetery_id.clone();
    let any_lease: String = store.conn.query_row("SELECT id FROM leases WHERE status='active' LIMIT 1", [], |r| r.get(0)).unwrap();
    let deceased: String = store.conn.query_row("SELECT deceased_id FROM interment_events WHERE plot_id IS NOT NULL LIMIT 1", [], |r| r.get(0)).unwrap();
    let plots: Vec<String> = store
        .expiries(&s, &ExpiryFilter { cemetery_id: Some(cem.clone()), within_days: 0, include_expired: true, ..Default::default() })
        .unwrap()
        .into_iter()
        .take(3)
        .map(|e| e.plot_id)
        .collect();
    assert!(!plots.is_empty());
    let docs = vec![
        ("register", serde_json::json!({"cemeteryId": cem})),
        ("crypt_book", serde_json::json!({"cemeteryId": cem})),
        ("inspection", serde_json::json!({"cemeteryId": cem, "dateFrom": "2000-01-01", "dateTo": "2026-12-31"})),
        ("index", serde_json::json!({"cemeteryId": cem})),
        ("information", serde_json::json!({"deceasedId": deceased})),
        ("lease_certificate", serde_json::json!({"leaseId": any_lease})),
        ("fee_request", serde_json::json!({"leaseId": any_lease})),
        ("expiry_notice", serde_json::json!({"plotIds": plots})),
        ("grave_labels", serde_json::json!({"plotIds": plots})),
        ("notice_board", serde_json::json!({"plotIds": plots})),
        ("map", serde_json::json!({"cemeteryId": cem, "paper": "A0"})),
        ("report_utilization", serde_json::json!({})),
        ("report_yearly", serde_json::json!({})),
    ];
    for (doc, args) in docs {
        let f = lunda_core::pdf::generate(&store, &s, doc, &args, &dir).unwrap_or_else(|e| panic!("{doc}: {e}"));
        let bytes = std::fs::read(&f.path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"), "{doc}");
        assert!(f.pages >= 1, "{doc}");
    }
    // the register is paged, authenticated and logged
    let f = lunda_core::pdf::generate(&store, &s, "register", &serde_json::json!({"cemeteryId": cem}), &dir).unwrap();
    assert!(f.pages > 2);
    assert!(f.volume_id.is_some() && f.content_hash.as_ref().unwrap().len() == 64);
    let logged: i64 = store.conn.query_row("SELECT count(*) FROM print_log WHERE document='register'", [], |r| r.get(0)).unwrap();
    assert_eq!(logged, 2);
    // same data -> same content digest (reproducible authentication)
    let g = lunda_core::pdf::generate(&store, &s, "register", &serde_json::json!({"cemeteryId": cem}), &dir).unwrap();
    assert_eq!(f.content_hash, g.content_hash);
    // reader may print, but nothing without print permission
    let none = lunda_core::auth::Session { permissions: vec![], ..lunda_core::auth::Session::system() };
    assert_eq!(lunda_core::pdf::generate(&store, &none, "index", &serde_json::json!({"cemeteryId": cem}), &dir).unwrap_err().code, "forbidden");
    std::fs::remove_dir_all(dir).ok();
}

/// An urn taken home or ashes scattered elsewhere: no plot, but the register
/// keeps the entry with a folio, the receiver or the place, and prints it.
#[test]
fn urn_taken_home_and_ashes_scattered_elsewhere() {
    let fx = fixture();
    let first = fx.burial("A/1/1", "coffin_burial", "2024-03-01", "Előző", "Péter").unwrap();
    let off = |ty: &str, receiver: &str, destination: &str, family: &str| {
        fx.store.record_burial(
            &fx.admin,
            &BurialInput {
                event_type: ty.into(),
                event_date: "2024-03-05".into(),
                container: "urn".into(),
                deceased: DeceasedRef::New { deceased: Fx::deceased(family, "Anna", "2024-02-20") },
                plot_id: None,
                crypt_slot_id: None,
                arranger: None,
                holder: None,
                holder_relation: None,
                lease: LeaseChoice::default(),
                historical_folio: None,
                override_reason: None,
                notes: String::new(),
                cemetery_id: Some(fx.cemetery.id.clone()),
                receiver_name: receiver.into(),
                receiver_address: "6050 Lajosmizse, Fő utca 2.".into(),
                destination: destination.into(),
            },
        )
    };
    // the receiver is required for an urn taken home
    assert_eq!(off("urn_release", "", "", "Hazavitt").unwrap_err().code, "validation");
    let home = off("urn_release", "Hazavitt Gábor", "", "Hazavitt").unwrap();
    assert_eq!(home.folio, first.folio + 1);
    assert_eq!(home.event.plot_id, None);
    assert_eq!(home.event.destination, "hazavitték");
    assert_eq!(off("ash_scattering", "", "", "Szórt").unwrap_err().code, "validation");
    let scattered = off("ash_scattering", "", "Kisvölgyi köztemető, szóróparcella", "Szórt").unwrap();
    assert_eq!(scattered.folio, first.folio + 2);
    let events = fx.store.list_events(&EventFilter { cemetery_id: Some(fx.cemetery.id.clone()), ..Default::default() }).unwrap();
    assert_eq!(events.total, 3);
    // the register book prints entries without a plot
    let dir = std::env::temp_dir().join(format!("reg-{}", lunda_core::util::new_id()));
    let f = lunda_core::pdf::generate(&fx.store, &fx.admin, "register", &serde_json::json!({ "cemeteryId": fx.cemetery.id }), &dir).unwrap();
    assert!(f.pages >= 1);
    // the certified copy is now on record for the dashboard reminder
    assert_eq!(fx.store.dashboard(None).unwrap().days_since_certified_copy, Some(0));
    // the receipt and declaration for the urn taken home (Tv. 18. § (1a))
    let r = lunda_core::pdf::generate(&fx.store, &fx.admin, "urn_receipt", &serde_json::json!({ "eventId": home.event.id }), &dir).unwrap();
    assert_eq!(r.pages, 1);
    // NÖRI export of national graves runs (none in the fixture)
    lunda_core::io::export(&fx.store, &fx.admin, "nori", "xlsx", &serde_json::json!({ "cemeteryId": fx.cemetery.id }), &dir).unwrap();
}
