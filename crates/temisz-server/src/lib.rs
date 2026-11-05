//! TEMISZ web server: HTTP handling shared by the standalone server
//! (development, E2E, a later VPS) and the Vercel function.
//!
//! * `POST /rpc` – the same `{cmd, args}` protocol the UI already speaks;
//! * sessions are random tokens in an HttpOnly cookie, stored hashed in
//!   `platform.sessions`;
//! * every signed-in request runs inside the customer's own schema.

use std::path::{Path, PathBuf};

use base64::Engine;
use lunda_core::auth::Session;
use lunda_core::sql::{params, Connection, OptionalExtension};
use lunda_core::util::{new_id, sha256_hex};
use lunda_core::{app, db, settings, CoreError, Result, Store};
use serde_json::{json, Value};

pub const COOKIE: &str = "temisz_session";
/// Absolute session lifetime; inactivity ends it earlier (tenant setting).
pub const SESSION_HOURS: i64 = 12;
pub const TRIAL_DAYS: i64 = 30;

const PLATFORM_MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/0001_platform.sql"))];

// ---------------------------------------------------------------- HTTP types

#[derive(Debug, Default)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub ip: String,
}

impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    pub fn cookie(&self, name: &str) -> Option<String> {
        self.header("cookie")?.split(';').filter_map(|p| p.trim().split_once('=')).find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
    }
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(v: &Value) -> Self {
        Self {
            status: 200,
            headers: vec![("Content-Type".into(), "application/json; charset=utf-8".into()), ("Cache-Control".into(), "no-store".into())],
            body: v.to_string().into_bytes(),
        }
    }

    pub fn text(status: u16, s: &str) -> Self {
        Self { status, headers: vec![("Content-Type".into(), "text/plain; charset=utf-8".into())], body: s.as_bytes().to_vec() }
    }
}

// ---------------------------------------------------------------- server

pub struct Config {
    /// Send cookies with `Secure` (HTTPS). Off only for local development.
    pub secure_cookies: bool,
    /// Serve the built UI from this directory (standalone server only).
    pub static_dir: Option<PathBuf>,
}

/// Creates/updates the platform schema. Safe to run at every start.
pub fn migrate_platform(conn: &Connection) -> Result<()> {
    conn.execute_batch("CREATE SCHEMA IF NOT EXISTS platform; CREATE TABLE IF NOT EXISTS platform.schema_version (version BIGINT NOT NULL)")?;
    let current: i64 = conn.query_row("SELECT coalesce(max(version), 0) FROM platform.schema_version", [], |r| r.get(0))?;
    for (v, sql) in PLATFORM_MIGRATIONS {
        if *v <= current {
            continue;
        }
        let t = db::tx(conn)?;
        t.execute_batch(sql)?;
        t.execute("INSERT INTO platform.schema_version (version) VALUES (?1)", params![v])?;
        t.commit()?;
    }
    Ok(())
}

/// Handles one request with a connection owned by the calling worker.
pub fn handle(cfg: &Config, store: &Store, req: &Request) -> Response {
    let path = req.path.split('?').next().unwrap_or("/");
    match (req.method.as_str(), path) {
        ("POST", "/rpc") | ("POST", "/api/rpc") => rpc(cfg, store, req),
        ("GET", "/healthz") | ("GET", "/api/healthz") => match store.conn.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)) {
            Ok(_) => Response::text(200, "ok"),
            Err(_) => Response::text(503, "database unavailable"),
        },
        ("GET", _) => match &cfg.static_dir {
            Some(dir) => serve_static(dir, path),
            None => Response::text(404, "not found"),
        },
        _ => Response::text(405, "method not allowed"),
    }
}

fn rpc(cfg: &Config, store: &Store, req: &Request) -> Response {
    // CSRF: only same-origin JSON requests (a cross-site form cannot send this content type).
    let json_ct = req.header("content-type").map(|c| c.starts_with("application/json")).unwrap_or(false);
    let same_origin = match (req.header("origin"), req.header("host")) {
        (Some(o), Some(h)) => o.split("://").nth(1).map(|x| x == h).unwrap_or(false),
        _ => true,
    };
    if !json_ct || !same_origin {
        return Response::text(403, "forbidden");
    }
    let mut set_cookie: Option<String> = None;
    let out = (|| -> Result<Value> {
        let body: Value = serde_json::from_slice(&req.body)?;
        let cmd = body["cmd"].as_str().unwrap_or("");
        let args = &body["args"];
        let s = |k: &str| args[k].as_str().unwrap_or("").to_string();
        match cmd {
            "app_status" => match current(store, req)? {
                Some(c) => Ok(serde_json::to_value(app::status(Some(store), Some(c.session)))?),
                None => Ok(logged_out_status()),
            },
            "register" => {
                let token = register(store, &s("organization"), &s("name"), &s("email"), &s("password"), req)?;
                set_cookie = Some(cookie_value(cfg, &token, SESSION_HOURS * 3600));
                let c = current_by_token(store, &token)?.ok_or_else(|| CoreError::new("session_expired", "session not found"))?;
                Ok(serde_json::to_value(app::status(Some(store), Some(c.session)))?)
            }
            "login" => {
                let token = login(store, &s("username"), &s("password"), req)?;
                set_cookie = Some(cookie_value(cfg, &token, SESSION_HOURS * 3600));
                let c = current_by_token(store, &token)?.ok_or_else(|| CoreError::new("session_expired", "session not found"))?;
                Ok(serde_json::to_value(app::status(Some(store), Some(c.session)))?)
            }
            "logout" => {
                if let Some(c) = current(store, req)? {
                    let _ = lunda_core::audit::record_simple(&store.conn, &c.session, "user", c.session.user_id.as_deref().unwrap_or(""), "logout", None);
                }
                if let Some(t) = req.cookie(COOKIE) {
                    store.conn.execute("DELETE FROM platform.sessions WHERE token_hash = ?1", params![sha256_hex(t.as_bytes())])?;
                }
                set_cookie = Some(cookie_value(cfg, "", 0));
                Ok(logged_out_status())
            }
            "touch" => {
                require(store, req)?;
                Ok(Value::Null)
            }
            "api" => {
                let c = require(store, req)?;
                let method = s("method");
                let empty = json!({});
                let a = args.get("args").unwrap_or(&empty);
                call_api(store, &c.session, &method, a)
            }
            _ => Err(CoreError::new("not_available", format!("{cmd} is not available in the web version"))),
        }
    })();
    let mut resp = Response::json(&match out {
        Ok(v) => json!({ "ok": v }),
        Err(e) => json!({ "err": e }),
    });
    if let Some(c) = set_cookie {
        resp.headers.push(("Set-Cookie".into(), c));
    }
    resp
}

/// Runs an API method; generated files come back inline (base64).
fn call_api(store: &Store, s: &Session, method: &str, args: &Value) -> Result<Value> {
    let work = std::env::temp_dir().join(format!("temisz-{}", new_id()));
    std::fs::create_dir_all(&work)?;
    let res = app::call(store, s, method, args, &work);
    let res = match res {
        Ok(mut v) if matches!(method, "print.document" | "export.file") => {
            if let Some(p) = v.get("path").and_then(|p| p.as_str()).map(PathBuf::from) {
                let bytes = std::fs::read(&p)?;
                v["data"] = json!(base64::engine::general_purpose::STANDARD.encode(bytes));
                v["path"] = json!("");
            }
            Ok(v)
        }
        other => other,
    };
    let _ = std::fs::remove_dir_all(&work);
    res
}

fn logged_out_status() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "dbReady": true,
        "dbError": null,
        "hasUsers": true,
        "loginRequired": true,
        "session": null,
        "dataDir": "",
        "settings": {},
        "backupWarning": false,
        "web": true
    })
}

fn cookie_value(cfg: &Config, token: &str, max_age: i64) -> String {
    format!("{COOKIE}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{}", if cfg.secure_cookies { "; Secure" } else { "" })
}

// ---------------------------------------------------------------- sessions

pub struct Current {
    pub session: Session,
    pub tenant_id: String,
    pub account_id: String,
}

fn require(store: &Store, req: &Request) -> Result<Current> {
    current(store, req)?.ok_or_else(|| CoreError::new("session_expired", "please log in again"))
}

fn current(store: &Store, req: &Request) -> Result<Option<Current>> {
    match req.cookie(COOKIE) {
        Some(t) if !t.is_empty() => current_by_token(store, &t),
        _ => Ok(None),
    }
}

/// Validates a session token, applies the tenant's inactivity limit, switches
/// the connection to the tenant schema and returns the user's session.
fn current_by_token(store: &Store, token: &str) -> Result<Option<Current>> {
    let hash = sha256_hex(token.as_bytes());
    let row: Option<(String, String, String, String, i64)> = store
        .conn
        .query_row(
            "SELECT a.id, a.tenant_id, a.user_id, t.schema_name,
                    CAST(EXTRACT(EPOCH FROM (now() - s.last_seen_at)) AS BIGINT)
             FROM platform.sessions s
             JOIN platform.accounts a ON a.id = s.account_id
             JOIN platform.tenants t ON t.id = a.tenant_id
             WHERE s.token_hash = ?1 AND s.expires_at > now() AND t.status <> 'closed'",
            params![hash],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .optional()?;
    let Some((account_id, tenant_id, user_id, schema, idle_secs)) = row else { return Ok(None) };
    store.conn.use_schema(&schema)?;
    let idle_min = settings::get_i64(&store.conn, "security.idle_logout_minutes", None, 15)?;
    if idle_min > 0 && idle_secs > idle_min * 60 {
        store.conn.execute("DELETE FROM platform.sessions WHERE token_hash = ?1", params![hash])?;
        return Ok(None);
    }
    store.conn.execute("UPDATE platform.sessions SET last_seen_at = now() WHERE token_hash = ?1", params![hash])?;
    let session = match store.session_for(&user_id) {
        Ok(s) => s,
        Err(_) => return Ok(None), // user deactivated
    };
    Ok(Some(Current { session, tenant_id, account_id }))
}

fn new_session(store: &Store, account_id: &str, req: &Request) -> Result<String> {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    let token = hex::encode(b);
    store.conn.execute(
        "INSERT INTO platform.sessions (token_hash, account_id, expires_at, ip, user_agent)
         VALUES (?1, ?2, now() + make_interval(hours => ?3), ?4, ?5)",
        params![
            sha256_hex(token.as_bytes()),
            account_id,
            SESSION_HOURS as i32,
            req.ip,
            req.header("user-agent").unwrap_or("").chars().take(300).collect::<String>()
        ],
    )?;
    Ok(token)
}

// ---------------------------------------------------------------- accounts

fn valid_email(e: &str) -> bool {
    let e = e.trim();
    let Some((local, domain)) = e.split_once('@') else { return false };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && !e.contains(char::is_whitespace) && e.len() <= 254
}

/// Creates a customer: tenant schema with the first administrator, account, session.
pub fn register(store: &Store, organization: &str, name: &str, email: &str, password: &str, req: &Request) -> Result<String> {
    let (organization, name, email) = (organization.trim(), name.trim(), email.trim().to_lowercase());
    if organization.chars().count() < 2 {
        return Err(CoreError::validation("organization", "organization name required").with("code", "required"));
    }
    if name.chars().count() < 2 {
        return Err(CoreError::validation("name", "name required").with("code", "required"));
    }
    if !valid_email(&email) {
        return Err(CoreError::validation("email", "invalid e-mail address").with("code", "email_invalid"));
    }
    lunda_core::auth::validate_password(password)?;
    let taken: Option<i64> = store.conn.query_row("SELECT 1 FROM platform.accounts WHERE lower(email) = ?1", params![email], |r| r.get(0)).optional()?;
    if taken.is_some() {
        return Err(CoreError::new("email_taken", "an account already exists for this e-mail address"));
    }
    let tenant_id = new_id().replace('-', "");
    let schema = db::tenant_schema(&tenant_id)?;
    let account_id = new_id();
    let t = db::tx(&store.conn)?;
    t.execute(
        "INSERT INTO platform.tenants (id, name, schema_name, trial_ends_at, billing_email)
         VALUES (?1, ?2, ?3, now() + make_interval(days => ?4), ?5)",
        params![tenant_id, organization, schema, TRIAL_DAYS as i32, email],
    )?;
    db::open_tenant(&store.conn, &schema)?;
    let user = store.create_initial_admin(&email, name, password)?;
    settings::set_many(&store.conn, &Session::system(), None, &[("operator.name".to_string(), organization.to_string())].into_iter().collect())?;
    t.execute("INSERT INTO platform.accounts (id, email, tenant_id, user_id) VALUES (?1, ?2, ?3, ?4)", params![account_id, email, tenant_id, user.id])?;
    t.commit()?;
    new_session(store, &account_id, req)
}

pub fn login(store: &Store, email: &str, password: &str, req: &Request) -> Result<String> {
    let email = email.trim().to_lowercase();
    let row: Option<(String, String, String)> = store
        .conn
        .query_row(
            "SELECT a.id, t.schema_name, t.status FROM platform.accounts a JOIN platform.tenants t ON t.id = a.tenant_id WHERE lower(a.email) = ?1",
            params![email],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    let Some((account_id, schema, status)) = row else {
        // same work as a wrong password, so the response time does not reveal accounts
        let _ = lunda_core::auth::verify_password(password, "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$1JqfI3pVQSrr7CFZQZD8Ewn5vYkM0+DtB4eMZ5yYO3w");
        return Err(CoreError::new("invalid_credentials", "invalid e-mail address or password"));
    };
    if status == "closed" {
        return Err(CoreError::new("account_closed", "this account has been closed"));
    }
    store.conn.use_schema(&schema)?;
    store.login(&email, password)?; // checks the password, lockout and audit-logs the login
    new_session(store, &account_id, req)
}

// ---------------------------------------------------------------- static files

fn serve_static(dir: &Path, path: &str) -> Response {
    let rel = path.trim_start_matches('/');
    let mut p = dir.join(if rel.is_empty() { "index.html" } else { rel });
    if rel.contains("..") || !p.is_file() {
        p = dir.join("index.html");
    }
    let ct = match p.extension().and_then(|e| e.to_str()) {
        Some("js") => "text/javascript",
        Some("css") => "text/css",
        Some("html") => "text/html; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("png") => "image/png",
        Some("json") => "application/json",
        _ => "application/octet-stream",
    };
    match std::fs::read(&p) {
        Ok(b) => Response { status: 200, headers: vec![("Content-Type".into(), ct.into())], body: b },
        Err(_) => Response::text(404, "not found"),
    }
}

// ---------------------------------------------------------------- sample account

pub const SAMPLE_EMAIL: &str = "admin@minta.temisz.hu";
pub const SAMPLE_PASSWORD: &str = "admin-jelszo";

/// Demo/E2E customer with the sample cemetery (created once).
pub fn ensure_sample_account(store: &Store) -> Result<()> {
    let exists: Option<i64> =
        store.conn.query_row("SELECT 1 FROM platform.accounts WHERE lower(email) = ?1", params![SAMPLE_EMAIL], |r| r.get(0)).optional()?;
    if exists.is_some() {
        return Ok(());
    }
    let req = Request::default();
    register(store, "Mintavölgy Község Önkormányzata", "Minta Adminisztrátor", SAMPLE_EMAIL, SAMPLE_PASSWORD, &req)?;
    let sys = Session::system();
    lunda_core::api::dispatch(store, &sys, "sample.load", &json!({}))?;
    settings::set_many(&store.conn, &sys, None, &[("onboarding.completed".to_string(), "true".to_string())].into_iter().collect())?;
    store
        .conn
        .execute("DELETE FROM platform.sessions WHERE account_id IN (SELECT id FROM platform.accounts WHERE lower(email) = ?1)", params![SAMPLE_EMAIL])?;
    Ok(())
}

/// Removes the sample customer completely (E2E runs start from scratch).
pub fn drop_sample_account(store: &Store) -> Result<()> {
    let tenants: Vec<(String, String)> = store
        .conn
        .prepare("SELECT t.id, t.schema_name FROM platform.tenants t JOIN platform.accounts a ON a.tenant_id = t.id WHERE lower(a.email) = ?1")?
        .query_map(params![SAMPLE_EMAIL], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    for (id, schema) in tenants {
        let t = db::tx(&store.conn)?;
        t.execute("DELETE FROM platform.sessions WHERE account_id IN (SELECT id FROM platform.accounts WHERE tenant_id = ?1)", params![id])?;
        t.execute("DELETE FROM platform.accounts WHERE tenant_id = ?1", params![id])?;
        t.execute("DELETE FROM platform.tenants WHERE id = ?1", params![id])?;
        t.execute_batch(&format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"))?;
        t.commit()?;
    }
    Ok(())
}
