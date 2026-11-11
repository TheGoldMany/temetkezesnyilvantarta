//! Per-cemetery access: a colleague limited to some of the organisation's
//! cemeteries (`Session::cemeteries`) can neither read nor change anything of
//! the others. Checked once for every API call: the arguments name the
//! cemetery directly (`cemeteryId`) or through a record of it (a plot, a
//! burial, an invoice …); lists in the answer keep only rows of the
//! cemeteries the user works in. People (holders, payers) belong to the whole
//! organisation and stay visible.

use serde_json::Value;

use crate::auth::Session;
use crate::sql::OptionalExtension;
use crate::{CoreError, Result, Store};

/// How a record leads to its cemetery.
fn cemetery_sql(table: &str) -> Option<String> {
    let direct = [
        "cemeteries",
        "sections",
        "plots",
        "interment_events",
        "fee_rates",
        "charges",
        "invoices",
        "payments",
        "cash_entries",
        "bank_transactions",
        "facilities",
        "bookings",
        "work_notices",
        "work_orders",
        "cases",
        "stock_items",
        "stock_moves",
        "stocktakes",
    ];
    if direct.contains(&table) {
        let col = if table == "cemeteries" { "id" } else { "cemetery_id" };
        return Some(format!("SELECT {col} FROM {table} WHERE id = ?1"));
    }
    match table {
        "plot_rows" => Some("SELECT s.cemetery_id FROM plot_rows r JOIN sections s ON s.id = r.section_id WHERE r.id = ?1".into()),
        "leases" | "notices" | "monuments" | "plot_restrictions" | "plot_holders" | "plot_conditions" | "crypt_slots" => {
            Some(format!("SELECT p.cemetery_id FROM {table} x JOIN plots p ON p.id = x.plot_id WHERE x.id = ?1"))
        }
        "invoice_lines" => Some("SELECT i.cemetery_id FROM invoice_lines l JOIN invoices i ON i.id = l.invoice_id WHERE l.id = ?1".into()),
        _ => None,
    }
}

/// The table of a record named by an argument key.
fn table_of_key(key: &str) -> Option<&'static str> {
    Some(match key {
        "plotId" | "targetId" | "sourceId" => "plots",
        "eventId" => "interment_events",
        "leaseId" => "leases",
        "sectionId" => "sections",
        "rowId" => "plot_rows",
        "bookingId" => "bookings",
        "invoiceId" => "invoices",
        "chargeId" => "charges",
        "paymentId" => "payments",
        "itemId" => "stock_items",
        "facilityId" => "facilities",
        "workOrderId" => "work_orders",
        "caseId" => "cases",
        "deceasedId" => "deceased",
        _ => return None,
    })
}

/// The table behind `id` for a method family.
fn table_of_method(method: &str) -> Option<&'static str> {
    let family = method.split('.').next().unwrap_or("");
    Some(match family {
        "cemeteries" => "cemeteries",
        "sections" => "sections",
        "rows" => "plot_rows",
        "plots" => "plots",
        "deceased" => "deceased",
        "events" => "interment_events",
        "leases" => "leases",
        "charges" => "charges",
        "invoices" => "invoices",
        "payments" => "payments",
        "facilities" => "facilities",
        "bookings" => "bookings",
        "workOrders" => "work_orders",
        "workNotices" => "work_notices",
        "cases" => "cases",
        "stock" => "stock_items",
        _ => return None,
    })
}

/// The table of an `entity` name (attachments, voiding, audit).
fn table_of_entity(entity: &str) -> Option<&'static str> {
    Some(match entity {
        "plot" => "plots",
        "section" => "sections",
        "event" | "interment_event" => "interment_events",
        "lease" => "leases",
        "deceased" => "deceased",
        "monument" => "monuments",
        "restriction" | "plot_restriction" => "plot_restrictions",
        "notice" => "notices",
        "booking" => "bookings",
        "work_order" => "work_orders",
        "case" => "cases",
        "charge" => "charges",
        "invoice" => "invoices",
        "payment" => "payments",
        "facility" => "facilities",
        "stock_item" => "stock_items",
        "cemetery" => "cemeteries",
        _ => return None,
    })
}

fn denied() -> CoreError {
    CoreError::forbidden("cemetery")
}

/// The cemeteries a record belongs to (a deceased: those of their burials;
/// none for unknown ids, which the method answers with "not found").
fn record_cemeteries(store: &Store, table: &str, id: &str) -> Result<Vec<String>> {
    if table == "deceased" {
        let mut st = store.conn.prepare("SELECT DISTINCT cemetery_id FROM interment_events WHERE deceased_id = ?1 AND cemetery_id IS NOT NULL")?;
        return Ok(st.query_map([id], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?);
    }
    let Some(sql) = cemetery_sql(table) else { return Ok(vec![]) };
    let cem: Option<Option<String>> = store.conn.query_row(&sql, [id], |r| r.get(0)).optional()?;
    Ok(cem.flatten().into_iter().collect())
}

fn collect(store: &Store, v: &Value, out: &mut Vec<Vec<String>>) -> Result<()> {
    match v {
        Value::Object(map) => {
            for (k, x) in map {
                match (k.as_str(), x) {
                    ("cemeteryId", Value::String(c)) if !c.is_empty() => out.push(vec![c.clone()]),
                    ("plotIds", Value::Array(ids)) => {
                        for id in ids.iter().filter_map(Value::as_str) {
                            out.push(record_cemeteries(store, "plots", id)?);
                        }
                    }
                    (key, Value::String(id)) if !id.is_empty() => {
                        if let Some(t) = table_of_key(key) {
                            out.push(record_cemeteries(store, t, id)?);
                        }
                    }
                    _ => collect(store, x, out)?,
                }
            }
            // entity + entityId pairs (attachments, voiding)
            if let (Some(Value::String(e)), Some(Value::String(id))) = (map.get("entity"), map.get("entityId")) {
                if let Some(t) = table_of_entity(e) {
                    out.push(record_cemeteries(store, t, id)?);
                }
            }
            Ok(())
        }
        Value::Array(items) => items.iter().try_for_each(|x| collect(store, x, out)),
        _ => Ok(()),
    }
}

/// The cemeteries a call reaches, as groups: a group is reached through one
/// argument (a deceased through several cemeteries: any of them will do).
/// Empty: the call names no cemetery (people, settings, organisation-wide lists).
pub fn cemeteries_of(store: &Store, method: &str, args: &Value) -> Result<Vec<Vec<String>>> {
    let mut out = vec![];
    if let (Some(t), Some(id)) = (table_of_method(method), args.get("id").and_then(Value::as_str)) {
        out.push(record_cemeteries(store, t, id)?);
    }
    collect(store, args, &mut out)?;
    out.retain(|g| !g.is_empty());
    Ok(out)
}

/// Refuses a call that reaches into a cemetery the user does not work in.
pub fn check(store: &Store, s: &Session, method: &str, args: &Value) -> Result<()> {
    if s.cemeteries.is_none() {
        return Ok(());
    }
    // the organisation's cemeteries are set up by administrators only
    if method == "cemeteries.save" {
        return Err(denied());
    }
    for group in cemeteries_of(store, method, args)? {
        if !group.iter().any(|c| s.sees_cemetery(c)) {
            return Err(denied());
        }
    }
    Ok(())
}

fn keep(store: &Store, s: &Session, row: &Value) -> Result<bool> {
    let Some(obj) = row.as_object() else { return Ok(true) };
    if let Some(c) = obj.get("cemeteryId").and_then(Value::as_str) {
        return Ok(s.sees_cemetery(c));
    }
    if let Some(p) = obj.get("plotId").and_then(Value::as_str) {
        return Ok(record_cemeteries(store, "plots", p)?.iter().all(|c| s.sees_cemetery(c)));
    }
    Ok(true)
}

fn filter_rows(store: &Store, s: &Session, v: &mut Value) -> Result<()> {
    match v {
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for x in items.drain(..) {
                if keep(store, s, &x)? {
                    out.push(x);
                }
            }
            *items = out;
        }
        Value::Object(map) => {
            for key in ["items", "rows", "results", "hits"] {
                if let Some(x @ Value::Array(_)) = map.get_mut(key) {
                    filter_rows(store, s, x)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Leaves out the rows of other cemeteries from a list answer.
pub fn filter(store: &Store, s: &Session, method: &str, mut v: Value) -> Result<Value> {
    if s.cemeteries.is_none() {
        return Ok(v);
    }
    if method == "cemeteries.list" {
        if let Value::Array(items) = &mut v {
            items.retain(|c| c.get("id").and_then(Value::as_str).is_some_and(|id| s.sees_cemetery(id)));
        }
        return Ok(v);
    }
    filter_rows(store, s, &mut v)?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use crate::api::dispatch;
    use crate::auth::{Role, Session};
    use crate::Store;
    use serde_json::json;

    #[test]
    fn a_colleague_limited_to_one_cemetery_sees_only_that_one() {
        let store = Store::in_memory().unwrap();
        let sys = Session::system();
        crate::sample::load_sample(&store, &sys).unwrap();
        let a: String = store.conn.query_row("SELECT id FROM cemeteries ORDER BY created_at LIMIT 1", [], |r| r.get(0)).unwrap();
        let b = dispatch(&store, &sys, "cemeteries.save", &json!({"cemetery": {"name": "Másik temető"}})).unwrap()["id"].as_str().unwrap().to_string();
        let single: String = store.conn.query_row("SELECT id FROM plot_types WHERE code = 'single'", [], |r| r.get(0)).unwrap();
        dispatch(
            &store,
            &sys,
            "structure.bulkCreate",
            &json!({"input": {"cemeteryId": b, "sectionCode": "Z", "rowFrom": 1, "rowTo": 1, "plotsPerRow": 2, "firstPlotNumber": 1, "plotTypeId": single}}),
        )
        .unwrap();
        let plot_b: String = store.conn.query_row("SELECT id FROM plots WHERE cemetery_id = ?1 LIMIT 1", [&b], |r| r.get(0)).unwrap();
        let plot_a: String = store.conn.query_row("SELECT id FROM plots WHERE cemetery_id = ?1 LIMIT 1", [&a], |r| r.get(0)).unwrap();

        let mut s = Session::for_role(Some("u".into()), "kata", "Kata", Role::Clerk, &[]);
        s.cemeteries = Some(vec![a.clone()]);
        let list = dispatch(&store, &s, "cemeteries.list", &json!({})).unwrap();
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(list[0]["id"], a);
        dispatch(&store, &s, "plots.get", &json!({"id": plot_a})).unwrap();
        assert_eq!(dispatch(&store, &s, "plots.get", &json!({"id": plot_b})).unwrap_err().code, "forbidden");
        assert_eq!(dispatch(&store, &s, "sections.list", &json!({"cemeteryId": b})).unwrap_err().code, "forbidden");
        assert_eq!(dispatch(&store, &s, "plots.list", &json!({"filter": {"cemeteryId": b}})).unwrap_err().code, "forbidden");
        assert_eq!(dispatch(&store, &s, "cemeteries.save", &json!({"cemetery": {"name": "Harmadik"}})).unwrap_err().code, "forbidden");
        // a list without a cemetery keeps only the allowed rows
        let all = dispatch(&store, &s, "plots.list", &json!({"filter": {}})).unwrap();
        assert!(!all["items"].as_array().unwrap().is_empty());
        assert!(all["items"].as_array().unwrap().iter().all(|p| p["cemeteryId"] == a), "{}", all["items"][0]);
        // everyone else is unaffected
        assert_eq!(dispatch(&store, &sys, "cemeteries.list", &json!({})).unwrap().as_array().unwrap().len(), 2);
    }
}
