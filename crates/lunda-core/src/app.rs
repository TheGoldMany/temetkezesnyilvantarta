//! Request-level service used by the web server: status for the UI and
//! dispatch of one API call for an authenticated session. Sessions themselves
//! (cookies, expiry) are kept by the server in the platform schema.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::auth::{Permission, Session};
use crate::error::{CoreError, Result};
use crate::{api, settings, Store};

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub version: String,
    pub db_ready: bool,
    pub db_error: Option<CoreError>,
    pub has_users: bool,
    pub login_required: bool,
    pub session: Option<Session>,
    pub data_dir: String,
    pub settings: BTreeMap<String, String>,
    pub backup_warning: bool,
}

/// Status shown by the UI: who is logged in and the tenant's settings.
pub fn status(store: Option<&Store>, session: Option<Session>) -> AppStatus {
    let (has_users, settings) = match (store, &session) {
        (Some(s), Some(_)) => (true, settings::all(&s.conn, None).unwrap_or_default()),
        (Some(s), None) => (s.user_count().unwrap_or(0) > 0, BTreeMap::new()),
        _ => (false, BTreeMap::new()),
    };
    let mut settings = settings;
    settings.remove("operator.logo"); // large; fetched separately
    AppStatus {
        version: env!("CARGO_PKG_VERSION").into(),
        db_ready: store.is_some(),
        db_error: None,
        has_users,
        login_required: true,
        session,
        data_dir: String::new(),
        settings,
        backup_warning: false,
    }
}

/// One API call. Generated files (PDF, XLSX) are written to `work_dir`; the
/// server sends them back in the same response.
pub fn call(store: &Store, s: &Session, method: &str, args: &Value, work_dir: &Path) -> Result<Value> {
    let str_arg = |k: &str| -> Result<String> {
        args.get(k).and_then(|v| v.as_str()).map(String::from).ok_or_else(|| CoreError::new("bad_request", format!("argument `{k}`")))
    };
    let empty = Value::Object(Default::default());
    match method {
        "print.document" => {
            let doc = str_arg("doc")?;
            let a = args.get("args").unwrap_or(&empty);
            Ok(serde_json::to_value(crate::pdf::generate(store, s, &doc, a, work_dir)?)?)
        }
        "export.file" => {
            let kind = str_arg("kind")?;
            let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("xlsx").to_string();
            let a = args.get("args").unwrap_or(&empty);
            Ok(serde_json::to_value(crate::io::export(store, s, &kind, &format, a, work_dir)?)?)
        }
        "import.read" | "import.run" => {
            s.require(Permission::Import)?;
            let sheet = args.get("sheet").and_then(|v| v.as_str());
            let content = str_arg("content")?;
            let table = crate::io::read_upload(&str_arg("fileName")?, &content, sheet)?;
            if method == "import.read" {
                let mapping = crate::io::guess_mapping(&table.headers);
                let sample: Vec<Vec<String>> = table.rows.iter().take(10).cloned().collect();
                Ok(
                    serde_json::json!({ "headers": table.headers, "rows": sample, "rowCount": table.rows.len(), "sheetNames": table.sheet_names, "mapping": mapping, "targets": crate::io::TARGETS }),
                )
            } else {
                let opts: crate::io::ImportOptions = serde_json::from_value(args.get("options").cloned().unwrap_or_default())?;
                Ok(serde_json::to_value(crate::io::import_rows(store, s, &table, &opts)?)?)
            }
        }
        "import.check" => Ok(serde_json::to_value(crate::io::migration_check(store, args.get("cemeteryId").and_then(|v| v.as_str()))?)?),
        m if m.starts_with("backup.") => Err(CoreError::new("not_available", "backups are made by the server")),
        _ => api::dispatch(store, s, method, args),
    }
}
