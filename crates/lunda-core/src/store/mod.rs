//! Application service layer: every operation checks permissions, validates,
//! writes the change and its audit entry in one transaction.

mod events;
mod leases;
mod people;
mod reports;
mod search;
mod structure;
mod users;

pub use reports::*;
pub use structure::natural_key;

use crate::sql::{Connection, Row};

use crate::error::{CoreError, Result};
use crate::model::*;

pub struct Store {
    pub conn: Connection,
}

impl Store {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    pub fn in_memory() -> Result<Self> {
        Ok(Self::new(crate::db::open_in_memory()?))
    }
}

pub(crate) fn require_reason(reason: Option<&str>) -> Result<String> {
    match reason.map(str::trim) {
        Some(r) if r.chars().count() >= 3 => Ok(r.to_string()),
        _ => Err(CoreError::reason_required()),
    }
}

pub(crate) fn non_empty(field: &str, v: &str) -> Result<()> {
    if v.trim().is_empty() {
        Err(CoreError::validation(field, format!("{field} is required")).with("code", "required"))
    } else {
        Ok(())
    }
}

pub(crate) fn opt_s(v: &Option<String>) -> Option<String> {
    v.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// Row mappers
// ---------------------------------------------------------------------------

pub(crate) fn map_cemetery(r: &Row) -> crate::sql::Result<Cemetery> {
    Ok(Cemetery {
        id: r.get("id")?,
        name: r.get("name")?,
        address: r.get("address")?,
        parcel_number: r.get("parcel_number")?,
        kind: r.get("kind")?,
        owner: r.get("owner")?,
        operator: r.get("operator")?,
        opening_hours: r.get("opening_hours")?,
        folio_scope: r.get("folio_scope")?,
        notes: r.get("notes")?,
        voided_at: r.get("voided_at")?,
    })
}

pub(crate) fn map_plot_type(r: &Row) -> crate::sql::Result<PlotType> {
    Ok(PlotType {
        id: r.get("id")?,
        code: r.get("code")?,
        name: r.get("name")?,
        category: r.get("category")?,
        default_lease_years: r.get("default_lease_years")?,
        rest_period_years: r.get("rest_period_years")?,
        coffin_capacity: r.get("coffin_capacity")?,
        urn_capacity: r.get("urn_capacity")?,
        width_cm: r.get("width_cm")?,
        length_cm: r.get("length_cm")?,
        builtin: r.get("builtin")?,
        sort_order: r.get("sort_order")?,
        voided_at: r.get("voided_at")?,
    })
}

pub(crate) fn map_plot(r: &Row) -> crate::sql::Result<Plot> {
    Ok(Plot {
        id: r.get("id")?,
        cemetery_id: r.get("cemetery_id")?,
        section_id: r.get("section_id")?,
        row_id: r.get("row_id")?,
        number: r.get("number")?,
        identifier: r.get("identifier")?,
        plot_type_id: r.get("plot_type_id")?,
        coffin_capacity: r.get("coffin_capacity")?,
        urn_capacity: r.get("urn_capacity")?,
        width_cm: r.get("width_cm")?,
        length_cm: r.get("length_cm")?,
        rest_period_years: r.get("rest_period_years")?,
        status_override: r.get("status_override")?,
        status_override_reason: r.get("status_override_reason")?,
        unusable: r.get("unusable")?,
        protected: r.get("protected")?,
        protection_kind: r.get("protection_kind")?,
        protection_reason: r.get("protection_reason")?,
        further_burial_allowed: r.get("further_burial_allowed")?,
        resale_ready_at: r.get("resale_ready_at")?,
        notes: r.get("notes")?,
        map_x: r.get("map_x")?,
        map_y: r.get("map_y")?,
        map_w: r.get("map_w")?,
        map_h: r.get("map_h")?,
        map_rot: r.get("map_rot")?,
        merged_into_id: r.get("merged_into_id")?,
        gps_lat: r.get("gps_lat")?,
        gps_lon: r.get("gps_lon")?,
        crypt_number: r.get("crypt_number")?,
        crypt_built_year: r.get("crypt_built_year")?,
        crypt_permit: r.get("crypt_permit")?,
        crypt_holder_instructions: r.get("crypt_holder_instructions")?,
        voided_at: r.get("voided_at")?,
    })
}

pub(crate) fn map_deceased(r: &Row) -> crate::sql::Result<Deceased> {
    Ok(Deceased {
        id: r.get("id")?,
        name_prefix: r.get("name_prefix")?,
        family_name: r.get("family_name")?,
        given_name: r.get("given_name")?,
        birth_name: r.get("birth_name")?,
        sex: r.get("sex")?,
        birth_place: r.get("birth_place")?,
        birth_date: r.get("birth_date")?,
        mother_name: r.get("mother_name")?,
        last_residence: r.get("last_residence")?,
        death_place: r.get("death_place")?,
        death_date: r.get("death_date")?,
        death_cert_no: r.get("death_cert_no")?,
        unknown_identity: r.get("unknown_identity")?,
        authority_name: r.get("authority_name")?,
        case_number: r.get("case_number")?,
        notes: r.get("notes")?,
        voided_at: r.get("voided_at")?,
    })
}

/// Public alias used by document generators.
pub fn map_person_pub(r: &Row) -> crate::sql::Result<Person> {
    map_person(r)
}

pub(crate) fn map_person(r: &Row) -> crate::sql::Result<Person> {
    Ok(Person {
        id: r.get("id")?,
        name_prefix: r.get("name_prefix")?,
        family_name: r.get("family_name")?,
        given_name: r.get("given_name")?,
        address: r.get("address")?,
        mail_address: r.get("mail_address")?,
        phone: r.get("phone")?,
        email: r.get("email")?,
        notes: r.get("notes")?,
        masked: false,
        voided_at: r.get("voided_at")?,
    })
}

pub(crate) fn map_event(r: &Row) -> crate::sql::Result<InterEvent> {
    Ok(InterEvent {
        id: r.get("id")?,
        cemetery_id: r.get("cemetery_id")?,
        scope_key: r.get("scope_key")?,
        folio: r.get("folio")?,
        historical: r.get("historical")?,
        event_type: r.get("event_type")?,
        container: r.get("container")?,
        event_date: r.get("event_date")?,
        deceased_id: r.get("deceased_id")?,
        plot_id: r.get("plot_id")?,
        crypt_slot_id: r.get("crypt_slot_id")?,
        arranger_id: r.get("arranger_id")?,
        receiver_name: r.get("receiver_name")?,
        receiver_address: r.get("receiver_address")?,
        destination: r.get("destination")?,
        source_event_id: r.get("source_event_id")?,
        override_reason: r.get("override_reason")?,
        notes: r.get("notes")?,
        voided_at: r.get("voided_at")?,
        void_reason: r.get("void_reason")?,
    })
}

pub(crate) fn map_lease(r: &Row) -> crate::sql::Result<Lease> {
    Ok(Lease {
        id: r.get("id")?,
        plot_id: r.get("plot_id")?,
        person_id: r.get("person_id")?,
        person_name: r.get("person_name")?,
        kind: r.get("kind")?,
        start_date: r.get("start_date")?,
        duration_years: r.get("duration_years")?,
        expiry_date: r.get("expiry_date")?,
        fee_huf: r.get("fee_huf")?,
        fee_rate_id: r.get("fee_rate_id")?,
        fee_exempt: r.get("fee_exempt")?,
        status: r.get("status")?,
        cancel_date: r.get("cancel_date")?,
        refund_huf: r.get("refund_huf")?,
        related_event_id: r.get("related_event_id")?,
        notes: r.get("notes")?,
        voided_at: r.get("voided_at")?,
    })
}

/// SELECT list for [`map_event_row`]; needs aliases e, d, p, c, a, s.
pub(crate) const EVENT_ROW_SQL: &str = "
    SELECT e.id, e.cemetery_id, c.name AS cemetery_name, e.folio, e.scope_key, e.historical, e.event_type,
           e.container, e.event_date, e.deceased_id,
           trim(d.name_prefix || ' ' || d.family_name || ' ' || d.given_name) AS deceased_name,
           d.birth_name, d.birth_date, d.birth_place, d.mother_name, d.last_residence, d.death_date,
           d.unknown_identity, d.authority_name, d.case_number,
           e.plot_id, p.identifier AS plot_identifier, s.slot_no AS crypt_slot_no,
           CASE WHEN a.id IS NULL THEN NULL ELSE trim(a.name_prefix || ' ' || a.family_name || ' ' || a.given_name) END AS arranger_name,
           a.address AS arranger_address,
           e.receiver_name, e.destination, e.notes, e.voided_at, e.void_reason
    FROM interment_events e
    JOIN deceased d ON d.id = e.deceased_id
    JOIN cemeteries c ON c.id = e.cemetery_id
    LEFT JOIN plots p ON p.id = e.plot_id
    LEFT JOIN crypt_slots s ON s.id = e.crypt_slot_id
    LEFT JOIN persons a ON a.id = e.arranger_id";

pub(crate) fn map_event_row(r: &Row) -> crate::sql::Result<EventRow> {
    Ok(EventRow {
        id: r.get("id")?,
        cemetery_id: r.get("cemetery_id")?,
        cemetery_name: r.get("cemetery_name")?,
        folio: r.get("folio")?,
        scope_key: r.get("scope_key")?,
        historical: r.get("historical")?,
        event_type: r.get("event_type")?,
        container: r.get("container")?,
        event_date: r.get("event_date")?,
        deceased_id: r.get("deceased_id")?,
        deceased_name: r.get("deceased_name")?,
        birth_name: r.get("birth_name")?,
        birth_date: r.get("birth_date")?,
        birth_place: r.get("birth_place")?,
        mother_name: r.get("mother_name")?,
        last_residence: r.get("last_residence")?,
        death_date: r.get("death_date")?,
        unknown_identity: r.get("unknown_identity")?,
        authority_name: r.get("authority_name")?,
        case_number: r.get("case_number")?,
        plot_id: r.get("plot_id")?,
        plot_identifier: r.get("plot_identifier")?,
        crypt_slot_no: r.get("crypt_slot_no")?,
        arranger_name: r.get("arranger_name")?,
        arranger_address: r.get("arranger_address")?,
        receiver_name: r.get("receiver_name")?,
        destination: r.get("destination")?,
        notes: r.get("notes")?,
        voided_at: r.get("voided_at")?,
        void_reason: r.get("void_reason")?,
    })
}

/// Placement event types (occupy a plot) and removal types (end a placement).
pub const PLACEMENT_TYPES: &[&str] = &["coffin_burial", "urn_placement", "reburial", "common_grave", "relocation", "ash_scattering"];
pub const REMOVAL_TYPES: &[&str] = &["urn_release", "urn_removal", "relocation"];
pub const ALL_EVENT_TYPES: &[&str] =
    &["coffin_burial", "urn_placement", "reburial", "urn_release", "urn_removal", "ash_scattering", "relocation", "common_grave"];

/// SQL fragment: active placements (not voided, not removed) – alias `e`.
pub(crate) const ACTIVE_PLACEMENT_SQL: &str = "
    e.voided_at IS NULL AND e.plot_id IS NOT NULL
    AND e.event_type IN ('coffin_burial','urn_placement','reburial','common_grave','relocation','ash_scattering')
    AND NOT EXISTS (SELECT 1 FROM interment_events r WHERE r.source_event_id = e.id AND r.voided_at IS NULL)";
