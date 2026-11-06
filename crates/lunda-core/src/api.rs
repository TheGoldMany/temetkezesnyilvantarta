//! Single JSON dispatcher used by the desktop shell (`invoke("api", {method, args})`).
//! Keeping it in the core makes every UI operation testable without a webview.

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::audit;
use crate::auth::{Permission, Session};
use crate::error::{CoreError, Result};
use crate::settings;
use crate::store::Store;

fn arg<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T> {
    let v = args.get(key).cloned().unwrap_or(Value::Null);
    serde_json::from_value(v).map_err(|e| CoreError::new("bad_request", format!("argument `{key}`: {e}")).with("arg", key))
}

fn opt<T: DeserializeOwned>(args: &Value, key: &str) -> Result<Option<T>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|e| CoreError::new("bad_request", format!("argument `{key}`: {e}"))),
    }
}

fn ok<T: serde::Serialize>(v: T) -> Result<Value> {
    Ok(serde_json::to_value(v)?)
}

/// What an invited funeral office may call: the shared calendar and what it
/// needs (cemeteries, facilities, choosing a plot). Nothing of the register.
const UNDERTAKER_METHODS: &[&str] = &[
    "cemeteries.list",
    "facilities.list",
    "operations.agenda",
    "bookings.list",
    "bookings.get",
    "bookings.save",
    "bookings.status",
    "plots.list",
    "settings.all",
    "session.permissions",
];

pub fn dispatch(store: &Store, s: &Session, method: &str, a: &Value) -> Result<Value> {
    if s.role == crate::auth::Role::Undertaker {
        if !UNDERTAKER_METHODS.contains(&method) {
            return Err(CoreError::forbidden("calendar"));
        }
        if method == "plots.list" {
            // identifiers and types only: no holders for an outside office
            let mut page = store.plot_summaries(&arg(a, "filter")?)?;
            for p in &mut page.items {
                p.holder_name = None;
            }
            return ok(page);
        }
    }
    match method {
        // ---- structure
        "cemeteries.list" => ok(store.list_cemeteries(opt(a, "includeVoided")?.unwrap_or(false))?),
        "cemeteries.save" => ok(store.save_cemetery(s, &arg(a, "cemetery")?)?),
        "plotTypes.list" => ok(store.list_plot_types()?),
        "plotTypes.save" => ok(store.save_plot_type(s, &arg(a, "plotType")?)?),
        "sections.list" => ok(store.list_sections(&arg::<String>(a, "cemeteryId")?)?),
        "rows.list" => ok(store.list_rows(&arg::<String>(a, "sectionId")?)?),
        "structure.bulkCreate" => ok(store.bulk_create(s, &arg(a, "input")?)?),
        "plots.list" => ok(store.plot_summaries(&arg(a, "filter")?)?),
        "plots.get" => ok(store.plot_detail(s, &arg::<String>(a, "id")?)?),
        "plots.find" => ok(store.find_plot(&arg::<String>(a, "cemeteryId")?, &arg::<String>(a, "identifier")?)?),
        "plots.ensure" => {
            let ident: String = arg(a, "identifier")?;
            let parts: Vec<&str> = ident.split('/').map(str::trim).collect();
            if parts.len() != 3 || parts.iter().any(|p| p.is_empty()) {
                return Err(CoreError::validation("plotIdentifier", "format: parcella/sor/hely").with("code", "invalid_identifier"));
            }
            let type_id = match opt::<String>(a, "plotTypeId")? {
                Some(t) if !t.is_empty() => t,
                _ => store.get_plot_type("single")?.id,
            };
            ok(store.ensure_plot(s, &arg::<String>(a, "cemeteryId")?, parts[0], parts[1], parts[2], &type_id)?)
        }
        "plots.update" => ok(store.update_plot(s, &arg(a, "plot")?, opt::<String>(a, "reason")?.as_deref())?),
        "plots.geometry" => {
            let items: Vec<(String, f64, f64, f64, f64, f64)> = arg(a, "items")?;
            store.update_geometry(s, &arg::<String>(a, "entity")?, &items)?;
            ok(true)
        }
        "plots.merge" => ok(store.merge_plots(s, &arg::<String>(a, "targetId")?, &arg::<String>(a, "absorbedId")?, opt::<String>(a, "reason")?.as_deref())?),
        "plots.markResellable" => ok(store.mark_resellable(s, &arg::<String>(a, "plotId")?, opt::<String>(a, "reason")?.as_deref())?),
        "records.void" => {
            store.void_record(s, &arg::<String>(a, "entity")?, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?;
            ok(true)
        }
        "restrictions.add" => ok(store.add_restriction(s, &arg(a, "restriction")?)?),
        "monuments.add" => ok(store.add_monument(s, &arg(a, "monument")?)?),
        "cryptSlots.list" => ok(store.crypt_slots(&arg::<String>(a, "plotId")?)?),

        // ---- people
        "deceased.get" => ok(store.get_deceased(&arg::<String>(a, "id")?)?),
        "deceased.update" => ok(store.update_deceased(s, &arg(a, "deceased")?, opt::<String>(a, "reason")?.as_deref())?),
        "deceased.similar" => ok(store.similar_deceased(
            &arg::<String>(a, "familyName")?,
            &opt::<String>(a, "givenName")?.unwrap_or_default(),
            opt::<String>(a, "birthDate")?.as_deref(),
        )?),
        "deceased.events" => ok(store.deceased_events(&arg::<String>(a, "id")?)?),
        "persons.list" => ok(store.list_persons(s, &opt::<String>(a, "query")?.unwrap_or_default(), opt(a, "limit")?.unwrap_or(100))?),
        "persons.get" => ok(store.get_person(s, &arg::<String>(a, "id")?)?),
        "persons.save" => ok(store.save_person(s, &arg(a, "person")?)?),
        "persons.plots" => ok(store.person_plots(&arg::<String>(a, "id")?)?),
        "holders.change" => ok(store.change_holder(s, &arg(a, "input")?)?),

        // ---- events
        "burial.preview" => ok(store.preview_burial(&arg::<String>(a, "plotId")?, &arg::<String>(a, "eventDate")?, &arg::<String>(a, "container")?)?),
        "burial.record" => ok(store.record_burial(s, &arg(a, "input")?)?),
        "removal.record" => ok(store.record_removal(s, &arg(a, "input")?)?),
        "events.void" => ok(store.void_event(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?),
        "events.correct" => ok(store.correct_event(s, &arg(a, "event")?, opt::<String>(a, "reason")?.as_deref())?),
        "events.list" => ok(store.list_events(&arg(a, "filter")?)?),
        "events.raw" => ok(store.get_event(&arg::<String>(a, "id")?)?),
        "events.get" => ok(store.get_event_row(&arg::<String>(a, "id")?)?),

        // ---- leases
        "leases.propose" => ok(store.propose_lease(&arg::<String>(a, "plotId")?, &arg::<String>(a, "kind")?, opt::<String>(a, "start")?.as_deref())?),
        "leases.create" => {
            let lease = store.create_lease(s, &arg(a, "input")?)?;
            // FR-PEN-02: the fee becomes an item to invoice
            if s.can(Permission::RecordPayments) {
                let cem: String = store.conn.query_row("SELECT cemetery_id FROM plots WHERE id = ?1", crate::params![lease.plot_id], |r| r.get(0))?;
                store.charge_for_lease(s, &lease, &cem)?;
            }
            ok(lease)
        }
        "leases.cancel" => ok(store.cancel_lease(s, &arg::<String>(a, "id")?, &arg::<String>(a, "cancelDate")?, opt::<String>(a, "reason")?.as_deref())?),
        "leases.void" => {
            store.void_lease(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?;
            ok(true)
        }
        "feeRates.list" => ok(store.list_fee_rates(&arg::<String>(a, "cemeteryId")?)?),
        "feeRates.add" => ok(store.add_fee_rate(s, &arg(a, "rate")?)?),
        "expiries.list" => ok(store.expiries(s, &arg(a, "filter")?)?),
        "notices.record" => ok(store.record_notices(
            s,
            &arg::<Vec<String>>(a, "plotIds")?,
            &arg::<String>(a, "kind")?,
            &arg::<String>(a, "noticeDate")?,
            &opt::<String>(a, "result")?.unwrap_or_default(),
        )?),

        // ---- search & reports
        "search.quick" => ok(store.quick_search(s, &arg::<String>(a, "query")?, opt(a, "limit")?.unwrap_or(30))?),
        "map.data" => ok(store.map_data(&arg::<String>(a, "cemeteryId")?)?),
        "dashboard" => ok(store.dashboard(opt::<String>(a, "cemeteryId")?.as_deref())?),
        "reports.utilization" => ok(store.utilization(opt::<String>(a, "cemeteryId")?.as_deref())?),
        "reports.yearly" => ok(store.yearly_stats(opt::<String>(a, "cemeteryId")?.as_deref())?),

        // ---- settings, users, audit
        "settings.all" => ok(settings::all(&store.conn, opt::<String>(a, "cemeteryId")?.as_deref())?),
        "settings.set" => {
            settings::set_many(&store.conn, s, opt::<String>(a, "cemeteryId")?.as_deref(), &arg(a, "values")?)?;
            ok(true)
        }
        "users.list" => ok(store.list_users(s)?),
        "users.create" => ok(store.create_user(
            s,
            &arg::<String>(a, "username")?,
            &arg::<String>(a, "displayName")?,
            &arg::<String>(a, "role")?,
            &opt::<Vec<String>>(a, "extraPermissions")?.unwrap_or_default(),
            &arg::<String>(a, "password")?,
        )?),
        "users.update" => ok(store.update_user(
            s,
            &arg::<String>(a, "id")?,
            &arg::<String>(a, "displayName")?,
            &arg::<String>(a, "role")?,
            &opt::<Vec<String>>(a, "extraPermissions")?.unwrap_or_default(),
            arg(a, "active")?,
        )?),
        "users.unlock" => {
            store.unlock_user(s, &arg::<String>(a, "id")?)?;
            ok(true)
        }
        "users.changePassword" => {
            store.change_password(s, &arg::<String>(a, "id")?, opt::<String>(a, "oldPassword")?.as_deref(), &arg::<String>(a, "newPassword")?)?;
            ok(true)
        }
        "audit.list" => {
            s.require(Permission::Export).or_else(|_| s.require(Permission::ManageSettings))?;
            ok(audit::list(
                &store.conn,
                opt::<String>(a, "entity")?.as_deref(),
                opt::<String>(a, "entityId")?.as_deref(),
                opt(a, "limit")?.unwrap_or(100),
                opt(a, "beforeSeq")?,
            )?)
        }
        // ---- finance (W7)
        "finance.status" => ok(store.finance_status()?),
        "finance.setAgentKey" => {
            store.set_szamlazz_key(s, &arg::<String>(a, "key")?)?;
            ok(store.finance_status()?)
        }
        "charges.list" => {
            ok(store.list_charges(&arg::<String>(a, "cemeteryId")?, opt::<String>(a, "status")?.as_deref(), opt::<String>(a, "personId")?.as_deref())?)
        }
        "charges.add" => ok(store.add_charge(s, &arg(a, "charge")?)?),
        "charges.void" => {
            store.void_charge(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?;
            ok(true)
        }
        "invoices.list" => ok(store.list_invoices(
            &arg::<String>(a, "cemeteryId")?,
            opt::<String>(a, "status")?.as_deref(),
            opt::<bool>(a, "overdueOnly")?.unwrap_or(false),
        )?),
        "invoices.get" => {
            let id = arg::<String>(a, "id")?;
            ok(json!({ "invoice": store.get_invoice(&id)?, "lines": store.invoice_lines(&id)? }))
        }
        "invoices.create" => ok(store.create_invoice(s, &arg(a, "invoice")?)?),
        "invoices.aspNumber" => ok(store.record_asp_number(s, &arg::<String>(a, "id")?, &arg::<String>(a, "number")?)?),
        "invoices.storno" => ok(store.storno_invoice(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?),
        "invoices.reminder" => ok(store.record_reminder(s, &arg::<String>(a, "id")?, &arg::<String>(a, "channel")?)?),
        "invoices.pdf" => {
            s.require(Permission::Print)?;
            let (number, pdf) = store.invoice_pdf(&arg::<String>(a, "id")?)?;
            use base64::Engine;
            ok(
                json!({ "fileName": format!("{}.pdf", number.replace('/', "-")), "path": "", "pages": 1, "sha256": "", "data": base64::engine::general_purpose::STANDARD.encode(pdf) }),
            )
        }
        "payments.list" => ok(store.list_payments(&arg::<String>(a, "cemeteryId")?, &arg::<String>(a, "from")?, &arg::<String>(a, "to")?)?),
        "payments.record" => ok(store.record_payment(s, &arg(a, "payment")?)?),
        "payments.void" => {
            store.void_payment(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?;
            ok(true)
        }
        "cash.day" => ok(store.cash_day(s, &arg::<String>(a, "cemeteryId")?, &arg::<String>(a, "date")?)?),
        "cash.add" => ok(store.add_cash_entry(
            s,
            &arg::<String>(a, "cemeteryId")?,
            opt::<String>(a, "date")?.as_deref(),
            &arg::<String>(a, "kind")?,
            arg::<i64>(a, "amountHuf")?,
            &arg::<String>(a, "partnerName")?,
            &arg::<String>(a, "description")?,
        )?),
        "cash.close" => ok(store.close_cash_day(
            s,
            &arg::<String>(a, "cemeteryId")?,
            &arg::<String>(a, "date")?,
            arg::<i64>(a, "countedHuf")?,
            &opt::<String>(a, "note")?.unwrap_or_default(),
        )?),
        "bank.import" => ok(store.import_bank_csv(s, &arg::<String>(a, "cemeteryId")?, &arg::<String>(a, "fileName")?, &arg::<String>(a, "content")?)?),
        "bank.list" => ok(store.list_bank(&arg::<String>(a, "cemeteryId")?, opt::<String>(a, "status")?.as_deref())?),
        "bank.match" => {
            store.match_bank(s, &arg::<String>(a, "id")?, opt::<String>(a, "invoiceId")?.as_deref())?;
            ok(true)
        }
        "receivables.list" => ok(store.receivables(&arg::<String>(a, "cemeteryId")?)?),
        "finance.revenue" => {
            s.require(Permission::RecordPayments)?;
            ok(store.revenue_report(opt::<String>(a, "cemeteryId")?.as_deref(), &arg::<String>(a, "from")?, &arg::<String>(a, "to")?)?)
        }
        // ---- operations (W8–W11)
        "facilities.list" => ok(store.list_facilities(&arg::<String>(a, "cemeteryId")?)?),
        "facilities.save" => ok(store.save_facility(s, &arg(a, "facility")?)?),
        "facilities.void" => {
            store.void_facility(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?;
            ok(true)
        }
        "contractors.list" => ok(store.list_contractors(opt(a, "includeVoided")?.unwrap_or(false))?),
        "contractors.save" => ok(store.save_contractor(s, &arg(a, "contractor")?)?),
        "contractors.account" => ok(store.contractor_account(s, &arg::<String>(a, "id")?)?),
        "contractors.void" => {
            store.void_contractor(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?;
            ok(true)
        }
        "bookings.list" => ok(store.list_bookings(&arg::<String>(a, "cemeteryId")?, &arg::<String>(a, "from")?, &arg::<String>(a, "to")?)?),
        "bookings.get" => ok(store.get_booking(&arg::<String>(a, "id")?)?),
        "bookings.save" => ok(store.save_booking(s, &arg(a, "booking")?)?),
        "bookings.status" => ok(store.set_booking_status(
            s,
            &arg::<String>(a, "id")?,
            &arg::<String>(a, "status")?,
            opt::<String>(a, "reason")?.as_deref(),
            opt::<String>(a, "eventId")?.as_deref(),
        )?),
        "workNotices.list" => ok(store.list_work_notices(&arg::<String>(a, "cemeteryId")?, &arg::<String>(a, "from")?, &arg::<String>(a, "to")?)?),
        "workNotices.add" => ok(store.add_work_notice(s, &arg(a, "notice")?)?),
        "workOrders.list" => {
            ok(store.list_work_orders(&arg::<String>(a, "cemeteryId")?, opt::<String>(a, "status")?.as_deref(), opt::<String>(a, "assignee")?.as_deref())?)
        }
        "workOrders.save" => ok(store.save_work_order(s, &arg(a, "order")?)?),
        "workOrders.complete" => ok(store.complete_work_order(s, &arg::<String>(a, "id")?, &opt::<String>(a, "note")?.unwrap_or_default())?),
        "workOrders.cancel" => ok(store.cancel_work_order(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?),
        "cases.list" => {
            ok(store.list_cases(&arg::<String>(a, "cemeteryId")?, opt::<String>(a, "status")?.as_deref(), opt::<String>(a, "plotId")?.as_deref())?)
        }
        "cases.get" => ok(store.case_detail(&arg::<String>(a, "id")?)?),
        "cases.create" => ok(store.create_case(s, &arg(a, "case")?)?),
        "cases.update" => ok(store.update_case(s, &arg::<String>(a, "id")?, &arg(a, "update")?)?),
        "cases.void" => {
            store.void_case(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?;
            ok(true)
        }
        "attachments.list" => ok(store.list_attachments(&arg::<String>(a, "entity")?, &arg::<String>(a, "entityId")?)?),
        "attachments.add" => ok(store.add_attachment(
            s,
            &arg::<String>(a, "entity")?,
            &arg::<String>(a, "entityId")?,
            &arg::<String>(a, "fileName")?,
            &opt::<String>(a, "mimeType")?.unwrap_or_default(),
            &arg::<String>(a, "content")?,
        )?),
        "attachments.get" => {
            let (name, mime, data) = store.get_attachment(&arg::<String>(a, "id")?)?;
            use base64::Engine;
            ok(
                json!({ "fileName": name, "mimeType": mime, "path": "", "pages": 0, "sha256": "", "data": base64::engine::general_purpose::STANDARD.encode(data) }),
            )
        }
        "attachments.void" => {
            store.void_attachment(s, &arg::<String>(a, "id")?, opt::<String>(a, "reason")?.as_deref())?;
            ok(true)
        }
        "conditions.record" => ok(store.record_condition(s, &arg(a, "condition")?)?),
        "conditions.list" => ok(store.list_conditions(&arg::<String>(a, "cemeteryId")?, opt(a, "problemsOnly")?.unwrap_or(false))?),
        "conditions.plot" => ok(store.plot_conditions(&arg::<String>(a, "plotId")?)?),
        "operations.agenda" => ok(store.agenda(s, &arg::<String>(a, "cemeteryId")?, &arg::<String>(a, "from")?, &arg::<String>(a, "to")?)?),
        // ---- maintainer reports, notices, public search (W12)
        "reports.annual" => ok(store.annual_report(s, opt::<String>(a, "cemeteryId")?.as_deref(), arg::<i32>(a, "year")?)?),
        "reports.capacity" => ok(store.capacity(opt::<String>(a, "cemeteryId")?.as_deref())?),
        "reminders.status" => ok(json!({
            "enabled": store.auto_reminders_enabled()?,
            "due": store.auto_reminder_plots()?.len(),
            "unanswered": store.unanswered_reminders()?,
        })),
        "notices.emailPrepare" => ok(store.expiry_mails(s, &arg::<Vec<String>>(a, "plotIds")?)?),
        "invoices.reminderMail" => ok(store.reminder_mail(s, &arg::<String>(a, "id")?)?),
        "plots.setGps" => {
            store.set_plot_gps(s, &arg::<String>(a, "id")?, opt::<f64>(a, "lat")?, opt::<f64>(a, "lon")?)?;
            ok(true)
        }
        "persons.dataExport" => ok(store.person_data_export(s, &arg::<String>(a, "id")?)?),
        "deceased.setPublic" => {
            store.set_deceased_public(s, &arg::<String>(a, "id")?, arg::<bool>(a, "hidden")?)?;
            ok(true)
        }
        "audit.verify" => ok(audit::verify_chain(&store.conn)?),
        "sample.load" => {
            s.require(Permission::ManageSettings)?;
            let r = crate::sample::load_sample(store, s)?;
            crate::db::analyze(&store.conn)?;
            ok(r)
        }
        "session.permissions" => ok(json!({ "role": s.role, "permissions": s.permissions })),
        _ => Err(CoreError::new("unknown_method", format!("unknown method {method}")).with("method", method)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invited_office_reaches_only_the_calendar() {
        let store = Store::in_memory().unwrap();
        let sys = Session::system();
        crate::sample::load_sample(&store, &sys).unwrap();
        let office = Session::for_role(None, "iroda@pelda.hu", "Kegyelet Temetkezés", crate::auth::Role::Undertaker, &[]);
        let cem = dispatch(&store, &office, "cemeteries.list", &json!({})).unwrap()[0]["id"].as_str().unwrap().to_string();
        for m in ["events.list", "persons.list", "search.quick", "deceased.get", "expiries.list", "reports.annual", "charges.list", "cases.list"] {
            assert_eq!(dispatch(&store, &office, m, &json!({ "cemeteryId": cem })).unwrap_err().code, "forbidden", "{m}");
        }
        assert_eq!(
            crate::app::call(&store, &office, "print.document", &json!({ "doc": "register" }), std::path::Path::new("/tmp")).unwrap_err().code,
            "forbidden"
        );
        let plots = dispatch(&store, &office, "plots.list", &json!({ "filter": { "cemeteryId": cem, "limit": 300 } })).unwrap();
        assert!(plots["items"].as_array().unwrap().iter().all(|p| p["holderName"].is_null()));
        let all = dispatch(&store, &sys, "plots.list", &json!({ "filter": { "cemeteryId": cem, "limit": 300 } })).unwrap();
        assert!(all["items"].as_array().unwrap().iter().any(|p| p["holderName"].is_string()));
        dispatch(&store, &office, "operations.agenda", &json!({ "cemeteryId": cem, "from": "2026-12-01", "to": "2026-12-07" })).unwrap();
    }

    #[test]
    fn dispatch_roundtrip() {
        let store = Store::in_memory().unwrap();
        let s = Session::system();
        let c = dispatch(&store, &s, "cemeteries.save", &json!({"cemetery": {"name": "Próba temető"}})).unwrap();
        let id = c["id"].as_str().unwrap().to_string();
        let list = dispatch(&store, &s, "cemeteries.list", &json!({})).unwrap();
        assert_eq!(list[0]["name"], "Próba temető");
        let types = dispatch(&store, &s, "plotTypes.list", &json!({})).unwrap();
        let single = types.as_array().unwrap().iter().find(|t| t["code"] == "single").unwrap()["id"].clone();
        let r = dispatch(
            &store,
            &s,
            "structure.bulkCreate",
            &json!({"input": {
                "cemeteryId": id, "sectionCode": "B", "rowFrom": 1, "rowTo": 2, "plotsPerRow": 3, "firstPlotNumber": 1, "plotTypeId": single
            }}),
        )
        .unwrap();
        assert_eq!(r["plotsCreated"], 6);
        let e = dispatch(&store, &s, "nope", &json!({})).unwrap_err();
        assert_eq!(e.code, "unknown_method");
        let e = dispatch(&store, &s, "plots.get", &json!({})).unwrap_err();
        assert_eq!(e.code, "bad_request");
    }
}
