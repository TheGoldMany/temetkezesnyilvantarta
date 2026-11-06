#![allow(dead_code)]

use lunda_core::auth::{Role, Session};
use lunda_core::model::*;
use lunda_core::Store;

pub struct Fx {
    pub store: Store,
    pub admin: Session,
    pub cemetery: Cemetery,
}

pub fn admin() -> Session {
    Session::for_role(Some("u-admin".into()), "admin", "Admin", Role::Admin, &[])
}

pub fn session(role: Role) -> Session {
    Session::for_role(Some(format!("u-{}", role.as_str())), role.as_str(), role.as_str(), role, &[])
}

pub fn fixture() -> Fx {
    let store = Store::in_memory().unwrap();
    let admin = admin();
    let cemetery = store.save_cemetery(&admin, &Cemetery { name: "Teszt köztemető".into(), kind: "public".into(), ..Default::default() }).unwrap();
    let single = store.get_plot_type("single").unwrap();
    store
        .bulk_create(
            &admin,
            &BulkCreateInput {
                cemetery_id: cemetery.id.clone(),
                section_code: "A".into(),
                row_from: 1,
                row_to: 3,
                plots_per_row: 10,
                first_plot_number: 1,
                plot_type_id: single.id.clone(),
                ..Default::default()
            },
        )
        .unwrap();
    for (kind, amount) in [("purchase", 100_000), ("renewal", 80_000)] {
        store
            .add_fee_rate(
                &admin,
                &FeeRate {
                    cemetery_id: cemetery.id.clone(),
                    plot_type_id: Some(single.id.clone()),
                    fee_kind: kind.into(),
                    amount_huf: amount,
                    valid_from: "1990-01-01".into(),
                    ..Default::default()
                },
            )
            .unwrap();
    }
    Fx { store, admin, cemetery }
}

impl Fx {
    pub fn plot(&self, ident: &str) -> Plot {
        self.store.find_plot(&self.cemetery.id, ident).unwrap().expect("plot exists")
    }

    pub fn deceased(family: &str, given: &str, death: &str) -> Deceased {
        Deceased {
            family_name: family.into(),
            given_name: given.into(),
            birth_date: Some("1940-05-01".into()),
            birth_place: "Kecskemét".into(),
            mother_name: "Minta Mária".into(),
            last_residence: "6050 Lajosmizse, Fő utca 1.".into(),
            death_date: Some(death.into()),
            sex: "male".into(),
            ..Default::default()
        }
    }

    pub fn person(family: &str, given: &str) -> Person {
        Person { family_name: family.into(), given_name: given.into(), address: "6050 Lajosmizse, Kossuth u. 2.".into(), ..Default::default() }
    }

    pub fn burial(&self, ident: &str, ty: &str, date: &str, family: &str, given: &str) -> lunda_core::Result<BurialResult> {
        let p = self.plot(ident);
        self.store.record_burial(
            &self.admin,
            &BurialInput {
                event_type: ty.into(),
                event_date: date.into(),
                container: if ty == "urn_placement" { "urn".into() } else { "coffin".into() },
                deceased: DeceasedRef::New { deceased: Self::deceased(family, given, date) },
                plot_id: Some(p.id),
                crypt_slot_id: None,
                arranger: Some(PersonRef::New { person: Self::person("Eltemettető", "Éva") }),
                holder: None,
                holder_relation: Some("lánya".into()),
                lease: LeaseChoice { create: true, ..Default::default() },
                historical_folio: None,
                override_reason: None,
                notes: String::new(),
                cemetery_id: None,
                receiver_name: String::new(),
                receiver_address: String::new(),
                destination: String::new(),
            },
        )
    }
}
