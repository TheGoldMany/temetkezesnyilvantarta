//! TEMISZ web server: HTTP handling shared by the standalone server
//! (development, E2E, a later VPS) and the Vercel function.
//!
//! * `POST /rpc` – the same `{cmd, args}` protocol the UI already speaks;
//! * sessions are random tokens in an HttpOnly cookie, stored hashed in
//!   `platform.sessions`;
//! * every signed-in request runs inside the customer's own schema.

pub mod accounts;
pub mod billing;
pub mod lifecycle;
pub mod mail;
pub mod reminders;
pub mod tenant_data;
pub mod totp;

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

const PLATFORM_MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/0001_platform.sql")),
    (2, include_str!("../migrations/0002_accounts.sql")),
    (3, include_str!("../migrations/0003_billing.sql")),
    (4, include_str!("../migrations/0004_lifecycle.sql")),
    (5, include_str!("../migrations/0005_notices.sql")),
    (6, include_str!("../migrations/0006_bank.sql")),
];

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
            headers: vec![
                ("Content-Type".into(), "application/json; charset=utf-8".into()),
                ("Cache-Control".into(), "no-store".into()),
                ("X-Content-Type-Options".into(), "nosniff".into()),
            ],
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
    /// Development and E2E only: exposes the outgoing e-mail log at /dev/outbox.
    pub dev_mode: bool,
}

/// Creates/updates the platform schema. Safe to run at every start.
pub fn migrate_platform(conn: &Connection) -> Result<()> {
    // several server instances may start at once: one migrates, the others
    // wait (CREATE … IF NOT EXISTS is not safe against concurrent runs either)
    let t = db::tx(conn)?;
    t.execute_batch(
        "SELECT pg_advisory_xact_lock(7381001);
         CREATE SCHEMA IF NOT EXISTS platform;
         CREATE TABLE IF NOT EXISTS platform.schema_version (version BIGINT NOT NULL)",
    )?;
    let current: i64 = t.query_row("SELECT coalesce(max(version), 0) FROM platform.schema_version", [], |r| r.get(0))?;
    for (v, sql) in PLATFORM_MIGRATIONS {
        if *v <= current {
            continue;
        }
        t.execute_batch(sql)?;
        t.execute("INSERT INTO platform.schema_version (version) VALUES (?1)", params![v])?;
    }
    t.commit()?;
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
        ("GET" | "OPTIONS", "/api/public/search" | "/api/public/info" | "/api/public/map" | "/api/public/notices") => public_search(store, req),
        ("GET", "/api/cron/daily") | ("POST", "/api/cron/daily") => {
            let secret = std::env::var("CRON_SECRET").unwrap_or_default();
            let ok = (!secret.is_empty() && req.header("authorization") == Some(&format!("Bearer {secret}"))) || cfg.dev_mode;
            if !ok {
                return Response::text(401, "unauthorized");
            }
            let res = billing::daily(store, &accounts::base_url(req))
                .and_then(|b| Ok(json!({ "billing": b, "lifecycle": lifecycle::daily(store)?, "reminders": reminders::daily(store)? })));
            match res {
                Ok(v) => Response::json(&v),
                Err(e) => Response::text(500, &e.to_string()),
            }
        }
        ("POST", "/dev/expire-trial") if cfg.dev_mode => {
            let email = req.path.split("email=").nth(1).unwrap_or("").replace("%40", "@");
            match store.conn.execute(
                "UPDATE platform.tenants SET trial_ends_at = now() - interval '1 minute' WHERE id = (SELECT tenant_id FROM platform.accounts WHERE lower(email) = lower(?1))",
                params![email],
            ) {
                Ok(n) => Response::text(200, &n.to_string()),
                Err(e) => Response::text(500, &e.to_string()),
            }
        }
        ("GET", "/dev/outbox") if cfg.dev_mode => {
            let to = req.path.split("to=").nth(1).unwrap_or("").replace("%40", "@");
            let rows = store.conn.query_row(
                "SELECT coalesce(json_agg(json_build_object('subject', subject, 'body', body, 'kind', kind) ORDER BY created_at DESC)::text, '[]')
                 FROM platform.emails WHERE lower(to_address) = lower(?1)",
                params![to],
                |r| r.get::<_, String>(0),
            );
            match rows {
                Ok(j) => Response { status: 200, headers: vec![("Content-Type".into(), "application/json".into())], body: j.into_bytes() },
                Err(e) => Response::text(500, &e.to_string()),
            }
        }
        ("GET", _) => match &cfg.static_dir {
            Some(dir) => serve_static(dir, path),
            None => Response::text(404, "not found"),
        },
        _ => Response::text(405, "method not allowed"),
    }
}

/// FR-UGY-02: public grave search for the operator's website (CORS open,
/// rate limited, only when the operator published it).
fn public_search(store: &Store, req: &Request) -> Response {
    let cors = |mut r: Response| {
        r.headers.push(("Access-Control-Allow-Origin".into(), "*".into()));
        r.headers.push(("Access-Control-Allow-Methods".into(), "GET".into()));
        r
    };
    if req.method == "OPTIONS" {
        return cors(Response::text(204, ""));
    }
    let query = req.path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let param = |k: &str| query.split('&').find_map(|kv| kv.split_once('=').filter(|(a, _)| *a == k).map(|(_, v)| url_decode(v))).unwrap_or_default();
    let out = (|| -> Result<Value> {
        accounts::rate_limit(&store.conn, &format!("public:{}", req.ip), 60, 1)?;
        let token = param("t");
        let row: Option<(String, String)> = store
            .conn
            .query_row(
                "SELECT schema_name, name FROM platform.tenants WHERE public_token = ?1 AND status <> 'closed' AND deleted_at IS NULL",
                params![token],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((schema, name)) = row else { return Err(CoreError::new("not_found", "unknown search")) };
        store.conn.use_schema(&schema)?;
        match req.path.split('?').next().unwrap_or("") {
            "/api/public/info" => Ok(json!({ "operator": name, "cemeteries": store.public_cemeteries()? })),
            "/api/public/map" => store.public_map(&param("c")),
            "/api/public/notices" => Ok(json!({ "notices": store.public_reuse_notices()? })),
            _ => Ok(json!({ "operator": name, "results": store.public_search(&param("q"))? })),
        }
    })();
    let mut resp = match out {
        Ok(v) => Response::json(&v),
        Err(e) => {
            let status = match e.code.as_str() {
                "not_found" | "public_search_disabled" => 404,
                "rate_limited" => 429,
                _ => 500,
            };
            let mut r = Response::json(&json!({ "error": e.code }));
            r.status = status;
            r
        }
    };
    resp.headers.retain(|(k, _)| k != "Cache-Control");
    resp.headers.push(("Cache-Control".into(), "public, max-age=60".into()));
    cors(resp)
}

fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("zz"), 16) {
                Ok(v) => {
                    out.push(v);
                    i += 2;
                }
                Err(_) => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
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
        // signed in: status with the account part (verification, 2FA, trial)
        let signed_in = |c: &Current| -> Result<Value> {
            let mut v = serde_json::to_value(app::status(Some(store), Some(c.session.clone())))?;
            let acc = accounts::account_by_id(&store.conn, &c.account_id)?;
            billing::effective_status(&store.conn, &c.tenant_id)?;
            v["account"] = accounts::account_status(store, &acc, c.mfa_setup)?;
            v["account"]["superadmin"] = json!(is_superadmin(&acc.email));
            v["web"] = json!(true);
            Ok(v)
        };
        let start = |token: &str, set_cookie: &mut Option<String>| -> Result<Value> {
            *set_cookie = Some(cookie_value(cfg, token, SESSION_HOURS * 3600));
            let c = current_by_token(store, token)?.ok_or_else(|| CoreError::new("session_expired", "session not found"))?;
            signed_in(&c)
        };
        match cmd {
            "app_status" => match current(store, req)? {
                Some(c) => signed_in(&c),
                None => Ok(logged_out_status()),
            },
            "register" => {
                accounts::rate_limit(&store.conn, &format!("register:{}", req.ip), 10, 60)?;
                let token = register(store, &s("organization"), &s("name"), &s("email"), &s("password"), req)?;
                start(&token, &mut set_cookie)
            }
            "login" => {
                accounts::rate_limit(&store.conn, &format!("login:{}", req.ip), 30, 15)?;
                accounts::rate_limit(&store.conn, &format!("login:{}", s("username").trim().to_lowercase()), 10, 15)?;
                match login(store, &s("username"), &s("password"), req)? {
                    LoginStep::Session(token) => start(&token, &mut set_cookie),
                    LoginStep::SecondFactor(challenge) => Ok(json!({ "mfa": true, "challenge": challenge })),
                }
            }
            "login_mfa" => {
                accounts::rate_limit(&store.conn, &format!("mfa:{}", req.ip), 20, 15)?;
                let t = db::tx(&store.conn)?;
                let acc_id = accounts::use_token(&t, &s("challenge"), "mfa")?;
                let acc = accounts::account_by_id(&t, &acc_id)?;
                if let Err(e) = accounts::check_second_factor(&t, &acc, &s("code")) {
                    // keep the challenge usable for another try
                    drop(t);
                    let again = accounts::create_token(&store.conn, &acc_id, "mfa", accounts::MFA_MINUTES)?;
                    return Err(e.with("challenge", again));
                }
                t.commit()?;
                let token = new_session(store, &acc_id, req)?;
                start(&token, &mut set_cookie)
            }
            "verify_email" => {
                accounts::verify_email(&store.conn, &s("token"))?;
                match current(store, req)? {
                    Some(c) => signed_in(&c),
                    None => Ok(logged_out_status()),
                }
            }
            "forgot_password" => {
                accounts::forgot_password(&store.conn, &s("email"), req)?;
                Ok(Value::Null)
            }
            "reset_password" | "accept_invite" => {
                let kind = if cmd == "reset_password" { "reset" } else { "invite" };
                let name = args["name"].as_str();
                let acc_id = accounts::set_password_with_token(store, &s("token"), kind, &s("password"), name)?;
                let acc = accounts::account_by_id(&store.conn, &acc_id)?;
                if acc.totp_enabled {
                    // the password link does not replace the second factor
                    return Ok(json!({ "mfa": true, "challenge": accounts::create_token(&store.conn, &acc_id, "mfa", accounts::MFA_MINUTES)? }));
                }
                let token = new_session(store, &acc_id, req)?;
                start(&token, &mut set_cookie)
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
            "resend_verification" => {
                let c = require(store, req)?;
                accounts::rate_limit(&store.conn, &format!("verify:{}", c.account_id), 3, 60)?;
                let acc = accounts::account_by_id(&store.conn, &c.account_id)?;
                accounts::send_verification(&store.conn, &acc.id, &acc.email, req)?;
                Ok(Value::Null)
            }
            "totp_setup" => {
                let c = require(store, req)?;
                accounts::totp_setup(store, &accounts::account_by_id(&store.conn, &c.account_id)?)
            }
            "totp_enable" => {
                let c = require(store, req)?;
                accounts::totp_enable(store, &accounts::account_by_id(&store.conn, &c.account_id)?, &c.session, &s("code"))
            }
            "totp_disable" => {
                let c = require(store, req)?;
                accounts::totp_disable(store, &accounts::account_by_id(&store.conn, &c.account_id)?, &c.session, &s("password"))?;
                Ok(Value::Null)
            }
            "user_send_link" => {
                let c = require(store, req)?;
                accounts::send_access_link(store, &c.session, &c.tenant_id, &s("id"), req)?;
                Ok(Value::Null)
            }
            "api" => {
                let c = require(store, req)?;
                let method = s("method");
                let empty = json!({});
                let a = args.get("args").unwrap_or(&empty);
                // an admin who must turn on two-factor sign-in can only do that
                if c.mfa_setup {
                    return Err(CoreError::new("totp_required", "turn on two-factor sign-in first"));
                }
                // after the trial or an unpaid proforma the register can only be read
                let read_only = billing::effective_status(&store.conn, &c.tenant_id)? == "read_only";
                if read_only && !billing::is_read_method(&method) {
                    return Err(CoreError::new("read_only", "the account is read-only until a subscription is paid"));
                }
                let session = if read_only { billing::read_only_session(&c.session) } else { c.session.clone() };
                match method.as_str() {
                    "users.create" => accounts::invite(store, &session, &c.tenant_id, a, req),
                    "users.update" => {
                        let v = call_api(store, &session, &method, a)?;
                        if a["active"].as_bool() == Some(false) {
                            store.conn.execute(
                                "DELETE FROM platform.sessions WHERE account_id IN (SELECT id FROM platform.accounts WHERE tenant_id = ?1 AND user_id = ?2)",
                                params![c.tenant_id, a["id"].as_str().unwrap_or("")],
                            )?;
                        }
                        Ok(v)
                    }
                    "users.changePassword" if a["id"].as_str() != session.user_id.as_deref() => {
                        Err(CoreError::new("not_available", "send a password link instead"))
                    }
                    // FR-UGY-01: e-mail notices go out through the platform mailer
                    "notices.email" => {
                        let prep = call_api(store, &session, "notices.emailPrepare", a)?;
                        let date = lunda_core::util::fmt_date(lunda_core::util::today());
                        let mut sent = 0;
                        for m in prep["mails"].as_array().cloned().unwrap_or_default() {
                            let m: lunda_core::store::oversight::OutMail = serde_json::from_value(m)?;
                            mail::send_for(&store.conn, Some(&c.tenant_id), &m.to, m.reply_to.as_deref(), &m.subject, &m.body, "expiry_notice")?;
                            store.record_notices(&session, std::slice::from_ref(&m.ref_id), "email", &date, &format!("e-mail: {}", m.to))?;
                            sent += 1;
                        }
                        Ok(json!({ "sent": sent, "withoutEmail": prep["withoutEmail"] }))
                    }
                    "invoices.reminder" if a["channel"].as_str() == Some("email") => {
                        let m: lunda_core::store::oversight::OutMail = serde_json::from_value(call_api(store, &session, "invoices.reminderMail", a)?)?;
                        mail::send_for(&store.conn, Some(&c.tenant_id), &m.to, m.reply_to.as_deref(), &m.subject, &m.body, "payment_reminder")?;
                        call_api(store, &session, &method, a)
                    }
                    "emails.list" => {
                        if !(session.can(lunda_core::auth::Permission::RecordLeases) || session.can(lunda_core::auth::Permission::RecordPayments)) {
                            return Err(CoreError::forbidden("record_leases"));
                        }
                        let text: String = store.conn.query_row(
                            "SELECT coalesce(json_agg(json_build_object('id', id, 'to', to_address, 'subject', subject, 'kind', kind, 'createdAt', created_at,
                                                                  'sentAt', sent_at, 'error', error) ORDER BY created_at DESC), '[]')::text
                             FROM (SELECT * FROM platform.emails WHERE tenant_id = ?1 ORDER BY created_at DESC LIMIT 200) e",
                            params![c.tenant_id],
                            |r| r.get(0),
                        )?;
                        Ok(serde_json::from_str(&text)?)
                    }
                    // FR-UGY-02: the embed code of the public grave search
                    "public.embed" => {
                        session.require(lunda_core::auth::Permission::ManageSettings)?;
                        let token: String = store.conn.query_row(
                            "UPDATE platform.tenants SET public_token = coalesce(public_token, replace(gen_random_uuid()::text, '-', '')) WHERE id = ?1 RETURNING public_token",
                            params![c.tenant_id],
                            |r| r.get(0),
                        )?;
                        let base = accounts::site_url(req);
                        Ok(json!({
                            "token": token,
                            "enabled": store.public_search_enabled()?,
                            "url": format!("{base}/kereso?t={token}"),
                            "iframe": format!("<iframe src=\"{base}/kereso?t={token}\" title=\"Sírkereső\" style=\"width:100%;min-height:480px;border:0\" loading=\"lazy\"></iframe>"),
                        }))
                    }
                    "cemeteries.save" | "records.void" => {
                        let v = call_api(store, &session, &method, a)?;
                        // a new or archived cemetery changes the subscription quantity
                        billing::sync_cemetery_count(store, &c.tenant_id)?;
                        Ok(v)
                    }
                    _ => call_api(store, &session, &method, a),
                }
            }
            "billing_overview" => {
                let c = require(store, req)?;
                c.session.require(lunda_core::auth::Permission::ManageSettings)?;
                billing::overview(store, &c.tenant_id)
            }
            "billing_save_details" => {
                let c = require(store, req)?;
                c.session.require(lunda_core::auth::Permission::ManageSettings)?;
                billing::save_details(&store.conn, &c.tenant_id, args)?;
                billing::overview(store, &c.tenant_id)
            }
            "billing_subscribe" => {
                let c = require(store, req)?;
                billing::subscribe(store, &c.session, &c.tenant_id, &s("plan"), &s("paymentMethod"))
            }
            "billing_quote_cemetery" => {
                let c = require(store, req)?;
                billing::quote_new_cemetery(store, &c.tenant_id)
            }
            "close_account" => {
                let c = require(store, req)?;
                lifecycle::close(store, &c.session, &c.tenant_id, &s("password"))?;
                set_cookie = Some(cookie_value(cfg, "", 0));
                Ok(logged_out_status())
            }
            "import_desktop_backup" => {
                let c = require(store, req)?;
                c.session.require(lunda_core::auth::Permission::ManageSettings)?;
                if billing::effective_status(&store.conn, &c.tenant_id)? == "read_only" {
                    return Err(CoreError::new("read_only", "the account is read-only"));
                }
                import_desktop(store, &c, &s("content"), &s("recoveryKey"))
            }
            "billing_invoice_pdf" => {
                let c = require(store, req)?;
                c.session.require(lunda_core::auth::Permission::ManageSettings)?;
                billing::invoice_pdf(&store.conn, &c.tenant_id, &s("id"))
            }
            "admin_overview" | "admin_mark_paid" | "admin_extend_trial" | "admin_bank_import" | "admin_bank_resolve" => {
                let c = require(store, req)?;
                let acc = accounts::account_by_id(&store.conn, &c.account_id)?;
                if !is_superadmin(&acc.email) {
                    return Err(CoreError::forbidden("platform_admin"));
                }
                match cmd {
                    "admin_mark_paid" => {
                        billing::mark_paid(&store.conn, &s("invoiceId"))?;
                        admin_overview(store)
                    }
                    "admin_bank_import" => {
                        let r = billing::import_statement(&store.conn, &s("fileName"), &s("content"), &acc.email)?;
                        let mut o = admin_overview(store)?;
                        o["import"] = r;
                        Ok(o)
                    }
                    "admin_bank_resolve" => {
                        let inv = args["invoiceId"].as_str().filter(|x| !x.is_empty());
                        billing::resolve_line(&store.conn, &s("lineId"), inv, &acc.email)?;
                        admin_overview(store)
                    }
                    "admin_extend_trial" => {
                        let days = args["days"].as_i64().unwrap_or(14).clamp(1, 90) as i32;
                        store.conn.execute(
                            "UPDATE platform.tenants SET trial_ends_at = greatest(trial_ends_at, now()) + make_interval(days => ?2),
                                    status = CASE WHEN status = 'read_only' THEN 'trial' ELSE status END WHERE id = ?1",
                            params![s("tenantId"), days],
                        )?;
                        admin_overview(store)
                    }
                    _ => admin_overview(store),
                }
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
    /// The admin must turn on two-factor sign-in before anything else.
    pub mfa_setup: bool,
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
    let row: Option<(String, String, String, String, i64, bool)> = store
        .conn
        .query_row(
            "SELECT a.id, a.tenant_id, a.user_id, t.schema_name,
                    CAST(EXTRACT(EPOCH FROM (now() - s.last_seen_at)) AS BIGINT), s.mfa_setup_required
             FROM platform.sessions s
             JOIN platform.accounts a ON a.id = s.account_id
             JOIN platform.tenants t ON t.id = a.tenant_id
             WHERE s.token_hash = ?1 AND s.expires_at > now() AND t.status <> 'closed'",
            params![hash],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
        )
        .optional()?;
    let Some((account_id, tenant_id, user_id, schema, idle_secs, mfa_setup)) = row else { return Ok(None) };
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
    Ok(Some(Current { session, tenant_id, account_id, mfa_setup }))
}

fn new_session(store: &Store, account_id: &str, req: &Request) -> Result<String> {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    let token = hex::encode(b);
    let acc = accounts::account_by_id(&store.conn, account_id)?;
    store.conn.use_schema(&acc.schema)?;
    let mfa_setup = accounts::mfa_required(store, &acc, &store.session_for(&acc.user_id)?)?;
    store.conn.execute(
        "INSERT INTO platform.sessions (token_hash, account_id, expires_at, ip, user_agent, mfa_setup_required)
         VALUES (?1, ?2, now() + make_interval(hours => ?3), ?4, ?5, ?6)",
        params![
            sha256_hex(token.as_bytes()),
            account_id,
            SESSION_HOURS as i32,
            req.ip,
            req.header("user-agent").unwrap_or("").chars().take(300).collect::<String>(),
            mfa_setup
        ],
    )?;
    Ok(token)
}

// ---------------------------------------------------------------- accounts

pub fn valid_email(e: &str) -> bool {
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
    lunda_core::auth::validate_password_for(password, &[&email, name, organization])?;
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
    accounts::send_verification(&t, &account_id, &email, req)?;
    t.commit()?;
    new_session(store, &account_id, req)
}

pub enum LoginStep {
    Session(String),
    /// Password was right; a code from the authenticator app is needed (challenge token).
    SecondFactor(String),
}

pub fn login(store: &Store, email: &str, password: &str, req: &Request) -> Result<LoginStep> {
    let email = email.trim().to_lowercase();
    let acc = accounts::account_by_email(&store.conn, &email)?;
    let Some(acc) = acc else {
        // same work as a wrong password, so the response time does not reveal accounts
        let _ = lunda_core::auth::verify_password(password, "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$1JqfI3pVQSrr7CFZQZD8Ewn5vYkM0+DtB4eMZ5yYO3w");
        return Err(CoreError::new("invalid_credentials", "invalid e-mail address or password"));
    };
    let status: String = store.conn.query_row("SELECT status FROM platform.tenants WHERE id = ?1", params![acc.tenant_id], |r| r.get(0))?;
    if status == "closed" {
        return Err(CoreError::new("account_closed", "this account has been closed"));
    }
    store.conn.use_schema(&acc.schema)?;
    store.login(&email, password)?; // checks the password and lockout, audit-logs the login
    if acc.totp_enabled {
        return Ok(LoginStep::SecondFactor(accounts::create_token(&store.conn, &acc.id, "mfa", accounts::MFA_MINUTES)?));
    }
    Ok(LoginStep::Session(new_session(store, &acc.id, req)?))
}

// ---------------------------------------------------------------- static files

/// Content Security Policy of the app (also set by vercel.json in production).
pub const APP_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; font-src 'self'; connect-src 'self' blob:; frame-src 'self' blob:; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

fn serve_static(dir: &Path, path: &str) -> Response {
    let rel = path.trim_start_matches('/');
    let mut p = dir.join(if rel.is_empty() { "index.html" } else { rel });
    // app-only build (development): website pages such as the grave search
    // embed come from the website folder next to it
    let site = dir.join("../website").join(if rel.contains('.') { rel.to_string() } else { format!("{rel}.html") });
    if !rel.is_empty() && !rel.contains("..") && !p.is_file() && site.is_file() {
        p = site;
    }
    if rel.contains("..") || !p.is_file() {
        // single-page app: /app/… (assembled site) or the root (app only)
        let app = dir.join("app/index.html");
        p = if rel.starts_with("app/") && app.is_file() { app } else { dir.join("index.html") };
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
    // the app page gets the same security headers as on Vercel (vercel.json)
    // (recognised by its mount point, so a stale build folder does not matter)
    let is_app = |b: &[u8]| {
        const ROOT: &[u8] = b"<div id=\"root\"";
        ct.starts_with("text/html") && b.windows(ROOT.len()).any(|w| w == ROOT)
    };
    match std::fs::read(&p) {
        Ok(b) if is_app(&b) => Response {
            status: 200,
            headers: vec![
                ("Content-Type".into(), ct.into()),
                ("Content-Security-Policy".into(), APP_CSP.into()),
                ("Permissions-Policy".into(), "camera=(self), microphone=(), geolocation=(self), payment=(), usb=()".into()),
                ("X-Content-Type-Options".into(), "nosniff".into()),
            ],
            body: b,
        },
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
        t.execute("DELETE FROM platform.tokens WHERE account_id IN (SELECT id FROM platform.accounts WHERE tenant_id = ?1)", params![id])?;
        t.execute("DELETE FROM platform.accounts WHERE tenant_id = ?1", params![id])?;
        t.execute("DELETE FROM platform.tenants WHERE id = ?1", params![id])?;
        t.execute_batch(&format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"))?;
        t.commit()?;
    }
    Ok(())
}

// ---------------------------------------------------------------- platform admin, webhooks

/// TEMISZ operators (comma-separated SUPERADMIN_EMAILS) see every customer's billing.
pub fn is_superadmin(email: &str) -> bool {
    std::env::var("SUPERADMIN_EMAILS").map(|v| v.split(',').any(|e| e.trim().eq_ignore_ascii_case(email))).unwrap_or(false)
}

fn admin_overview(store: &Store) -> Result<Value> {
    let rows: Vec<Value> = store
        .conn
        .prepare(
            "SELECT t.id, t.name, t.status, to_char(t.trial_ends_at, 'YYYY-MM-DD'), to_char(t.created_at, 'YYYY-MM-DD'), t.billing_email,
                    s.plan, s.payment_method, s.cemetery_count, s.status, to_char(s.current_period_end, 'YYYY-MM-DD'),
                    (SELECT count(*) FROM platform.accounts a WHERE a.tenant_id = t.id)
             FROM platform.tenants t LEFT JOIN platform.subscriptions s ON s.tenant_id = t.id
             ORDER BY t.created_at DESC LIMIT 500",
        )?
        .query_map([], |r| {
            Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "status": r.get::<_, String>(2)?, "trialEndsAt": r.get::<_, String>(3)?,
                       "createdAt": r.get::<_, String>(4)?, "email": r.get::<_, String>(5)?, "plan": r.get::<_, Option<String>>(6)?,
                       "paymentMethod": r.get::<_, Option<String>>(7)?, "cemeteryCount": r.get::<_, Option<i64>>(8)?,
                       "subscriptionStatus": r.get::<_, Option<String>>(9)?, "periodEnd": r.get::<_, Option<String>>(10)?, "users": r.get::<_, i64>(11)? }))
        })?
        .collect::<std::result::Result<_, _>>()?;
    let open: Vec<Value> = store
        .conn
        .prepare(
            "SELECT i.id, t.name, i.number, i.gross_huf, to_char(i.due_date, 'YYYY-MM-DD'), i.description, substr(i.tenant_id, 1, 8)
             FROM platform.invoices i JOIN platform.tenants t ON t.id = i.tenant_id
             WHERE i.kind = 'proforma' AND i.status = 'open' ORDER BY i.due_date",
        )?
        .query_map([], |r| {
            Ok(json!({ "id": r.get::<_, String>(0)?, "tenant": r.get::<_, String>(1)?, "number": r.get::<_, String>(2)?, "grossHuf": r.get::<_, i64>(3)?,
                       "dueDate": r.get::<_, Option<String>>(4)?, "description": r.get::<_, String>(5)?, "reference": r.get::<_, String>(6)? }))
        })?
        .collect::<std::result::Result<_, _>>()?;
    Ok(json!({ "tenants": rows, "openProformas": open, "bankLines": billing::bank_lines(&store.conn)? }))
}

#[cfg(feature = "desktop-import")]
fn import_desktop(store: &Store, c: &Current, content_b64: &str, recovery_key: &str) -> Result<Value> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(content_b64.trim()).map_err(|_| CoreError::new("bad_request", "file content"))?;
    let path = std::env::temp_dir().join(format!("temisz-import-{}.ltbak", new_id()));
    std::fs::write(&path, bytes)?;
    let tables = temisz_desktop_import::read_backup(&path, recovery_key);
    let _ = std::fs::remove_file(&path);
    tenant_data::import_desktop(store, &c.session, &c.tenant_id, tables?)
}

#[cfg(not(feature = "desktop-import"))]
fn import_desktop(_: &Store, _: &Current, _: &str, _: &str) -> Result<Value> {
    Err(CoreError::new("not_available", "desktop backups are loaded by TEMISZ support"))
}
