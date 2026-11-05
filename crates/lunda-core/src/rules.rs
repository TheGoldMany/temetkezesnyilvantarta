//! Pure business rules (no database access) – fully unit tested.
//!
//! * lease expiry (FR-MEG-01)
//! * extension on reburial and pro-rata fee (FR-MEG-03, QUESTIONS Q4)
//! * pro-rata refund on cancellation (FR-MEG-04)
//! * burial permission check (FR-ELH-04, JOG-06)
//! * computed plot status (FR-TEM-05)

use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Adds calendar years. 29 February maps to 28 February in non-leap years.
pub fn add_years(date: NaiveDate, years: i32) -> NaiveDate {
    let y = date.year() + years;
    NaiveDate::from_ymd_opt(y, date.month(), date.day())
        .or_else(|| NaiveDate::from_ymd_opt(y, date.month(), date.day() - 1))
        .expect("valid date")
}

/// lejárat = kezdet + időtartam
pub fn lease_expiry(start: NaiveDate, duration_years: i32) -> NaiveDate {
    add_years(start, duration_years)
}

/// Rounds a fraction of an amount to whole forints (half away from zero).
pub fn prorate(amount_huf: i64, numerator_days: i64, denominator_days: i64) -> i64 {
    if denominator_days <= 0 || numerator_days <= 0 || amount_huf == 0 {
        return 0;
    }
    let num = amount_huf as i128 * numerator_days as i128;
    let den = denominator_days as i128;
    ((num * 2 + den) / (den * 2)) as i64
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Extension {
    /// New expiry date (unchanged if no extension is needed).
    pub new_expiry: NaiveDate,
    /// Number of days added.
    pub extension_days: i64,
    /// Pro-rata fee for the extension in HUF.
    pub fee_huf: i64,
}

/// Extension of the right of use on reburial (FR-MEG-03, Q4):
/// new expiry = max(current expiry, burial date + rest period);
/// fee = extension days / days of a full standard lease starting on the burial date × current rate.
pub fn extension_on_reburial(
    current_expiry: Option<NaiveDate>,
    burial_date: NaiveDate,
    rest_period_years: i32,
    standard_lease_years: i32,
    current_rate_huf: i64,
    fee_exempt: bool,
) -> Extension {
    let required = add_years(burial_date, rest_period_years);
    let base = current_expiry.unwrap_or(burial_date).max(burial_date);
    if required <= base {
        return Extension { new_expiry: current_expiry.unwrap_or(base).max(base), extension_days: 0, fee_huf: 0 };
    }
    let ext_days = (required - base).num_days();
    let full_days = (add_years(burial_date, standard_lease_years.max(1)) - burial_date).num_days();
    let fee = if fee_exempt { 0 } else { prorate(current_rate_huf, ext_days, full_days) };
    Extension { new_expiry: required, extension_days: ext_days, fee_huf: fee }
}

/// Pro-rata refund when the right of use is renounced (FR-MEG-04).
/// Based on the fee paid at purchase: fee × remaining days / total days.
pub fn pro_rata_refund(start: NaiveDate, expiry: NaiveDate, cancel_date: NaiveDate, fee_paid_huf: i64) -> i64 {
    if cancel_date >= expiry || expiry <= start {
        return 0;
    }
    let effective = cancel_date.max(start);
    prorate(fee_paid_huf, (expiry - effective).num_days(), (expiry - start).num_days())
}

// ---------------------------------------------------------------------------
// Burial check
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    Coffin,
    Urn,
    None,
}

impl Container {
    pub fn as_str(&self) -> &'static str {
        match self {
            Container::Coffin => "coffin",
            Container::Urn => "urn",
            Container::None => "none",
        }
    }
    pub fn parse(s: &str) -> Self {
        match s {
            "urn" => Container::Urn,
            "none" => Container::None,
            _ => Container::Coffin,
        }
    }
}

/// A reason in a verdict; `code` maps to `burialCheck.<code>` in the i18n file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    pub code: String,
    pub params: BTreeMap<String, String>,
}

impl Reason {
    fn new(code: &str) -> Self {
        Self { code: code.into(), params: BTreeMap::new() }
    }
    fn p(mut self, k: &str, v: impl ToString) -> Self {
        self.params.insert(k.into(), v.to_string());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Verdict {
    pub allowed: bool,
    pub blockers: Vec<Reason>,
    pub warnings: Vec<Reason>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRestriction {
    pub kind: String, // "ban" | "condition"
    pub text: String,
}

/// Everything needed to decide whether a burial may take place.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BurialContext {
    pub burial_date: NaiveDate,
    pub container: Container,
    pub plot_category: String, // grave, crypt, urn, scatter, common
    pub coffin_capacity: i32,
    pub urn_capacity: i32,
    pub coffins_present: i32,
    pub urns_present: i32,
    /// Date of the latest coffin burial still in the plot.
    pub last_coffin_burial: Option<NaiveDate>,
    pub rest_period_years: i32,
    pub restrictions: Vec<ActiveRestriction>,
    pub unusable: bool,
    pub further_burial_allowed: bool,
    pub status: PlotStatus,
    pub lease_expiry: Option<NaiveDate>,
    pub protected: bool,
}

/// FR-ELH-04 / UC-02: decides whether a burial may take place and why not.
///
/// Rules (see QUESTIONS Q3):
/// * unusable, liquidated or resellable plots are blocked;
/// * an active "ban" restriction blocks (JOG-06); "condition"s are warnings;
/// * protected plots without "further burial allowed" are blocked (FR-EML-03);
/// * a coffin needs free coffin capacity, OR – when the plot is full – the
///   rest period of the latest coffin burial must have elapsed (reopening);
/// * an urn needs free urn capacity (urns do not disturb the rest period);
/// * scatter areas accept only urns/ashes, urn plots only urns;
/// * an expired lease is a warning (renewal is offered).
pub fn check_burial(ctx: &BurialContext) -> Verdict {
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();

    if ctx.unusable || ctx.status == PlotStatus::Unusable {
        blockers.push(Reason::new("plot_unusable"));
    }
    if matches!(ctx.status, PlotStatus::Liquidated | PlotStatus::Resellable) {
        blockers.push(Reason::new("plot_not_available").p("status", ctx.status.as_str()));
    }
    for r in &ctx.restrictions {
        if r.kind == "ban" {
            blockers.push(Reason::new("burial_ban").p("text", &r.text));
        } else {
            warnings.push(Reason::new("burial_condition").p("text", &r.text));
        }
    }
    if ctx.protected && !ctx.further_burial_allowed {
        blockers.push(Reason::new("protected_no_further_burial"));
    } else if !ctx.further_burial_allowed {
        blockers.push(Reason::new("further_burial_not_allowed"));
    }

    match ctx.container {
        Container::Coffin => {
            if matches!(ctx.plot_category.as_str(), "urn" | "scatter") {
                blockers.push(Reason::new("coffin_not_allowed_in_type"));
            } else if ctx.plot_category != "common" && ctx.coffins_present >= ctx.coffin_capacity {
                // full: reopening only after the rest period
                match ctx.last_coffin_burial {
                    Some(last) => {
                        let until = add_years(last, ctx.rest_period_years);
                        if ctx.burial_date < until {
                            blockers.push(
                                Reason::new("rest_period_not_elapsed")
                                    .p("until", until.format("%Y-%m-%d"))
                                    .p("last", last.format("%Y-%m-%d")),
                            );
                        } else {
                            warnings.push(Reason::new("reopening_after_rest_period"));
                        }
                    }
                    None => {
                        if ctx.coffin_capacity <= 0 {
                            blockers.push(Reason::new("no_coffin_capacity"));
                        } else {
                            blockers.push(Reason::new("coffin_capacity_full").p("capacity", ctx.coffin_capacity));
                        }
                    }
                }
            }
        }
        Container::Urn => {
            if ctx.plot_category != "scatter"
                && ctx.plot_category != "common"
                && ctx.urns_present >= ctx.urn_capacity
            {
                blockers.push(Reason::new("urn_capacity_full").p("capacity", ctx.urn_capacity));
            }
        }
        Container::None => {}
    }

    if let Some(exp) = ctx.lease_expiry {
        if exp < ctx.burial_date && !ctx.protected {
            warnings.push(Reason::new("lease_expired").p("expiry", exp.format("%Y-%m-%d")));
        }
    }

    Verdict { allowed: blockers.is_empty(), blockers, warnings }
}

// ---------------------------------------------------------------------------
// Plot status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlotStatus {
    /// szabad
    Free,
    /// lefoglalt (előre megváltott)
    Reserved,
    /// foglalt (betemetett)
    Occupied,
    /// lejárt (újraváltásra vár)
    Expired,
    /// értesítés kiküldve
    Notified,
    /// újraértékesíthető
    Resellable,
    /// felszámolt
    Liquidated,
    /// nem használható
    Unusable,
}

impl PlotStatus {
    pub const ALL: [PlotStatus; 8] = [
        PlotStatus::Free,
        PlotStatus::Reserved,
        PlotStatus::Occupied,
        PlotStatus::Expired,
        PlotStatus::Notified,
        PlotStatus::Resellable,
        PlotStatus::Liquidated,
        PlotStatus::Unusable,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            PlotStatus::Free => "free",
            PlotStatus::Reserved => "reserved",
            PlotStatus::Occupied => "occupied",
            PlotStatus::Expired => "expired",
            PlotStatus::Notified => "notified",
            PlotStatus::Resellable => "resellable",
            PlotStatus::Liquidated => "liquidated",
            PlotStatus::Unusable => "unusable",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        PlotStatus::ALL.iter().copied().find(|p| p.as_str() == s)
    }
}

#[derive(Debug, Clone, Default)]
pub struct StatusInputs {
    pub status_override: Option<PlotStatus>,
    pub unusable: bool,
    pub protected: bool,
    pub occupants: i32,
    /// expiry and start of the current (latest active) lease
    pub lease_expiry: Option<NaiveDate>,
    pub lease_start: Option<NaiveDate>,
    /// latest notice date
    pub last_notice: Option<NaiveDate>,
    pub resale_ready_at: Option<NaiveDate>,
}

/// Computes the plot status from leases, events and notices (FRS 6. „Számított értékek”).
pub fn compute_status(i: &StatusInputs, today: NaiveDate) -> PlotStatus {
    if let Some(s) = i.status_override {
        return s;
    }
    if i.unusable {
        return PlotStatus::Unusable;
    }
    if let Some(r) = i.resale_ready_at {
        // only valid if no newer lease started after the confirmation
        if i.lease_start.map(|s| s <= r).unwrap_or(true) {
            return PlotStatus::Resellable;
        }
    }
    if i.protected {
        return if i.occupants > 0 { PlotStatus::Occupied } else { PlotStatus::Reserved };
    }
    match i.lease_expiry {
        Some(exp) => {
            let notified = match (i.last_notice, i.lease_start) {
                (Some(n), Some(s)) => n >= s,
                (Some(_), None) => true,
                _ => false,
            };
            if exp < today {
                if notified {
                    PlotStatus::Notified
                } else {
                    PlotStatus::Expired
                }
            } else if notified {
                PlotStatus::Notified
            } else if i.occupants > 0 {
                PlotStatus::Occupied
            } else {
                PlotStatus::Reserved
            }
        }
        None => {
            if i.occupants > 0 {
                PlotStatus::Occupied
            } else {
                PlotStatus::Free
            }
        }
    }
}

/// Date after which an expired, notified plot may be declared resellable (FR-MEG-07).
pub fn resale_allowed_from(lease_expiry: NaiveDate, last_notice: Option<NaiveDate>, grace_days: i64) -> Option<NaiveDate> {
    let notice = last_notice?;
    let base = lease_expiry.max(notice);
    Some(base + chrono::Duration::days(grace_days))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn expiry_adds_years() {
        assert_eq!(lease_expiry(d("2026-03-15"), 25), d("2051-03-15"));
        assert_eq!(lease_expiry(d("2024-02-29"), 25), d("2049-02-28"));
        assert_eq!(lease_expiry(d("2024-02-29"), 4), d("2028-02-29"));
    }

    #[test]
    fn prorate_rounds_half_up() {
        assert_eq!(prorate(100_000, 1, 2), 50_000);
        assert_eq!(prorate(10, 1, 3), 3);
        assert_eq!(prorate(5, 1, 2), 3); // 2.5 -> 3
        assert_eq!(prorate(100, 0, 10), 0);
        assert_eq!(prorate(100, 5, 0), 0);
    }

    #[test]
    fn reburial_extends_to_rest_period() {
        // lease expires 2030-01-01; reburial 2026-01-01 with 25-year rest period
        let e = extension_on_reburial(Some(d("2030-01-01")), d("2026-01-01"), 25, 25, 250_000, false);
        assert_eq!(e.new_expiry, d("2051-01-01"));
        assert_eq!(e.extension_days, (d("2051-01-01") - d("2030-01-01")).num_days());
        let full = (d("2051-01-01") - d("2026-01-01")).num_days();
        assert_eq!(e.fee_huf, prorate(250_000, e.extension_days, full));
        assert!(e.fee_huf > 0 && e.fee_huf < 250_000);
    }

    #[test]
    fn reburial_no_extension_when_lease_long_enough() {
        let e = extension_on_reburial(Some(d("2060-01-01")), d("2026-01-01"), 25, 25, 250_000, false);
        assert_eq!(e.new_expiry, d("2060-01-01"));
        assert_eq!(e.extension_days, 0);
        assert_eq!(e.fee_huf, 0);
    }

    #[test]
    fn reburial_on_expired_lease_charges_full_rest_period() {
        let e = extension_on_reburial(Some(d("2020-01-01")), d("2026-01-01"), 25, 25, 250_000, false);
        assert_eq!(e.new_expiry, d("2051-01-01"));
        assert_eq!(e.fee_huf, 250_000);
    }

    #[test]
    fn reburial_exempt_is_free() {
        let e = extension_on_reburial(Some(d("2030-01-01")), d("2026-01-01"), 25, 25, 250_000, true);
        assert_eq!(e.fee_huf, 0);
        assert_eq!(e.new_expiry, d("2051-01-01"));
    }

    #[test]
    fn refund_is_pro_rata() {
        // half of the period remaining -> half of the fee
        let start = d("2020-01-01");
        let exp = d("2030-01-01");
        let mid = start + chrono::Duration::days((exp - start).num_days() / 2);
        let r = pro_rata_refund(start, exp, mid, 100_000);
        assert!((49_950..=50_050).contains(&r), "{r}");
        assert_eq!(pro_rata_refund(start, exp, exp, 100_000), 0);
        assert_eq!(pro_rata_refund(start, exp, d("2035-01-01"), 100_000), 0);
        assert_eq!(pro_rata_refund(start, exp, d("2019-01-01"), 100_000), 100_000);
    }

    fn ctx() -> BurialContext {
        BurialContext {
            burial_date: d("2026-06-01"),
            container: Container::Coffin,
            plot_category: "grave".into(),
            coffin_capacity: 1,
            urn_capacity: 4,
            coffins_present: 0,
            urns_present: 0,
            last_coffin_burial: None,
            rest_period_years: 25,
            restrictions: vec![],
            unusable: false,
            further_burial_allowed: true,
            status: PlotStatus::Free,
            lease_expiry: None,
            protected: false,
        }
    }

    #[test]
    fn empty_plot_allows_coffin() {
        let v = check_burial(&ctx());
        assert!(v.allowed, "{v:?}");
    }

    #[test]
    fn full_plot_within_rest_period_is_blocked() {
        let mut c = ctx();
        c.coffins_present = 1;
        c.last_coffin_burial = Some(d("2010-05-01"));
        c.status = PlotStatus::Occupied;
        let v = check_burial(&c);
        assert!(!v.allowed);
        assert_eq!(v.blockers[0].code, "rest_period_not_elapsed");
        assert_eq!(v.blockers[0].params["until"], "2035-05-01");
    }

    #[test]
    fn full_plot_after_rest_period_allows_reopening() {
        let mut c = ctx();
        c.coffins_present = 1;
        c.last_coffin_burial = Some(d("2000-05-01"));
        c.status = PlotStatus::Occupied;
        let v = check_burial(&c);
        assert!(v.allowed);
        assert_eq!(v.warnings[0].code, "reopening_after_rest_period");
    }

    #[test]
    fn rest_period_boundary_day() {
        let mut c = ctx();
        c.coffins_present = 1;
        c.last_coffin_burial = Some(d("2001-06-01"));
        c.burial_date = d("2026-05-31");
        assert!(!check_burial(&c).allowed);
        c.burial_date = d("2026-06-01");
        assert!(check_burial(&c).allowed);
    }

    #[test]
    fn double_grave_allows_second_coffin() {
        let mut c = ctx();
        c.coffin_capacity = 2;
        c.coffins_present = 1;
        c.last_coffin_burial = Some(d("2025-05-01"));
        assert!(check_burial(&c).allowed);
    }

    #[test]
    fn urn_into_grave_until_capacity() {
        let mut c = ctx();
        c.container = Container::Urn;
        c.coffins_present = 1;
        c.last_coffin_burial = Some(d("2025-01-01"));
        c.urns_present = 3;
        assert!(check_burial(&c).allowed);
        c.urns_present = 4;
        let v = check_burial(&c);
        assert!(!v.allowed);
        assert_eq!(v.blockers[0].code, "urn_capacity_full");
    }

    #[test]
    fn ban_blocks_condition_warns() {
        let mut c = ctx();
        c.restrictions = vec![
            ActiveRestriction { kind: "condition".into(), text: "csak urna".into() },
            ActiveRestriction { kind: "ban".into(), text: "talajvíz 2 m-en belül".into() },
        ];
        let v = check_burial(&c);
        assert!(!v.allowed);
        assert_eq!(v.blockers[0].code, "burial_ban");
        assert_eq!(v.warnings[0].code, "burial_condition");
    }

    #[test]
    fn coffin_not_allowed_in_urn_niche() {
        let mut c = ctx();
        c.plot_category = "urn".into();
        assert_eq!(check_burial(&c).blockers[0].code, "coffin_not_allowed_in_type");
    }

    #[test]
    fn unusable_resellable_protected_block() {
        let mut c = ctx();
        c.unusable = true;
        assert!(!check_burial(&c).allowed);
        let mut c = ctx();
        c.status = PlotStatus::Resellable;
        assert!(!check_burial(&c).allowed);
        let mut c = ctx();
        c.protected = true;
        c.further_burial_allowed = false;
        assert_eq!(check_burial(&c).blockers[0].code, "protected_no_further_burial");
    }

    #[test]
    fn expired_lease_is_warning() {
        let mut c = ctx();
        c.lease_expiry = Some(d("2020-01-01"));
        let v = check_burial(&c);
        assert!(v.allowed);
        assert_eq!(v.warnings[0].code, "lease_expired");
    }

    #[test]
    fn status_computation() {
        let t = d("2026-06-01");
        let mut i = StatusInputs::default();
        assert_eq!(compute_status(&i, t), PlotStatus::Free);
        i.lease_start = Some(d("2020-01-01"));
        i.lease_expiry = Some(d("2045-01-01"));
        assert_eq!(compute_status(&i, t), PlotStatus::Reserved);
        i.occupants = 1;
        assert_eq!(compute_status(&i, t), PlotStatus::Occupied);
        i.lease_expiry = Some(d("2026-01-01"));
        assert_eq!(compute_status(&i, t), PlotStatus::Expired);
        i.last_notice = Some(d("2026-02-01"));
        assert_eq!(compute_status(&i, t), PlotStatus::Notified);
        i.resale_ready_at = Some(d("2026-05-01"));
        assert_eq!(compute_status(&i, t), PlotStatus::Resellable);
        // new lease after resale confirmation -> no longer resellable
        i.lease_start = Some(d("2026-05-20"));
        i.lease_expiry = Some(d("2051-05-20"));
        i.last_notice = None;
        assert_eq!(compute_status(&i, t), PlotStatus::Occupied);
        i.status_override = Some(PlotStatus::Liquidated);
        assert_eq!(compute_status(&i, t), PlotStatus::Liquidated);
    }

    #[test]
    fn old_notice_does_not_mark_renewed_lease() {
        let t = d("2026-06-01");
        let i = StatusInputs {
            occupants: 1,
            lease_start: Some(d("2026-01-01")),
            lease_expiry: Some(d("2051-01-01")),
            last_notice: Some(d("2025-06-01")),
            ..Default::default()
        };
        assert_eq!(compute_status(&i, t), PlotStatus::Occupied);
    }

    #[test]
    fn protected_never_expires() {
        let i = StatusInputs {
            protected: true,
            occupants: 1,
            lease_expiry: Some(d("2000-01-01")),
            ..Default::default()
        };
        assert_eq!(compute_status(&i, d("2026-01-01")), PlotStatus::Occupied);
    }

    #[test]
    fn resale_date_after_grace() {
        assert_eq!(resale_allowed_from(d("2026-01-01"), None, 180), None);
        assert_eq!(resale_allowed_from(d("2026-01-01"), Some(d("2026-02-01")), 30), Some(d("2026-03-03")));
    }
}
