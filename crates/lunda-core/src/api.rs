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

pub fn dispatch(store: &Store, s: &Session, method: &str, a: &Value) -> Result<Value> {
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
        "leases.create" => ok(store.create_lease(s, &arg(a, "input")?)?),
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
        "audit.verify" => ok(audit::verify_chain(&store.conn)?),
        "sample.load" => {
            s.require(Permission::ManageSettings)?;
            ok(crate::sample::load_sample(store, s)?)
        }
        "session.permissions" => ok(json!({ "role": s.role, "permissions": s.permissions })),
        _ => Err(CoreError::new("unknown_method", format!("unknown method {method}")).with("method", method)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
