//! Users, login with lockout (FR-JOG-01..02, NFR-BIZ-02).

use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, OptionalExtension};

use super::*;
use crate::audit;
use crate::auth::{hash_password, verify_password, Permission, Role, Session, LOCKOUT_MINUTES, MAX_FAILED_ATTEMPTS};
use crate::util::{new_id, now_ts};

fn map_user(r: &rusqlite::Row) -> rusqlite::Result<User> {
    let extra: String = r.get("extra_permissions")?;
    Ok(User {
        id: r.get("id")?,
        username: r.get("username")?,
        display_name: r.get("display_name")?,
        role: r.get("role")?,
        extra_permissions: serde_json::from_str(&extra).unwrap_or_default(),
        active: r.get("active")?,
        locked_until: r.get("locked_until")?,
        must_change_password: r.get("must_change_password")?,
        last_login_at: r.get("last_login_at")?,
    })
}

fn parse_perms(v: &[String]) -> Vec<Permission> {
    v.iter().filter_map(|p| serde_json::from_value(serde_json::Value::String(p.clone())).ok()).collect()
}

impl Store {
    pub fn user_count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?)
    }

    pub fn list_users(&self, s: &Session) -> Result<Vec<User>> {
        s.require(Permission::ManageUsers)?;
        let mut st = self.conn.prepare("SELECT * FROM users ORDER BY display_name COLLATE HU")?;
        let rows = st.query_map([], map_user)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn get_user(&self, id: &str) -> Result<User> {
        self.conn
            .query_row("SELECT * FROM users WHERE id=?1", [id], map_user)
            .optional()?
            .ok_or_else(|| CoreError::not_found("user", id))
    }

    /// First-run: the initial administrator (only when there are no users).
    pub fn create_initial_admin(&self, username: &str, display_name: &str, password: &str) -> Result<User> {
        if self.user_count()? > 0 {
            return Err(CoreError::new("already_initialized", "users already exist"));
        }
        self.insert_user(&Session::system(), username, display_name, Role::Admin, &[], password, false)
    }

    pub fn create_user(&self, s: &Session, username: &str, display_name: &str, role: &str, extra: &[String], password: &str) -> Result<User> {
        s.require(Permission::ManageUsers)?;
        self.insert_user(s, username, display_name, Role::parse(role)?, extra, password, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_user(&self, s: &Session, username: &str, display_name: &str, role: Role, extra: &[String], password: &str, must_change: bool) -> Result<User> {
        let username = username.trim();
        if username.len() < 2 || username.contains(char::is_whitespace) {
            return Err(CoreError::validation("username", "invalid user name"));
        }
        non_empty("displayName", display_name)?;
        let hash = hash_password(password)?;
        let id = new_id();
        let ts = now_ts();
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "INSERT INTO users (id, username, display_name, role, extra_permissions, password_hash, must_change_password, created_at, created_by, updated_at, updated_by)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?8,?9)",
            params![id, username, display_name.trim(), role.as_str(), serde_json::to_string(extra)?, hash, must_change, ts, s.user_name],
        )?;
        let u = tx.query_row("SELECT * FROM users WHERE id=?1", [&id], map_user)?;
        audit::record(&tx, s, "user", &id, "create", None::<&()>, Some(&u), None)?;
        tx.commit()?;
        Ok(u)
    }

    pub fn update_user(&self, s: &Session, id: &str, display_name: &str, role: &str, extra: &[String], active: bool) -> Result<User> {
        s.require(Permission::ManageUsers)?;
        let old = self.get_user(id)?;
        let role = Role::parse(role)?;
        // never lock out the last active administrator
        if (old.role == "admin") && (role != Role::Admin || !active) {
            let admins: i64 = self.conn.query_row("SELECT count(*) FROM users WHERE role='admin' AND active=1 AND id<>?1", [id], |r| r.get(0))?;
            if admins == 0 {
                return Err(CoreError::new("last_admin", "at least one active administrator is required"));
            }
        }
        let tx = crate::db::tx(&self.conn)?;
        tx.execute(
            "UPDATE users SET display_name=?2, role=?3, extra_permissions=?4, active=?5, updated_at=?6, updated_by=?7 WHERE id=?1",
            params![id, display_name.trim(), role.as_str(), serde_json::to_string(extra)?, active, now_ts(), s.user_name],
        )?;
        let new = tx.query_row("SELECT * FROM users WHERE id=?1", [id], map_user)?;
        audit::record(&tx, s, "user", id, "update", Some(&old), Some(&new), None)?;
        tx.commit()?;
        Ok(new)
    }

    /// Login with lockout after 5 failed attempts for 15 minutes.
    pub fn login(&self, username: &str, password: &str) -> Result<Session> {
        let row: Option<(String, String, String, String, String, bool, i64, Option<String>)> = self
            .conn
            .query_row(
                "SELECT id, username, display_name, role, password_hash, active, failed_attempts, locked_until FROM users WHERE username=?1",
                [username.trim()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
            )
            .optional()?;
        let invalid = || CoreError::new("invalid_credentials", "invalid user name or password");
        let Some((id, uname, display, role, hash, active, failed, locked_until)) = row else {
            // constant-ish work to avoid user enumeration by timing
            let _ = verify_password(password, "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$1JqfI3pVQSrr7CFZQZD8Ewn5vYkM0+DtB4eMZ5yYO3w");
            return Err(invalid());
        };
        if !active {
            return Err(CoreError::new("user_inactive", "user is inactive"));
        }
        if let Some(lu) = &locked_until {
            if let Ok(t) = DateTime::parse_from_rfc3339(lu) {
                if t.with_timezone(&Utc) > Utc::now() {
                    return Err(CoreError::new("user_locked", "too many failed attempts").with("until", lu));
                }
            }
        }
        let sys = Session { user_id: Some(id.clone()), user_name: uname.clone(), display_name: display.clone(), role: Role::parse(&role)?, permissions: vec![] };
        if !verify_password(password, &hash) {
            let failed = if locked_until.is_some() { 1 } else { failed + 1 };
            let lock = if failed >= MAX_FAILED_ATTEMPTS {
                Some((Utc::now() + Duration::minutes(LOCKOUT_MINUTES)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            } else {
                None
            };
            self.conn.execute(
                "UPDATE users SET failed_attempts=?2, locked_until=?3 WHERE id=?1",
                params![id, if lock.is_some() { 0 } else { failed }, lock],
            )?;
            audit::record_simple(&self.conn, &sys, "user", &id, "login_failed", lock.as_deref())?;
            return Err(match lock {
                Some(l) => CoreError::new("user_locked", "too many failed attempts").with("until", l),
                None => invalid().with("remaining", MAX_FAILED_ATTEMPTS - failed),
            });
        }
        self.conn.execute(
            "UPDATE users SET failed_attempts=0, locked_until=NULL, last_login_at=?2 WHERE id=?1",
            params![id, now_ts()],
        )?;
        audit::record_simple(&self.conn, &sys, "user", &id, "login", None)?;
        self.session_for(&id)
    }

    pub fn session_for(&self, user_id: &str) -> Result<Session> {
        let u = self.get_user(user_id)?;
        if !u.active {
            return Err(CoreError::new("user_inactive", "user is inactive"));
        }
        Ok(Session::for_role(Some(u.id.clone()), &u.username, &u.display_name, Role::parse(&u.role)?, &parse_perms(&u.extra_permissions)))
    }

    pub fn change_password(&self, s: &Session, user_id: &str, old_password: Option<&str>, new_password: &str) -> Result<()> {
        let self_change = s.user_id.as_deref() == Some(user_id);
        if !self_change {
            s.require(Permission::ManageUsers)?;
        }
        let hash: String = self.conn.query_row("SELECT password_hash FROM users WHERE id=?1", [user_id], |r| r.get(0))?;
        if self_change && !verify_password(old_password.unwrap_or(""), &hash) {
            return Err(CoreError::new("invalid_credentials", "current password is wrong"));
        }
        let new_hash = hash_password(new_password)?;
        self.conn.execute(
            "UPDATE users SET password_hash=?2, must_change_password=?3, failed_attempts=0, locked_until=NULL, updated_at=?4, updated_by=?5 WHERE id=?1",
            params![user_id, new_hash, !self_change, now_ts(), s.user_name],
        )?;
        audit::record_simple(&self.conn, s, "user", user_id, "password_change", None)?;
        Ok(())
    }

    pub fn unlock_user(&self, s: &Session, user_id: &str) -> Result<()> {
        s.require(Permission::ManageUsers)?;
        self.conn.execute("UPDATE users SET failed_attempts=0, locked_until=NULL WHERE id=?1", [user_id])?;
        audit::record_simple(&self.conn, s, "user", user_id, "unlock", None)?;
        Ok(())
    }
}
