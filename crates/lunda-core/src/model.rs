//! Data transfer objects shared between the core and the UI (camelCase JSON).

use serde::{Deserialize, Serialize};

use crate::rules::{PlotStatus, Verdict};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Cemetery {
    pub id: String,
    pub name: String,
    pub address: String,
    pub parcel_number: String,
    pub kind: String,
    pub owner: String,
    pub operator: String,
    pub opening_hours: String,
    pub folio_scope: String,
    pub notes: String,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlotType {
    pub id: String,
    pub code: String,
    pub name: String,
    pub category: String,
    pub default_lease_years: i32,
    pub rest_period_years: i32,
    pub coffin_capacity: i32,
    pub urn_capacity: i32,
    pub width_cm: Option<i32>,
    pub length_cm: Option<i32>,
    pub builtin: bool,
    pub sort_order: i32,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Section {
    pub id: String,
    pub cemetery_id: String,
    pub code: String,
    pub name: String,
    pub map_x: f64,
    pub map_y: f64,
    pub map_w: f64,
    pub map_h: f64,
    pub map_rot: f64,
    pub row_count: i64,
    pub plot_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlotRow {
    pub id: String,
    pub section_id: String,
    pub number: String,
    pub direction: String,
}

/// Bulk creation of a section with rows and plots (FR-TEM-03).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BulkCreateInput {
    pub cemetery_id: String,
    pub section_code: String,
    pub section_name: String,
    pub row_from: i32,
    pub row_to: i32,
    pub plots_per_row: i32,
    pub first_plot_number: i32,
    pub plot_type_id: String,
    /// map origin of the section (map units = decimetres)
    pub origin_x: Option<f64>,
    pub origin_y: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct BulkCreateResult {
    pub section_id: String,
    pub rows_created: i64,
    pub plots_created: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Plot {
    pub id: String,
    pub cemetery_id: String,
    pub section_id: String,
    pub row_id: String,
    pub number: String,
    pub identifier: String,
    pub plot_type_id: String,
    pub coffin_capacity: i32,
    pub urn_capacity: i32,
    pub width_cm: Option<i32>,
    pub length_cm: Option<i32>,
    pub rest_period_years: Option<i32>,
    pub status_override: Option<String>,
    pub status_override_reason: Option<String>,
    pub unusable: bool,
    pub protected: bool,
    pub protection_kind: Option<String>,
    pub protection_reason: Option<String>,
    pub further_burial_allowed: bool,
    pub resale_ready_at: Option<String>,
    pub notes: String,
    pub map_x: f64,
    pub map_y: f64,
    pub map_w: f64,
    pub map_h: f64,
    pub map_rot: f64,
    pub merged_into_id: Option<String>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub crypt_number: Option<String>,
    pub crypt_built_year: Option<i32>,
    pub crypt_permit: Option<String>,
    pub crypt_holder_instructions: Option<String>,
    pub voided_at: Option<String>,
}

/// Compact plot info for lists and the map.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlotSummary {
    pub id: String,
    pub identifier: String,
    pub cemetery_id: String,
    pub section_id: String,
    pub row_id: String,
    pub type_code: String,
    pub type_name: String,
    pub status: PlotStatus,
    pub protected: bool,
    pub occupants: i32,
    pub lease_expiry: Option<String>,
    pub holder_name: Option<String>,
    pub occupant_names: Option<String>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rot: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Restriction {
    pub id: String,
    pub plot_id: String,
    pub kind: String,
    pub text: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Monument {
    pub id: String,
    pub plot_id: String,
    pub entry_date: String,
    pub description: String,
    pub contractor: String,
    pub material: String,
    pub size: String,
    pub permit_date: Option<String>,
    pub protected: bool,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CryptSlot {
    pub id: String,
    pub plot_id: String,
    pub slot_no: i32,
    pub label: String,
    /// active occupant of the slot
    pub occupant_name: Option<String>,
    pub occupant_event_id: Option<String>,
    pub occupant_folio: Option<i64>,
    pub occupant_date: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Deceased {
    pub id: String,
    pub name_prefix: String,
    pub family_name: String,
    pub given_name: String,
    pub birth_name: String,
    pub sex: String,
    pub birth_place: String,
    pub birth_date: Option<String>,
    pub mother_name: String,
    pub last_residence: String,
    pub death_place: String,
    pub death_date: Option<String>,
    pub death_cert_no: String,
    pub unknown_identity: bool,
    pub authority_name: String,
    pub case_number: String,
    pub notes: String,
    /// FR-UGY-02: kept out of the public grave search at the family's request
    #[serde(default)]
    pub hide_public: bool,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Person {
    pub id: String,
    pub name_prefix: String,
    pub family_name: String,
    pub given_name: String,
    pub address: String,
    pub mail_address: String,
    pub phone: String,
    pub email: String,
    pub notes: String,
    /// true when personal fields were hidden for the current user (FR-JOG-04)
    pub masked: bool,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct InterEvent {
    pub id: String,
    pub cemetery_id: String,
    pub scope_key: String,
    pub folio: i64,
    pub historical: bool,
    pub event_type: String,
    pub container: String,
    pub event_date: String,
    pub deceased_id: String,
    pub plot_id: Option<String>,
    pub crypt_slot_id: Option<String>,
    pub arranger_id: Option<String>,
    pub receiver_name: String,
    pub receiver_address: String,
    pub destination: String,
    pub source_event_id: Option<String>,
    pub override_reason: Option<String>,
    pub notes: String,
    pub voided_at: Option<String>,
    pub void_reason: Option<String>,
}

/// Event joined with display data (register book row, lists).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EventRow {
    pub id: String,
    pub cemetery_id: String,
    pub cemetery_name: String,
    pub folio: i64,
    pub scope_key: String,
    pub historical: bool,
    pub event_type: String,
    pub container: String,
    pub event_date: String,
    pub deceased_id: String,
    pub deceased_name: String,
    pub birth_name: String,
    pub birth_date: Option<String>,
    pub birth_place: String,
    pub mother_name: String,
    pub last_residence: String,
    pub death_date: Option<String>,
    pub unknown_identity: bool,
    pub authority_name: String,
    pub case_number: String,
    pub plot_id: Option<String>,
    pub plot_identifier: Option<String>,
    pub crypt_slot_no: Option<i32>,
    pub arranger_name: Option<String>,
    pub arranger_address: Option<String>,
    pub receiver_name: String,
    pub destination: String,
    pub notes: String,
    pub voided_at: Option<String>,
    pub void_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Holder {
    pub id: String,
    pub plot_id: String,
    pub person_id: String,
    pub person_name: String,
    pub person_address: String,
    pub role: String,
    pub relation: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub legal_basis: String,
    pub document_ref: String,
    pub plot_identifier: Option<String>,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Lease {
    pub id: String,
    pub plot_id: String,
    pub person_id: Option<String>,
    pub person_name: Option<String>,
    pub kind: String,
    pub start_date: String,
    pub duration_years: i32,
    pub expiry_date: String,
    pub fee_huf: i64,
    pub fee_rate_id: Option<String>,
    pub fee_exempt: bool,
    pub status: String,
    pub cancel_date: Option<String>,
    pub refund_huf: Option<i64>,
    pub related_event_id: Option<String>,
    pub notes: String,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Notice {
    pub id: String,
    pub plot_id: String,
    pub lease_id: Option<String>,
    pub kind: String,
    pub notice_date: String,
    pub result: String,
    pub voided_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FeeRate {
    pub id: String,
    pub cemetery_id: String,
    pub plot_type_id: Option<String>,
    pub fee_kind: String,
    /// net amount (VAT is added on invoicing according to `vat_rate`)
    pub amount_huf: i64,
    pub valid_from: String,
    pub valid_to: Option<String>,
    pub voided_at: Option<String>,
    /// name on invoices; empty = the fee kind's name
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_unit")]
    pub unit: String,
    /// "27", "5", "0", "AAM" (tárgyi adómentes), "TAM" …
    #[serde(default = "default_vat")]
    pub vat_rate: String,
    /// főkönyvi szám for the accounting export
    #[serde(default)]
    pub ledger_code: String,
}

fn default_unit() -> String {
    "db".into()
}

fn default_vat() -> String {
    "27".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Occupant {
    pub event_id: String,
    pub folio: i64,
    pub event_type: String,
    pub container: String,
    pub event_date: String,
    pub deceased_id: String,
    pub deceased_name: String,
    pub birth_date: Option<String>,
    pub death_date: Option<String>,
    pub crypt_slot_no: Option<i32>,
}

/// Everything shown on a plot's detail page.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PlotDetail {
    pub plot: Plot,
    pub plot_type: PlotType,
    pub cemetery_name: String,
    pub section_code: String,
    pub row_number: String,
    pub status: PlotStatus,
    pub rest_period_years: i32,
    pub occupants: Vec<Occupant>,
    pub events: Vec<EventRow>,
    pub leases: Vec<Lease>,
    pub current_lease: Option<Lease>,
    pub holders: Vec<Holder>,
    pub restrictions: Vec<Restriction>,
    pub monuments: Vec<Monument>,
    pub notices: Vec<Notice>,
    pub crypt_slots: Vec<CryptSlot>,
    pub resale_allowed_from: Option<String>,
}

/// New or existing record reference used by composite operations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode")]
pub enum PersonRef {
    #[serde(rename = "existing")]
    Existing { id: String },
    #[serde(rename = "new")]
    New { person: Person },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "mode")]
pub enum DeceasedRef {
    #[serde(rename = "existing")]
    Existing { id: String },
    #[serde(rename = "new")]
    New { deceased: Deceased },
}

/// Lease to create together with a burial (UC-01 step 5, UC-02 step 3).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LeaseChoice {
    pub create: bool,
    /// overrides the computed values when provided
    pub duration_years: Option<i32>,
    pub fee_huf: Option<i64>,
    pub notes: String,
}

/// UC-01 / UC-02 – record a burial-type event.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BurialInput {
    pub event_type: String,
    pub event_date: String,
    pub container: String,
    pub deceased: DeceasedRef,
    pub plot_id: Option<String>,
    pub crypt_slot_id: Option<String>,
    pub arranger: Option<PersonRef>,
    /// rendelkezési jogosult – required when the plot has no primary holder
    pub holder: Option<PersonRef>,
    pub holder_relation: Option<String>,
    #[serde(default)]
    pub lease: LeaseChoice,
    /// paper-book folio for historical entries (FR-IMP-03)
    pub historical_folio: Option<i64>,
    /// justification to override a failed burial check (needs override_checks)
    pub override_reason: Option<String>,
    #[serde(default)]
    pub notes: String,
    /// without a plot (urn taken home, ashes scattered elsewhere): the
    /// cemetery whose register keeps the entry
    #[serde(default)]
    pub cemetery_id: Option<String>,
    /// urn taken home: who received it (JOG-15)
    #[serde(default)]
    pub receiver_name: String,
    #[serde(default)]
    pub receiver_address: String,
    /// ashes scattered elsewhere, urn taken abroad, …
    #[serde(default)]
    pub destination: String,
}

/// Urn release / urn removal / relocation / ash scattering from an existing placement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalInput {
    pub event_type: String,
    pub event_date: String,
    pub source_event_id: String,
    #[serde(default)]
    pub receiver_name: String,
    #[serde(default)]
    pub receiver_address: String,
    #[serde(default)]
    pub destination: String,
    /// target plot for relocation inside the cemetery
    pub target_plot_id: Option<String>,
    pub historical_folio: Option<i64>,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LeaseProposal {
    pub kind: String,
    pub start_date: String,
    pub duration_years: i32,
    pub expiry_date: String,
    pub fee_huf: i64,
    pub fee_rate_id: Option<String>,
    pub fee_exempt: bool,
    pub extension_days: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BurialPreview {
    pub verdict: Verdict,
    pub status: PlotStatus,
    pub has_primary_holder: bool,
    pub lease: Option<LeaseProposal>,
    pub crypt_free_slots: Vec<CryptSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BurialResult {
    pub event: InterEvent,
    pub folio: i64,
    pub lease: Option<Lease>,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct NewLeaseInput {
    pub plot_id: String,
    pub person: Option<PersonRef>,
    /// purchase | prepurchase | renewal
    pub kind: String,
    pub start_date: String,
    pub duration_years: Option<i32>,
    pub fee_huf: Option<i64>,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct HolderChangeInput {
    pub plot_id: String,
    pub person: Option<PersonRef>,
    pub role: String,
    pub relation: String,
    pub change_date: String,
    /// öröklés, átruházás, megváltás ...
    pub legal_basis: String,
    pub document_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExpiryRow {
    pub plot_id: String,
    pub identifier: String,
    pub cemetery_id: String,
    pub cemetery_name: String,
    pub section_code: String,
    pub type_name: String,
    pub status: PlotStatus,
    pub lease_id: String,
    pub lease_expiry: String,
    pub days_left: i64,
    pub holder_id: Option<String>,
    pub holder_name: Option<String>,
    pub holder_address: Option<String>,
    pub occupant_names: Option<String>,
    pub last_notice: Option<String>,
    pub resale_allowed_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ExpiryFilter {
    pub cemetery_id: Option<String>,
    pub section_id: Option<String>,
    /// lease expiring within this many days (0 = only already expired)
    pub within_days: i64,
    pub include_expired: bool,
    pub only_not_notified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// deceased | person | plot
    pub kind: String,
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub plot_id: Option<String>,
    pub plot_identifier: Option<String>,
    pub cemetery_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PlotFilter {
    pub cemetery_id: Option<String>,
    pub section_id: Option<String>,
    pub row_id: Option<String>,
    pub plot_type_id: Option<String>,
    pub statuses: Vec<String>,
    pub expiry_from: Option<String>,
    pub expiry_to: Option<String>,
    pub burial_from: Option<String>,
    pub burial_to: Option<String>,
    pub identifier_prefix: Option<String>,
    pub protected_only: bool,
    /// map mode: skip names and holders
    pub light: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct EventFilter {
    pub cemetery_id: Option<String>,
    pub section_id: Option<String>,
    pub event_types: Vec<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub folio_from: Option<i64>,
    pub folio_to: Option<i64>,
    pub crypts_only: bool,
    pub include_voided: bool,
    pub query: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    /// "folio" (default) or "date_desc"
    pub order: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub role: String,
    pub extra_permissions: Vec<String>,
    pub active: bool,
    pub locked_until: Option<String>,
    pub must_change_password: bool,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DashboardStats {
    pub plots_total: i64,
    pub by_status: Vec<(String, i64)>,
    pub events_this_year: i64,
    pub expiring_90: i64,
    pub expired: i64,
    pub deceased_total: i64,
    pub recent_events: Vec<EventRow>,
}
