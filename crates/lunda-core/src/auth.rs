//! Roles, permissions and password hashing (FRS 3., NFR-BIZ-02).

use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Rendszergazda
    Admin,
    /// Temetőgondnok / ügyintéző
    Clerk,
    /// Pénzügyes
    Finance,
    /// Olvasó
    Reader,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Clerk => "clerk",
            Role::Finance => "finance",
            Role::Reader => "reader",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "admin" => Role::Admin,
            "clerk" => Role::Clerk,
            "finance" => Role::Finance,
            "reader" => Role::Reader,
            _ => return Err(CoreError::validation("role", format!("unknown role {s}"))),
        })
    }

    pub fn default_permissions(&self) -> &'static [Permission] {
        use Permission::*;
        match self {
            Role::Admin => &[
                ManageUsers,
                ManageSettings,
                ManageStructure,
                RecordEvents,
                RecordLeases,
                RecordPayments,
                CorrectEntries,
                VoidEntries,
                ViewPersonal,
                Print,
                Export,
                Import,
                Backup,
                EditMap,
                OverrideChecks,
            ],
            Role::Clerk => &[ManageStructure, RecordEvents, RecordLeases, RecordPayments, ViewPersonal, Print, Export, Import, EditMap, Backup],
            Role::Finance => &[RecordPayments, ViewPersonal, Print, Export],
            Role::Reader => &[Print, Export],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ManageUsers,
    ManageSettings,
    ManageStructure,
    RecordEvents,
    RecordLeases,
    RecordPayments,
    /// bejegyzés javítása (FR-JOG-03)
    CorrectEntries,
    /// érvénytelenítés (FR-NAP-02)
    VoidEntries,
    /// személyes adatok megtekintése (FR-JOG-04)
    ViewPersonal,
    Print,
    Export,
    Import,
    Backup,
    EditMap,
    /// temethetőségi ellenőrzés felülbírálása indoklással
    OverrideChecks,
}

impl Permission {
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::ManageUsers => "manage_users",
            Permission::ManageSettings => "manage_settings",
            Permission::ManageStructure => "manage_structure",
            Permission::RecordEvents => "record_events",
            Permission::RecordLeases => "record_leases",
            Permission::RecordPayments => "record_payments",
            Permission::CorrectEntries => "correct_entries",
            Permission::VoidEntries => "void_entries",
            Permission::ViewPersonal => "view_personal",
            Permission::Print => "print",
            Permission::Export => "export",
            Permission::Import => "import",
            Permission::Backup => "backup",
            Permission::EditMap => "edit_map",
            Permission::OverrideChecks => "override_checks",
        }
    }
}

/// The authenticated user performing an operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub user_id: Option<String>,
    pub user_name: String,
    pub display_name: String,
    pub role: Role,
    pub permissions: Vec<Permission>,
}

impl Session {
    pub fn for_role(user_id: Option<String>, user_name: &str, display_name: &str, role: Role, extra: &[Permission]) -> Self {
        let mut permissions: Vec<Permission> = role.default_permissions().to_vec();
        for p in extra {
            if !permissions.contains(p) {
                permissions.push(*p);
            }
        }
        Self { user_id, user_name: user_name.into(), display_name: display_name.into(), role, permissions }
    }

    /// Internal system actor (migrations, scheduled backups, sample data).
    pub fn system() -> Self {
        Self::for_role(None, "system", "Rendszer", Role::Admin, &[])
    }

    pub fn can(&self, p: Permission) -> bool {
        self.permissions.contains(&p)
    }

    pub fn require(&self, p: Permission) -> Result<()> {
        if self.can(p) {
            Ok(())
        } else {
            Err(CoreError::forbidden(p.as_str()))
        }
    }
}

pub fn hash_password(password: &str) -> Result<String> {
    validate_password(password)?;
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(password.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| CoreError::new("password_hash", e.to_string()))
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

pub fn validate_password(password: &str) -> Result<()> {
    if password.chars().count() < 8 {
        return Err(CoreError::new("password_too_short", "password must be at least 8 characters").with("min", 8));
    }
    Ok(())
}

pub const MAX_FAILED_ATTEMPTS: i64 = 5;
pub const LOCKOUT_MINUTES: i64 = 15;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_roundtrip() {
        let h = hash_password("titkos-jelszo").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("titkos-jelszo", &h));
        assert!(!verify_password("rossz-jelszo", &h));
        assert!(!verify_password("x", "not-a-hash"));
    }

    #[test]
    fn short_password_rejected() {
        assert_eq!(hash_password("rövid").unwrap_err().code, "password_too_short");
    }

    #[test]
    fn reader_cannot_modify_or_see_personal_data() {
        let s = Session::for_role(None, "olvaso", "Olvasó", Role::Reader, &[]);
        assert!(!s.can(Permission::RecordEvents));
        assert!(!s.can(Permission::ViewPersonal));
        assert!(s.can(Permission::Export));
        assert_eq!(s.require(Permission::RecordEvents).unwrap_err().code, "forbidden");
    }

    #[test]
    fn finance_cannot_modify_burials() {
        let s = Session::for_role(None, "penz", "Pénzügy", Role::Finance, &[]);
        assert!(s.can(Permission::RecordPayments));
        assert!(!s.can(Permission::RecordEvents));
    }

    #[test]
    fn extra_permissions_are_added() {
        let s = Session::for_role(None, "u", "U", Role::Clerk, &[Permission::CorrectEntries]);
        assert!(s.can(Permission::CorrectEntries));
        assert!(!Session::for_role(None, "u", "U", Role::Clerk, &[]).can(Permission::CorrectEntries));
    }
}
