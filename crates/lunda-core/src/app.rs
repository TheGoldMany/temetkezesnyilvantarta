//! Application session state shared by the desktop shell and the
//! development/E2E HTTP bridge: login, idle timeout, dispatch.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;

use crate::auth::Session;
use crate::error::{CoreError, Result};
use std::path::Path;

use crate::auth::Permission;
use crate::{api, backup, db, settings, Store};

pub struct App {
    pub store: Option<Store>,
    pub session: Option<Session>,
    pub last_activity: Instant,
    pub data_dir: PathBuf,
    pub db_error: Option<CoreError>,
    pub db_key: Option<String>,
}

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

impl App {
    pub fn new(store: Option<Store>, data_dir: PathBuf, db_error: Option<CoreError>) -> Self {
        Self { store, session: None, last_activity: Instant::now(), data_dir, db_error, db_key: None }
    }

    fn store(&self) -> Result<&Store> {
        self.store.as_ref().ok_or_else(|| CoreError::new("db_not_ready", "database not ready"))
    }

    pub fn status(&mut self) -> AppStatus {
        // single-user mode (Q10): log in as the configured user without a password
        if self.session.is_none() {
            if let Some(store) = &self.store {
                if !settings::get_bool(&store.conn, "security.login_required", true).unwrap_or(true) {
                    if let Ok(Some(uid)) = settings::get(&store.conn, "security.auto_login_user", None) {
                        if let Ok(s) = store.session_for(&uid) {
                            self.session = Some(s);
                            self.last_activity = Instant::now();
                        }
                    }
                }
            }
        }
        let (has_users, login_required, settings, backup_warning) = match &self.store {
            Some(s) => (
                s.user_count().unwrap_or(0) > 0,
                settings::get_bool(&s.conn, "security.login_required", true).unwrap_or(true),
                settings::all(&s.conn, None).unwrap_or_default(),
                crate::backup::needs_warning(&s.conn).unwrap_or(false),
            ),
            None => (false, true, Default::default(), false),
        };
        let mut settings = settings;
        settings.remove("operator.logo"); // large; fetched separately
        AppStatus {
            version: env!("CARGO_PKG_VERSION").into(),
            db_ready: self.store.is_some(),
            db_error: self.db_error.clone(),
            has_users,
            login_required,
            session: self.session.clone(),
            data_dir: self.data_dir.display().to_string(),
            settings,
            backup_warning,
        }
    }

    pub fn setup_admin(&mut self, username: &str, display_name: &str, password: &str) -> Result<AppStatus> {
        let store = self.store()?;
        let u = store.create_initial_admin(username, display_name, password)?;
        let s = store.session_for(&u.id)?;
        self.session = Some(s);
        self.last_activity = Instant::now();
        Ok(self.status())
    }

    pub fn login(&mut self, username: &str, password: &str) -> Result<AppStatus> {
        let s = self.store()?.login(username, password)?;
        self.session = Some(s);
        self.last_activity = Instant::now();
        Ok(self.status())
    }

    pub fn logout(&mut self) -> AppStatus {
        if let (Some(store), Some(s)) = (&self.store, &self.session) {
            let _ = crate::audit::record_simple(&store.conn, s, "user", s.user_id.as_deref().unwrap_or(""), "logout", None);
        }
        self.session = None;
        self.status()
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// Current session or `session_expired` after inactivity (NFR-BIZ-02).
    pub fn session(&mut self) -> Result<Session> {
        let store = self.store()?;
        let idle = settings::get_i64(&store.conn, "security.idle_logout_minutes", None, 15)?;
        let required = settings::get_bool(&store.conn, "security.login_required", true)?;
        if required && idle > 0 && self.last_activity.elapsed() > Duration::from_secs(idle as u64 * 60) {
            self.session = None;
        }
        let s = self.session.clone().ok_or_else(|| CoreError::new("session_expired", "please log in again"))?;
        self.last_activity = Instant::now();
        Ok(s)
    }

    pub fn call(&mut self, method: &str, args: &Value) -> Result<Value> {
        let s = self.session()?;
        let store = self.store()?;
        let str_arg = |k: &str| -> Result<String> {
            args.get(k).and_then(|v| v.as_str()).map(String::from).ok_or_else(|| CoreError::new("bad_request", format!("argument `{k}`")))
        };
        match method {
            "backup.create" => Ok(serde_json::to_value(backup::create(&store.conn, &s, &self.data_dir, "manual")?)?),
            "backup.list" => {
                let (dir, _) = backup::target_dir(&store.conn, &self.data_dir)?;
                let mut v = backup::list(&dir)?;
                let local = backup::default_dir(&self.data_dir);
                if local != dir {
                    v.extend(backup::list(&local)?);
                }
                Ok(serde_json::to_value(v)?)
            }
            "print.document" => {
                let doc = str_arg("doc")?;
                let empty = Value::Object(Default::default());
                let a = args.get("args").unwrap_or(&empty);
                Ok(serde_json::to_value(crate::pdf::generate(store, &s, &doc, a, &self.data_dir.join("exports"))?)?)
            }
            "export.file" => {
                let kind = str_arg("kind")?;
                let format = args.get("format").and_then(|v| v.as_str()).unwrap_or("xlsx").to_string();
                let empty = Value::Object(Default::default());
                let a = args.get("args").unwrap_or(&empty);
                Ok(serde_json::to_value(crate::io::export(store, &s, &kind, &format, a, &self.data_dir.join("exports"))?)?)
            }
            "import.read" | "import.run" => {
                s.require(Permission::Import)?;
                let sheet = args.get("sheet").and_then(|v| v.as_str());
                let table = match (args.get("path").and_then(|v| v.as_str()), args.get("content").and_then(|v| v.as_str())) {
                    (Some(p), _) => crate::io::read_table(Path::new(p), sheet)?,
                    (None, Some(c)) => crate::io::read_upload(&str_arg("fileName")?, c, sheet)?,
                    _ => return Err(CoreError::new("bad_request", "path or content required")),
                };
                if method == "import.read" {
                    let mapping = crate::io::guess_mapping(&table.headers);
                    let sample: Vec<Vec<String>> = table.rows.iter().take(10).cloned().collect();
                    Ok(
                        serde_json::json!({ "headers": table.headers, "rows": sample, "rowCount": table.rows.len(), "sheetNames": table.sheet_names, "mapping": mapping, "targets": crate::io::TARGETS }),
                    )
                } else {
                    let opts: crate::io::ImportOptions = serde_json::from_value(args.get("options").cloned().unwrap_or_default())?;
                    Ok(serde_json::to_value(crate::io::import_rows(store, &s, &table, &opts)?)?)
                }
            }
            "import.check" => Ok(serde_json::to_value(crate::io::migration_check(store, args.get("cemeteryId").and_then(|v| v.as_str()))?)?),
            "backup.log" => Ok(serde_json::to_value(backup::log(&store.conn, 50)?)?),
            "backup.verify" => {
                s.require(Permission::Backup)?;
                let key = args.get("recoveryKey").and_then(|v| v.as_str()).map(String::from);
                Ok(serde_json::to_value(backup::verify(Path::new(&str_arg("path")?), key.as_deref())?)?)
            }
            "backup.recoveryKey" => Ok(serde_json::to_value(backup::reveal_recovery_key(&store.conn, &s)?)?),
            "backup.restore" => {
                s.require(Permission::ManageSettings)?;
                let path = str_arg("path")?;
                let rk = str_arg("recoveryKey")?;
                let key = self.db_key.clone().ok_or_else(|| CoreError::new("db_not_ready", "no database key"))?;
                self.restore(&s, Path::new(&path), &rk, &key)?;
                Ok(Value::Bool(true))
            }
            _ => api::dispatch(store, &s, method, args),
        }
    }

    /// Opens the encrypted database in `data_dir` with `key`. Before a schema
    /// migration a backup of the old database is taken (NFR-KAR-01).
    pub fn open(data_dir: PathBuf, key: Option<String>, key_error: Option<CoreError>) -> Self {
        let path = data_dir.join(DB_FILE);
        let result = match (&key, key_error) {
            (_, Some(e)) => Err(e),
            (None, None) => Err(CoreError::new("db_key_missing", "the database key is missing; restore from a backup")),
            (Some(k), None) => {
                let dd = data_dir.clone();
                db::open(&path, k, |conn, _from| backup::create(conn, &Session::system(), &dd, "pre-migration").map(|_| ())).map(Store::new)
            }
        };
        let mut app = match result {
            Ok(store) => App::new(Some(store), data_dir, None),
            Err(e) => App::new(None, data_dir, Some(e)),
        };
        app.db_key = key;
        app
    }

    /// FR-NAP-05: takes a pre-restore backup, replaces the database, reopens it.
    pub fn restore(&mut self, s: &Session, backup_path: &Path, recovery_key: &str, db_key: &str) -> Result<()> {
        let rk = backup::parse_recovery_key(recovery_key)?;
        let rep = backup::verify(backup_path, Some(&rk))?;
        if !rep.opened {
            return Err(CoreError::new("wrong_recovery_key", "the backup cannot be opened with this recovery key"));
        }
        if let Some(store) = &self.store {
            backup::create(&store.conn, s, &self.data_dir, "pre-restore")?;
        }
        self.store = None; // closes the connection
        let path = self.data_dir.join(DB_FILE);
        std::fs::create_dir_all(&self.data_dir)?;
        backup::restore_into(backup_path, &rk, &path, db_key)?;
        let conn = db::open(&path, db_key, |_, _| Ok(()))?;
        let store = Store::new(conn);
        // the restored database keeps using the same recovery key for future backups
        settings::set_internal(&store.conn, "internal.recovery_key", &rk)?;
        crate::audit::record_simple(&store.conn, s, "backup", &backup_path.display().to_string(), "restore", rep.manifest.as_ref().map(|m| m.sha256.as_str()))?;
        self.store = Some(store);
        self.db_error = None;
        self.session = None;
        Ok(())
    }

    /// First-run restore on a new machine or after keychain loss: allowed only
    /// when the database cannot be opened or has no users yet.
    pub fn restore_fresh(&mut self, backup_path: &Path, recovery_key: &str, new_db_key: &str) -> Result<()> {
        if let Some(store) = &self.store {
            if store.user_count()? > 0 {
                return Err(CoreError::forbidden("restore_requires_login"));
            }
        }
        let rk = backup::parse_recovery_key(recovery_key)?;
        self.store = None;
        let path = self.data_dir.join(DB_FILE);
        std::fs::create_dir_all(&self.data_dir)?;
        if path.exists() {
            let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
            std::fs::rename(&path, self.data_dir.join(format!("{DB_FILE}.unreadable-{stamp}")))?;
        }
        backup::restore_into(backup_path, &rk, &path, new_db_key)?;
        let store = Store::new(db::open(&path, new_db_key, |_, _| Ok(()))?);
        settings::set_internal(&store.conn, "internal.recovery_key", &rk)?;
        crate::audit::record_simple(&store.conn, &Session::system(), "backup", &backup_path.display().to_string(), "restore", None)?;
        self.store = Some(store);
        self.db_key = Some(new_db_key.to_string());
        self.db_error = None;
        self.session = None;
        Ok(())
    }

    /// Daily automatic backup (FR-NAP-03). Returns true when a backup was made.
    pub fn backup_if_due(&mut self) -> Result<bool> {
        let store = self.store()?;
        if backup::due(&store.conn)? {
            backup::create(&store.conn, &Session::system(), &self.data_dir, "auto")?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Backup on exit (FR-NAP-03), if enabled.
    pub fn backup_on_exit(&mut self) -> Result<()> {
        if let Some(store) = &self.store {
            if settings::get_bool(&store.conn, "backup.on_exit", true)? {
                backup::create(&store.conn, &Session::system(), &self.data_dir, "exit")?;
            }
        }
        Ok(())
    }
}

pub const DB_FILE: &str = "lunda-temeto.db";
