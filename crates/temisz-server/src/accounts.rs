//! Account lifecycle on the platform level (W2): e-mail verification,
//! password reset, inviting colleagues, two-factor sign-in, rate limits.

use lunda_core::auth::{Permission, Role, Session};
use lunda_core::sql::{params, Connection, OptionalExtension};
use lunda_core::util::{new_id, sha256_hex};
use lunda_core::{db, CoreError, Result, Store};
use serde_json::{json, Value};

use crate::{mail, totp, Request};

pub const VERIFY_HOURS: i32 = 72;
pub const RESET_MINUTES: i32 = 60;
pub const INVITE_HOURS: i32 = 7 * 24;
pub const MFA_MINUTES: i32 = 5;
/// Admins must use two-factor sign-in once the account is a week old or has colleagues.
pub const MFA_GRACE_DAYS: i64 = 7;

// ---------------------------------------------------------------- tokens

fn random_token() -> String {
    use rand::RngCore;
    let mut b = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut b);
    hex::encode(b)
}

/// Creates a one-time token; earlier unused tokens of the same kind are revoked.
pub fn create_token(conn: &Connection, account_id: &str, kind: &str, minutes: i32) -> Result<String> {
    conn.execute("UPDATE platform.tokens SET used_at = now() WHERE account_id = ?1 AND kind = ?2 AND used_at IS NULL", params![account_id, kind])?;
    let token = random_token();
    conn.execute(
        "INSERT INTO platform.tokens (token_hash, kind, account_id, expires_at) VALUES (?1, ?2, ?3, now() + make_interval(mins => ?4))",
        params![sha256_hex(token.as_bytes()), kind, account_id, minutes],
    )?;
    Ok(token)
}

/// Consumes a token and returns its account id.
pub fn use_token(conn: &Connection, token: &str, kind: &str) -> Result<String> {
    let account: Option<String> = conn
        .query_row(
            "UPDATE platform.tokens SET used_at = now()
             WHERE token_hash = ?1 AND kind = ?2 AND used_at IS NULL AND expires_at > now()
             RETURNING account_id",
            params![sha256_hex(token.trim().as_bytes()), kind],
            |r| r.get(0),
        )
        .optional()?;
    account.ok_or_else(|| CoreError::new("token_invalid", "the link is invalid or has expired"))
}

// ---------------------------------------------------------------- rate limits

/// Off only in the local development/E2E server (all requests come from 127.0.0.1).
pub static RATE_LIMITS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Allows at most `max` events per `minutes` for a key (IP, e-mail).
pub fn rate_limit(conn: &Connection, key: &str, max: i64, minutes: i32) -> Result<()> {
    if !RATE_LIMITS.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok(());
    }
    let n: i64 =
        conn.query_row("SELECT count(*) FROM platform.rate_events WHERE key = ?1 AND ts > now() - make_interval(mins => ?2)", params![key, minutes], |r| {
            r.get(0)
        })?;
    if n >= max {
        return Err(CoreError::new("rate_limited", "too many attempts, try again later").with("minutes", minutes));
    }
    conn.execute("INSERT INTO platform.rate_events (key) VALUES (?1)", params![key])?;
    // keep the table small
    if rand::random::<u8>() == 0 {
        conn.execute("DELETE FROM platform.rate_events WHERE ts < now() - interval '1 day'", [])?;
    }
    Ok(())
}

// ---------------------------------------------------------------- helpers

pub struct AccountRow {
    pub id: String,
    pub email: String,
    pub tenant_id: String,
    pub user_id: String,
    pub schema: String,
    pub organization: String,
    pub verified: bool,
    pub totp_secret: Option<String>,
    pub totp_enabled: bool,
    pub age_days: i64,
}

const ACCOUNT_SQL: &str = "SELECT a.id, a.email, a.tenant_id, a.user_id, t.schema_name, t.name, a.email_verified_at IS NOT NULL,
            a.totp_secret, a.totp_enabled_at IS NOT NULL, CAST(EXTRACT(DAY FROM now() - a.created_at) AS BIGINT)
     FROM platform.accounts a JOIN platform.tenants t ON t.id = a.tenant_id";

fn map_account(r: &lunda_core::sql::Row) -> lunda_core::sql::Result<AccountRow> {
    Ok(AccountRow {
        id: r.get(0)?,
        email: r.get(1)?,
        tenant_id: r.get(2)?,
        user_id: r.get(3)?,
        schema: r.get(4)?,
        organization: r.get(5)?,
        verified: r.get(6)?,
        totp_secret: r.get(7)?,
        totp_enabled: r.get(8)?,
        age_days: r.get(9)?,
    })
}

pub fn account_by_id(conn: &Connection, id: &str) -> Result<AccountRow> {
    conn.query_row(&format!("{ACCOUNT_SQL} WHERE a.id = ?1"), params![id], map_account).optional()?.ok_or_else(|| CoreError::not_found("account", id))
}

pub fn account_by_email(conn: &Connection, email: &str) -> Result<Option<AccountRow>> {
    Ok(conn.query_row(&format!("{ACCOUNT_SQL} WHERE lower(a.email) = lower(?1)"), params![email.trim()], map_account).optional()?)
}

pub fn account_by_user(conn: &Connection, tenant_id: &str, user_id: &str) -> Result<Option<AccountRow>> {
    Ok(conn.query_row(&format!("{ACCOUNT_SQL} WHERE a.tenant_id = ?1 AND a.user_id = ?2"), params![tenant_id, user_id], map_account).optional()?)
}

/// Public address of the app for links in e-mails.
/// The website root (the app lives under /app on it).
pub const DEFAULT_APP_URL: &str = "https://www.temisz.hu/app";

pub fn site_url(req: &Request) -> String {
    let b = base_url(req);
    b.strip_suffix("/app").map(String::from).unwrap_or(b)
}

/// Address of the app in e-mailed links: PUBLIC_URL (the app lives under
/// /app, added when missing), else the request's host when it is one of ours.
/// A foreign Host header never ends up in a password link.
pub fn base_url(req: &Request) -> String {
    if let Some(u) = std::env::var("PUBLIC_URL").ok().map(|u| u.trim().trim_end_matches('/').to_string()).filter(|u| !u.is_empty()) {
        return if u.ends_with("/app") { u } else { format!("{u}/app") };
    }
    let host = req.header("x-forwarded-host").or(req.header("host")).unwrap_or("localhost");
    let bare = host.split(':').next().unwrap_or("");
    let ours = matches!(bare, "localhost" | "127.0.0.1") || bare == "temisz.hu" || bare.ends_with(".temisz.hu") || bare.ends_with(".vercel.app");
    if !ours {
        return DEFAULT_APP_URL.to_string();
    }
    let proto = req.header("x-forwarded-proto").unwrap_or(if host.starts_with("127.0.0.1") || host.starts_with("localhost") { "http" } else { "https" });
    format!("{proto}://{host}")
}

/// Whether an admin must set up two-factor sign-in now.
pub fn mfa_required(store: &Store, acc: &AccountRow, session: &Session) -> Result<bool> {
    if session.role != Role::Admin || acc.totp_enabled {
        return Ok(false);
    }
    if acc.age_days >= MFA_GRACE_DAYS {
        return Ok(true);
    }
    let colleagues: i64 = store.conn.query_row("SELECT count(*) FROM platform.accounts WHERE tenant_id = ?1", params![acc.tenant_id], |r| r.get(0))?;
    Ok(colleagues > 1)
}

/// Account part of the status sent to the UI.
pub fn account_status(store: &Store, acc: &AccountRow, mfa_setup: bool) -> Result<Value> {
    let (trial_ends, status, trial_days): (String, String, i64) = store.conn.query_row(
        "SELECT to_char(trial_ends_at AT TIME ZONE 'Europe/Budapest', 'YYYY-MM-DD'), status,
                CAST(CEIL(EXTRACT(EPOCH FROM (trial_ends_at - now())) / 86400) AS BIGINT)
         FROM platform.tenants WHERE id = ?1",
        params![acc.tenant_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let tour_seen: bool = store.conn.query_row("SELECT tour_seen_at IS NOT NULL FROM platform.accounts WHERE id = ?1", params![acc.id], |r| r.get(0))?;
    Ok(json!({
        "tourSeen": tour_seen,
        "email": acc.email,
        "organization": acc.organization,
        "emailVerified": acc.verified,
        "totpEnabled": acc.totp_enabled,
        "totpRequired": mfa_setup && !acc.totp_enabled,
        "tenantStatus": status,
        "trialEndsAt": trial_ends,
        "trialDaysLeft": trial_days.max(0),
    }))
}

// ---------------------------------------------------------------- e-mail verification

pub fn send_verification(conn: &Connection, acc_id: &str, email: &str, req: &Request) -> Result<()> {
    let token = create_token(conn, acc_id, "verify", VERIFY_HOURS * 60)?;
    let link = format!("{}/#megerosites={token}", base_url(req));
    mail::send(
        conn,
        email,
        "Erősítse meg az e-mail-címét",
        &format!(
            "Jó napot kívánunk!\n\nKöszönjük, hogy kipróbálja a TEMISZ-t. Az e-mail-cím megerősítéséhez nyissa meg ezt a hivatkozást:\n\n{link}\n\nA hivatkozás {VERIFY_HOURS} óráig érvényes. Ha nem Ön regisztrált, hagyja figyelmen kívül ezt a levelet.{}",
            mail::FOOTER
        ),
        "verify",
    )
}

pub fn verify_email(conn: &Connection, token: &str) -> Result<()> {
    let acc = use_token(conn, token, "verify")?;
    conn.execute("UPDATE platform.accounts SET email_verified_at = coalesce(email_verified_at, now()) WHERE id = ?1", params![acc])?;
    Ok(())
}

// ---------------------------------------------------------------- password reset

/// Always answers the same way, so it does not reveal which addresses have accounts.
pub fn forgot_password(conn: &Connection, email: &str, req: &Request) -> Result<()> {
    let email = email.trim().to_lowercase();
    rate_limit(conn, &format!("forgot:{}", req.ip), 10, 60)?;
    rate_limit(conn, &format!("forgot:{email}"), 3, 60)?;
    if let Some(acc) = account_by_email(conn, &email)? {
        send_reset(conn, &acc, req)?;
    }
    Ok(())
}

fn send_reset(conn: &Connection, acc: &AccountRow, req: &Request) -> Result<()> {
    let token = create_token(conn, &acc.id, "reset", RESET_MINUTES)?;
    let link = format!("{}/#jelszo={token}", base_url(req));
    mail::send(
        conn,
        &acc.email,
        "Új jelszó beállítása",
        &format!(
            "Jó napot kívánunk!\n\nÚj jelszót kért a TEMISZ-fiókjához ({}). Az új jelszót itt állíthatja be:\n\n{link}\n\nA hivatkozás {RESET_MINUTES} percig érvényes. Ha nem Ön kérte, hagyja figyelmen kívül ezt a levelet, a jelszava nem változik.{}",
            acc.organization,
            mail::FOOTER
        ),
        "reset",
    )
}

/// Sets a new password from a reset or invitation link, ends all sessions of the account.
pub fn set_password_with_token(store: &Store, token: &str, kind: &str, password: &str, display_name: Option<&str>) -> Result<String> {
    lunda_core::auth::validate_password(password)?;
    let t = db::tx(&store.conn)?;
    let acc_id = use_token(&t, token, kind)?;
    let acc = account_by_id(&t, &acc_id)?;
    lunda_core::auth::validate_password_for(password, &[&acc.email, display_name.unwrap_or("")])?;
    t.use_schema(&acc.schema)?;
    store.set_password_system(&acc.user_id, password, display_name)?;
    // the link proves access to the mailbox
    t.execute("UPDATE platform.accounts SET email_verified_at = coalesce(email_verified_at, now()) WHERE id = ?1", params![acc.id])?;
    t.execute("DELETE FROM platform.sessions WHERE account_id = ?1", params![acc.id])?;
    t.commit()?;
    Ok(acc.id)
}

// ---------------------------------------------------------------- invitations

/// Creates a colleague (tenant user + account) and e-mails an invitation.
pub fn invite(store: &Store, s: &Session, tenant_id: &str, args: &Value, req: &Request) -> Result<Value> {
    s.require(Permission::ManageUsers)?;
    let email = args["username"].as_str().or(args["email"].as_str()).unwrap_or("").trim().to_lowercase();
    if !crate::valid_email(&email) {
        return Err(CoreError::validation("email", "invalid e-mail address").with("code", "email_invalid"));
    }
    if account_by_email(&store.conn, &email)?.is_some() {
        return Err(CoreError::new("email_taken", "an account already exists for this e-mail address"));
    }
    let display = args["displayName"].as_str().unwrap_or("").trim().to_string();
    let role = args["role"].as_str().unwrap_or("clerk");
    let extra: Vec<String> = serde_json::from_value(args.get("extraPermissions").cloned().unwrap_or(json!([]))).unwrap_or_default();
    let t = db::tx(&store.conn)?;
    // unusable random password until the invitation is accepted
    let user = store.create_user(s, &email, &display, role, &extra, &format!("{}!Aa1", random_token()))?;
    let acc_id = new_id();
    t.execute("INSERT INTO platform.accounts (id, email, tenant_id, user_id) VALUES (?1, ?2, ?3, ?4)", params![acc_id, email, tenant_id, user.id])?;
    let token = create_token(&t, &acc_id, "invite", INVITE_HOURS * 60)?;
    let org: String = t.query_row("SELECT name FROM platform.tenants WHERE id = ?1", params![tenant_id], |r| r.get(0))?;
    let link = format!("{}/#meghivas={token}", base_url(req));
    mail::send(
        &t,
        &email,
        &format!("Meghívó a TEMISZ-be – {org}"),
        &format!(
            "Jó napot kívánunk!\n\n{} meghívta Önt, hogy a TEMISZ-ben dolgozzon ({org}). A fiók aktiválásához és a jelszó beállításához nyissa meg ezt a hivatkozást:\n\n{link}\n\nA meghívó 7 napig érvényes.{}",
            s.display_name,
            mail::FOOTER
        ),
        "invite",
    )?;
    t.commit()?;
    Ok(serde_json::to_value(user)?)
}

/// Re-sends an invitation or a password reset link to a colleague (admin action).
pub fn send_access_link(store: &Store, s: &Session, tenant_id: &str, user_id: &str, req: &Request) -> Result<()> {
    s.require(Permission::ManageUsers)?;
    let acc = account_by_user(&store.conn, tenant_id, user_id)?.ok_or_else(|| CoreError::not_found("account", user_id))?;
    send_reset(&store.conn, &acc, req)
}

// ---------------------------------------------------------------- two-factor sign-in

pub fn totp_setup(store: &Store, acc: &AccountRow) -> Result<Value> {
    if acc.totp_enabled {
        return Err(CoreError::new("totp_already_enabled", "two-factor sign-in is already on"));
    }
    let secret = totp::new_secret();
    store.conn.execute("UPDATE platform.accounts SET totp_secret = ?2 WHERE id = ?1", params![acc.id, secret])?;
    let uri = totp::uri(&secret, &acc.email);
    let qr = qrcode::QrCode::new(uri.as_bytes()).map_err(|e| CoreError::new("qr", e.to_string()))?;
    let svg = qr.render::<qrcode::render::svg::Color>().min_dimensions(200, 200).quiet_zone(true).build();
    Ok(json!({ "secret": secret, "uri": uri, "qrSvg": svg }))
}

pub fn totp_enable(store: &Store, acc: &AccountRow, s: &Session, code: &str) -> Result<Value> {
    let secret = acc.totp_secret.clone().ok_or_else(|| CoreError::new("totp_not_setup", "start the setup first"))?;
    if !totp::verify(&secret, code, now_unix()) {
        return Err(CoreError::new("totp_invalid", "the code is not correct"));
    }
    let codes: Vec<String> = (0..8).map(|_| random_token()[..10].to_uppercase()).collect();
    let hashes: Vec<String> = codes.iter().map(|c| sha256_hex(c.as_bytes())).collect();
    store
        .conn
        .execute("UPDATE platform.accounts SET totp_enabled_at = now(), recovery_codes = ?2 WHERE id = ?1", params![acc.id, serde_json::to_string(&hashes)?])?;
    store.conn.execute("UPDATE platform.sessions SET mfa_setup_required = false WHERE account_id = ?1", params![acc.id])?;
    lunda_core::audit::record_simple(&store.conn, s, "user", s.user_id.as_deref().unwrap_or(""), "totp_enabled", None)?;
    Ok(json!({ "recoveryCodes": codes }))
}

pub fn totp_disable(store: &Store, acc: &AccountRow, s: &Session, password: &str) -> Result<()> {
    store.verify_own_password(s, password)?;
    store.conn.execute("UPDATE platform.accounts SET totp_secret = NULL, totp_enabled_at = NULL, recovery_codes = '[]' WHERE id = ?1", params![acc.id])?;
    lunda_core::audit::record_simple(&store.conn, s, "user", s.user_id.as_deref().unwrap_or(""), "totp_disabled", None)?;
    Ok(())
}

/// Second step of sign-in: a code from the app or a one-time recovery code.
pub fn check_second_factor(conn: &Connection, acc: &AccountRow, code: &str) -> Result<()> {
    let code = code.trim();
    if let Some(secret) = &acc.totp_secret {
        if totp::verify(secret, code, now_unix()) {
            return Ok(());
        }
    }
    let stored: String = conn.query_row("SELECT recovery_codes FROM platform.accounts WHERE id = ?1", params![acc.id], |r| r.get(0))?;
    let mut hashes: Vec<String> = serde_json::from_str(&stored).unwrap_or_default();
    let h = sha256_hex(code.to_uppercase().replace([' ', '-'], "").as_bytes());
    if let Some(i) = hashes.iter().position(|x| *x == h) {
        hashes.remove(i);
        conn.execute("UPDATE platform.accounts SET recovery_codes = ?2 WHERE id = ?1", params![acc.id, serde_json::to_string(&hashes)?])?;
        return Ok(());
    }
    Err(CoreError::new("totp_invalid", "the code is not correct"))
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

// ---------------------------------------------------------------- security log

/// Records a sign-in or account security change with IP address and browser.
/// Never fails the request it belongs to.
pub fn security_event(conn: &Connection, req: &Request, email: &str, kind: &str) {
    let email = email.trim().to_lowercase();
    let ua: String = req.header("user-agent").unwrap_or("").chars().take(300).collect();
    let _ = conn.execute(
        "INSERT INTO platform.security_log (tenant_id, account_id, email, kind, ip, user_agent)
         SELECT a.tenant_id, a.id, ?1, ?2, ?3, ?4 FROM (SELECT 1) one LEFT JOIN platform.accounts a ON lower(a.email) = ?1",
        params![email.chars().take(254).collect::<String>(), kind, req.ip, ua],
    );
    let _ = conn.execute("DELETE FROM platform.security_log WHERE at < now() - interval '1 year'", []);
}

/// The organisation's last 200 security events (administrators).
pub fn security_log(conn: &Connection, tenant_id: &str) -> Result<Value> {
    let text: String = conn.query_row(
        "SELECT coalesce(json_agg(json_build_object('at', at, 'email', email, 'kind', kind, 'ip', ip, 'userAgent', user_agent) ORDER BY at DESC), '[]')::text
         FROM (SELECT * FROM platform.security_log WHERE tenant_id = ?1 ORDER BY at DESC LIMIT 200) l",
        params![tenant_id],
        |r| r.get(0),
    )?;
    Ok(serde_json::from_str(&text)?)
}
