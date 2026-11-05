//! Deceased (FR-ELH-01, 05, 08), persons and plot holders (FR-UGY).

use crate::sql::{params, OptionalExtension};

use super::*;
use crate::audit;
use crate::auth::{Permission, Session};
use crate::text::normalize;
use crate::util::{new_id, now_ts, opt_date, parse_date};

pub(crate) fn deceased_search_text(d: &Deceased) -> String {
    normalize(&format!("{} {} {} {}", d.family_name, d.given_name, d.birth_name, d.name_prefix))
}

pub(crate) fn name_sort_key(family: &str, given: &str) -> String {
    crate::text::sort_key(&format!("{} {}", family.trim(), given.trim()))
}

pub(crate) fn person_search_text(p: &Person) -> String {
    normalize(&format!("{} {} {}", p.family_name, p.given_name, p.name_prefix))
}

fn reindex(conn: &crate::sql::Connection, kind: &str, id: &str, text: &str) -> Result<()> {
    conn.execute("DELETE FROM search_index WHERE kind=?1 AND ref_id=?2", params![kind, id])?;
    conn.execute("INSERT INTO search_index (kind, ref_id, text) VALUES (?1,?2,?3)", params![kind, id, text])?;
    Ok(())
}

pub(crate) fn validate_deceased(d: &Deceased) -> Result<()> {
    if d.unknown_identity {
        // FR-ELH-05 / JOG-08
        non_empty("authorityName", &d.authority_name)?;
        non_empty("caseNumber", &d.case_number)?;
    } else {
        // JOG-03 mandatory columns
        non_empty("familyName", &d.family_name)?;
        non_empty("givenName", &d.given_name)?;
        non_empty("motherName", &d.mother_name)?;
        non_empty("lastResidence", &d.last_residence)?;
        if d.birth_date.as_deref().map(str::trim).unwrap_or("").is_empty() {
            return Err(CoreError::validation("birthDate", "birth date is required").with("code", "required"));
        }
    }
    if d.death_date.as_deref().map(str::trim).unwrap_or("").is_empty() {
        return Err(CoreError::validation("deathDate", "death date is required").with("code", "required"));
    }
    let b = opt_date(&d.birth_date)?;
    let de = opt_date(&d.death_date)?;
    if let (Some(b), Some(de)) = (b, de) {
        if b > de {
            return Err(CoreError::validation("deathDate", "death date before birth date").with("code", "death_before_birth"));
        }
    }
    if !["", "male", "female", "unknown"].contains(&d.sex.as_str()) {
        return Err(CoreError::validation("sex", "invalid value"));
    }
    Ok(())
}

impl Store {
    // ---------------------------------------------------------------- deceased

    pub fn get_deceased(&self, id: &str) -> Result<Deceased> {
        self.conn.query_row("SELECT * FROM deceased WHERE id=?1", [id], map_deceased).optional()?.ok_or_else(|| CoreError::not_found("deceased", id))
    }

    pub(crate) fn insert_deceased(&self, tx: &crate::sql::Connection, s: &Session, d: &Deceased, validate: bool) -> Result<Deceased> {
        if validate {
            validate_deceased(d)?;
        }
        let id = new_id();
        let ts = now_ts();
        let mut d = d.clone();
        d.id = id.clone();
        d.family_name = d.family_name.trim().into();
        d.given_name = d.given_name.trim().into();
        tx.execute(
            "INSERT INTO deceased (id, name_prefix, family_name, given_name, birth_name, sex, birth_place, birth_date, mother_name, last_residence,
                death_place, death_date, death_cert_no, unknown_identity, authority_name, case_number, notes, search_text, created_at, created_by, updated_at, updated_by, sort_key)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?19,?20,?21)",
            params![
                id, d.name_prefix.trim(), d.family_name, d.given_name, d.birth_name.trim(), d.sex, d.birth_place.trim(), opt_s(&d.birth_date),
                d.mother_name.trim(), d.last_residence.trim(), d.death_place.trim(), opt_s(&d.death_date), d.death_cert_no.trim(),
                d.unknown_identity, d.authority_name.trim(), d.case_number.trim(), d.notes, deceased_search_text(&d), ts, s.user_name,
                name_sort_key(&d.family_name, &d.given_name)
            ],
        )?;
        reindex(tx, "deceased", &id, &deceased_search_text(&d))?;
        audit::record(tx, s, "deceased", &id, "create", None::<&()>, Some(&d), None)?;
        Ok(d)
    }

    /// Import path: inserts without the mandatory-column validation (the gaps
    /// are reported as warnings by the importer).
    pub(crate) fn insert_deceased_unchecked(&self, tx: &crate::sql::Connection, s: &Session, d: &Deceased) -> Result<String> {
        Ok(self.insert_deceased(tx, s, d, false)?.id)
    }

    pub(crate) fn primary_holder_id_pub(&self, plot_id: &str) -> Result<Option<String>> {
        self.primary_holder_id(&self.conn, plot_id)
    }

    /// Updates a deceased person's data. When the person already appears in
    /// the register, this is a correction (FR-JOG-03): needs `correct_entries`
    /// and a justification; the original stays in the audit log.
    pub fn update_deceased(&self, s: &Session, d: &Deceased, reason: Option<&str>) -> Result<Deceased> {
        s.require(Permission::RecordEvents)?;
        validate_deceased(d)?;
        let old = self.get_deceased(&d.id)?;
        if old.voided_at.is_some() {
            return Err(CoreError::new("immutable", "voided record"));
        }
        let in_register: i64 =
            self.conn.query_row("SELECT count(*) FROM interment_events WHERE deceased_id=?1 AND voided_at IS NULL", [&d.id], |r| r.get(0))?;
        let reason = if in_register > 0 {
            s.require(Permission::CorrectEntries)?;
            Some(require_reason(reason)?)
        } else {
            None
        };
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE deceased SET name_prefix=?2, family_name=?3, given_name=?4, birth_name=?5, sex=?6, birth_place=?7, birth_date=?8, mother_name=?9,
                last_residence=?10, death_place=?11, death_date=?12, death_cert_no=?13, unknown_identity=?14, authority_name=?15, case_number=?16,
                notes=?17, search_text=?18, updated_at=?19, updated_by=?20, sort_key=?21 WHERE id=?1",
            params![
                d.id,
                d.name_prefix.trim(),
                d.family_name.trim(),
                d.given_name.trim(),
                d.birth_name.trim(),
                d.sex,
                d.birth_place.trim(),
                opt_s(&d.birth_date),
                d.mother_name.trim(),
                d.last_residence.trim(),
                d.death_place.trim(),
                opt_s(&d.death_date),
                d.death_cert_no.trim(),
                d.unknown_identity,
                d.authority_name.trim(),
                d.case_number.trim(),
                d.notes,
                deceased_search_text(d),
                now_ts(),
                s.user_name,
                name_sort_key(&d.family_name, &d.given_name)
            ],
        )?;
        reindex(&tx, "deceased", &d.id, &deceased_search_text(d))?;
        let new = tx.query_row("SELECT * FROM deceased WHERE id=?1", [&d.id], map_deceased)?;
        audit::record(&tx, s, "deceased", &d.id, if reason.is_some() { "correct" } else { "update" }, Some(&old), Some(&new), reason.as_deref())?;
        tx.commit()?;
        Ok(new)
    }

    /// UC-01 step 2: similar deceased already recorded (same family+given name, accent-insensitive).
    pub fn similar_deceased(&self, family_name: &str, given_name: &str, birth_date: Option<&str>) -> Result<Vec<Deceased>> {
        let fam = normalize(family_name);
        let giv = normalize(given_name);
        if fam.is_empty() {
            return Ok(vec![]);
        }
        let mut stmt = self.conn.prepare(
            "SELECT * FROM deceased WHERE voided_at IS NULL AND search_text LIKE ?1 || '%' AND (?2 = '' OR search_text LIKE '%' || ?2 || '%')
             ORDER BY (birth_date = ?3) DESC, sort_key LIMIT 20",
        )?;
        let rows = stmt.query_map(params![fam, giv, birth_date.unwrap_or("")], map_deceased)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn deceased_events(&self, deceased_id: &str) -> Result<Vec<EventRow>> {
        let mut stmt = self.conn.prepare(&format!("{EVENT_ROW_SQL} WHERE e.deceased_id=?1 ORDER BY e.event_date, e.folio"))?;
        let rows = stmt.query_map([deceased_id], map_event_row)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---------------------------------------------------------------- persons

    fn mask(&self, s: &Session, mut p: Person) -> Person {
        if !s.can(Permission::ViewPersonal) {
            p.address.clear();
            p.mail_address.clear();
            p.phone.clear();
            p.email.clear();
            p.masked = true;
        }
        p
    }

    /// Person detail. Viewing personal data is logged (NFR-BIZ-03).
    pub fn get_person(&self, s: &Session, id: &str) -> Result<Person> {
        let p = self.conn.query_row("SELECT * FROM persons WHERE id=?1", [id], map_person).optional()?.ok_or_else(|| CoreError::not_found("person", id))?;
        if s.can(Permission::ViewPersonal) {
            audit::record_simple(&self.conn, s, "person", id, "view_personal", None)?;
        }
        Ok(self.mask(s, p))
    }

    pub(crate) fn insert_person(&self, tx: &crate::sql::Connection, s: &Session, p: &Person) -> Result<Person> {
        non_empty("familyName", &p.family_name)?;
        let id = new_id();
        let ts = now_ts();
        let mut p = p.clone();
        p.id = id.clone();
        p.masked = false;
        tx.execute(
            "INSERT INTO persons (id, name_prefix, family_name, given_name, address, mail_address, phone, email, notes, search_text, created_at, created_by, updated_at, updated_by, sort_key)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?11,?12,?13)",
            params![id, p.name_prefix.trim(), p.family_name.trim(), p.given_name.trim(), p.address.trim(), p.mail_address.trim(), p.phone.trim(), p.email.trim(), p.notes, person_search_text(&p), ts, s.user_name, name_sort_key(&p.family_name, &p.given_name)],
        )?;
        reindex(tx, "person", &id, &person_search_text(&p))?;
        audit::record(tx, s, "person", &id, "create", None::<&()>, Some(&p), None)?;
        Ok(p)
    }

    pub fn save_person(&self, s: &Session, p: &Person) -> Result<Person> {
        if !(s.can(Permission::RecordEvents) || s.can(Permission::RecordLeases) || s.can(Permission::RecordPayments)) {
            return Err(CoreError::forbidden("record_events"));
        }
        non_empty("familyName", &p.family_name)?;
        let tx = crate::db::tx(&self.conn)?;
        let out = if p.id.is_empty() {
            self.insert_person(&tx, s, p)?
        } else {
            let old =
                tx.query_row("SELECT * FROM persons WHERE id=?1", [&p.id], map_person).optional()?.ok_or_else(|| CoreError::not_found("person", &p.id))?;
            // a masked client must not wipe personal data it could not see
            let (address, mail, phone, email) = if p.masked || !s.can(Permission::ViewPersonal) {
                (old.address.clone(), old.mail_address.clone(), old.phone.clone(), old.email.clone())
            } else {
                (p.address.trim().into(), p.mail_address.trim().into(), p.phone.trim().into(), p.email.trim().into())
            };
            tx.execute(
                "UPDATE persons SET name_prefix=?2, family_name=?3, given_name=?4, address=?5, mail_address=?6, phone=?7, email=?8, notes=?9, search_text=?10, updated_at=?11, updated_by=?12, sort_key=?13 WHERE id=?1",
                params![p.id, p.name_prefix.trim(), p.family_name.trim(), p.given_name.trim(), address, mail, phone, email, p.notes, person_search_text(p), now_ts(), s.user_name, name_sort_key(&p.family_name, &p.given_name)],
            )?;
            reindex(&tx, "person", &p.id, &person_search_text(p))?;
            let new = tx.query_row("SELECT * FROM persons WHERE id=?1", [&p.id], map_person)?;
            audit::record(&tx, s, "person", &p.id, "update", Some(&old), Some(&new), None)?;
            new
        };
        tx.commit()?;
        Ok(self.mask(s, out))
    }

    pub(crate) fn resolve_person(&self, tx: &crate::sql::Connection, s: &Session, r: &PersonRef) -> Result<String> {
        match r {
            PersonRef::Existing { id } => {
                let ok: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM persons WHERE id=?1 AND voided_at IS NULL)", [id], |r| r.get(0))?;
                if !ok {
                    return Err(CoreError::not_found("person", id));
                }
                Ok(id.clone())
            }
            PersonRef::New { person } => Ok(self.insert_person(tx, s, person)?.id),
        }
    }

    pub fn list_persons(&self, s: &Session, query: &str, limit: i64) -> Result<Vec<Person>> {
        let q = normalize(query);
        let mut stmt = self.conn.prepare(
            "SELECT * FROM persons WHERE voided_at IS NULL AND (?1 = '' OR search_text LIKE '%' || ?1 || '%')
             ORDER BY sort_key LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![q, limit], map_person)?;
        let v: Vec<Person> = rows.collect::<std::result::Result<_, _>>()?;
        Ok(v.into_iter().map(|p| self.mask(s, p)).collect())
    }

    /// Plots where the person is (or was) a holder.
    pub fn person_plots(&self, person_id: &str) -> Result<Vec<Holder>> {
        let mut stmt = self.conn.prepare(
            "SELECT h.*, trim(pe.name_prefix||' '||pe.family_name||' '||pe.given_name) AS person_name, pe.address AS person_address, p.identifier AS plot_identifier
             FROM plot_holders h JOIN persons pe ON pe.id=h.person_id JOIN plots p ON p.id=h.plot_id
             WHERE h.person_id=?1 ORDER BY h.start_date DESC",
        )?;
        let rows = stmt.query_map([person_id], map_holder)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---------------------------------------------------------------- holders

    pub fn holders_for_plot(&self, plot_id: &str) -> Result<Vec<Holder>> {
        let mut stmt = self.conn.prepare(
            "SELECT h.*, trim(pe.name_prefix||' '||pe.family_name||' '||pe.given_name) AS person_name, pe.address AS person_address, p.identifier AS plot_identifier
             FROM plot_holders h JOIN persons pe ON pe.id=h.person_id JOIN plots p ON p.id=h.plot_id
             WHERE h.plot_id=?1 ORDER BY (h.end_date IS NULL) DESC, h.role DESC, h.start_date DESC",
        )?;
        let rows = stmt.query_map([plot_id], map_holder)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub(crate) fn primary_holder_id(&self, conn: &crate::sql::Connection, plot_id: &str) -> Result<Option<String>> {
        Ok(conn
            .query_row("SELECT person_id FROM plot_holders WHERE plot_id=?1 AND role='primary' AND end_date IS NULL AND voided_at IS NULL", [plot_id], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub(crate) fn insert_holder(
        &self,
        tx: &crate::sql::Connection,
        s: &Session,
        plot_id: &str,
        person_id: &str,
        role: &str,
        relation: &str,
        start: &str,
        basis: &str,
        doc: &str,
    ) -> Result<String> {
        let id = new_id();
        let ts = now_ts();
        tx.execute(
            "INSERT INTO plot_holders (id, plot_id, person_id, role, relation, start_date, legal_basis, document_ref, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?9,?10)",
            params![id, plot_id, person_id, role, relation, start, basis, doc, ts, s.user_name],
        )?;
        audit::record(
            tx,
            s,
            "holder",
            &id,
            "create",
            None::<&()>,
            Some(&serde_json::json!({
                "plotId": plot_id, "personId": person_id, "role": role, "startDate": start, "legalBasis": basis, "documentRef": doc
            })),
            None,
        )?;
        Ok(id)
    }

    /// FR-UGY-03: change of the primary holder (inheritance, transfer) or adding a contact.
    /// The previous primary holder's record is closed, never removed.
    pub fn change_holder(&self, s: &Session, input: &HolderChangeInput) -> Result<Vec<Holder>> {
        s.require(Permission::RecordLeases)?;
        let date = parse_date(&input.change_date)?;
        let person = input.person.as_ref().ok_or_else(|| CoreError::validation("person", "person is required"))?;
        let role = if input.role == "contact" { "contact" } else { "primary" };
        if role == "primary" {
            non_empty("legalBasis", &input.legal_basis)?;
        }
        self.get_plot(&input.plot_id)?;
        let tx = crate::db::tx(&self.conn)?;
        let person_id = self.resolve_person(&tx, s, person)?;
        if role == "primary" {
            let current: Vec<(String, String)> = {
                let mut st =
                    tx.prepare("SELECT id, start_date FROM plot_holders WHERE plot_id=?1 AND role='primary' AND end_date IS NULL AND voided_at IS NULL")?;
                let rows = st.query_map([&input.plot_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect::<std::result::Result<_, _>>()?
            };
            for (hid, start) in current {
                if parse_date(&start)? > date {
                    return Err(CoreError::validation("changeDate", "change date before the current holder's start").with("code", "date_before_start"));
                }
                tx.execute(
                    "UPDATE plot_holders SET end_date=?2, updated_at=?3, updated_by=?4 WHERE id=?1",
                    params![hid, input.change_date, now_ts(), s.user_name],
                )?;
                audit::record(
                    &tx,
                    s,
                    "holder",
                    &hid,
                    "end",
                    None::<&()>,
                    Some(&serde_json::json!({"endDate": input.change_date, "legalBasis": input.legal_basis})),
                    None,
                )?;
            }
        }
        self.insert_holder(&tx, s, &input.plot_id, &person_id, role, &input.relation, &input.change_date, &input.legal_basis, &input.document_ref)?;
        // keep the active lease's holder in sync
        if role == "primary" {
            tx.execute(
                "UPDATE leases SET person_id=?2, updated_at=?3, updated_by=?4 WHERE plot_id=?1 AND status='active' AND voided_at IS NULL",
                params![input.plot_id, person_id, now_ts(), s.user_name],
            )?;
        }
        tx.commit()?;
        self.holders_for_plot(&input.plot_id)
    }
}

pub(crate) fn map_holder(r: &crate::sql::Row) -> crate::sql::Result<Holder> {
    Ok(Holder {
        id: r.get("id")?,
        plot_id: r.get("plot_id")?,
        person_id: r.get("person_id")?,
        person_name: r.get("person_name")?,
        person_address: r.get("person_address")?,
        role: r.get("role")?,
        relation: r.get("relation")?,
        start_date: r.get("start_date")?,
        end_date: r.get("end_date")?,
        legal_basis: r.get("legal_basis")?,
        document_ref: r.get("document_ref")?,
        plot_identifier: r.get("plot_identifier")?,
        voided_at: r.get("voided_at")?,
    })
}
