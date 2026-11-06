//! Automatic expiry reminders (daily job): for every live customer that
//! switched them on, e-mails holders half a year before the expiry and then
//! monthly, records each as a notice, and tells the office who did not react.

use lunda_core::auth::Session;
use lunda_core::store::oversight::OutMail;
use lunda_core::{settings, Result, Store};
use serde_json::{json, Value};

use crate::mail;

pub fn daily(store: &Store) -> Result<Value> {
    let tenants: Vec<(String, String, String)> = store
        .conn
        .prepare("SELECT id, schema_name, billing_email FROM platform.tenants WHERE status IN ('trial', 'active') AND deleted_at IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let (mut sent, mut offices) = (0, 0);
    for (tenant, schema, billing_email) in tenants {
        store.conn.use_schema(&schema)?;
        if !store.auto_reminders_enabled()? {
            continue;
        }
        let n = run_for_tenant(store, &tenant, &billing_email)?;
        sent += n;
        offices += 1;
    }
    Ok(json!({ "reminders": sent, "offices": offices }))
}

fn run_for_tenant(store: &Store, tenant: &str, billing_email: &str) -> Result<usize> {
    let system = Session::system();
    let today = lunda_core::util::fmt_date(lunda_core::util::today());
    let plots = store.auto_reminder_plots()?;
    let mut sent = 0;
    if !plots.is_empty() {
        let prep = store.expiry_mails(&system, &plots)?;
        for m in prep["mails"].as_array().cloned().unwrap_or_default() {
            let m: OutMail = serde_json::from_value(m)?;
            mail::send_for(&store.conn, Some(tenant), &m.to, m.reply_to.as_deref(), &m.subject, &m.body, "expiry_reminder")?;
            store.record_notices(&system, std::slice::from_ref(&m.ref_id), "email", &today, &format!("automatikus e-mail: {}", m.to))?;
            sent += 1;
        }
    }
    // the office hears about holders who did not react, once a week (Monday)
    let unanswered = store.unanswered_reminders()?;
    let monday = chrono::Datelike::weekday(&lunda_core::util::today()) == chrono::Weekday::Mon;
    if sent > 0 || (monday && !unanswered.is_empty()) {
        let to = settings::get(&store.conn, "operator.email", None)?.filter(|e| e.contains('@')).unwrap_or_else(|| billing_email.to_string());
        let list: Vec<String> = unanswered
            .iter()
            .take(30)
            .map(|u| {
                format!(
                    "- {} – {} (lejárt: {}{})",
                    u["identifier"].as_str().unwrap_or(""),
                    u["holderName"].as_str().unwrap_or("?"),
                    u["expiryDate"].as_str().unwrap_or(""),
                    if u["reason"] == "no_email" {
                        ", nincs e-mail-cím, levél kell".to_string()
                    } else {
                        format!(", {} emlékeztető után sem jelentkezett", u["reminders"])
                    }
                )
            })
            .collect();
        let body = format!(
            "Jó napot kívánunk!\n\nMa {sent} lejárati emlékeztető ment ki e-mailben a rendelkezési jogosultaknak.{}\n\nA részletek a TEMISZ Lejáratok oldalán láthatók.{}",
            if list.is_empty() {
                String::new()
            } else {
                format!("\n\nNem reagáltak ({}):\n{}{}", unanswered.len(), list.join("\n"), if unanswered.len() > 30 { "\n…" } else { "" })
            },
            mail::FOOTER
        );
        mail::send_for(&store.conn, Some(tenant), &to, None, "Lejárati emlékeztetők – napi összesítő", &body, "reminder_summary")?;
    }
    Ok(sent)
}
