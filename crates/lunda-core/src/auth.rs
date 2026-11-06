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
    /// Pénztáros (csak a házipénztár)
    Cashier,
    /// Temetői dolgozó (munkalapok)
    Worker,
    /// Meghívott temetkezési iroda (közös temetési naptár)
    Undertaker,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Clerk => "clerk",
            Role::Finance => "finance",
            Role::Reader => "reader",
            Role::Cashier => "cashier",
            Role::Worker => "worker",
            Role::Undertaker => "undertaker",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "admin" => Role::Admin,
            "clerk" => Role::Clerk,
            "finance" => Role::Finance,
            "reader" => Role::Reader,
            "cashier" => Role::Cashier,
            "worker" => Role::Worker,
            "undertaker" => Role::Undertaker,
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
                CashDesk,
                Operations,
                CompleteWork,
                Calendar,
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
            Role::Clerk => &[
                ManageStructure,
                RecordEvents,
                RecordLeases,
                RecordPayments,
                CashDesk,
                Operations,
                CompleteWork,
                Calendar,
                ViewPersonal,
                Print,
                Export,
                Import,
                EditMap,
                Backup,
            ],
            Role::Finance => &[RecordPayments, CashDesk, ViewPersonal, Print, Export],
            // FR-PEN-10: the cash desk only
            Role::Cashier => &[CashDesk, Print],
            // FR-UZE-01: work orders only
            Role::Worker => &[CompleteWork, Print],
            // a funeral office invited into the account: the shared calendar only
            Role::Undertaker => &[Calendar],
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
    /// házipénztár (FR-PEN-06)
    CashDesk,
    /// naptár, munkalapok, vállalkozók, ügyiratok, bejárás (W8–W11)
    Operations,
    /// munkalap elvégzettnek jelölése
    CompleteWork,
    /// közös temetési naptár (időpont foglalása, a saját foglalás módosítása)
    Calendar,
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
            Permission::CashDesk => "cash_desk",
            Permission::Operations => "operations",
            Permission::CompleteWork => "complete_work",
            Permission::Calendar => "calendar",
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

/// Shortest accepted password (NIST SP 800-63B: length over complexity rules).
pub const MIN_PASSWORD_LEN: usize = 10;

/// Passwords seen in every leaked list, plus the obvious Hungarian ones.
const COMMON_PASSWORDS: &[&str] = &[
    "1234567890",
    "12345678910",
    "0123456789",
    "1111111111",
    "0000000000",
    "1234512345",
    "9876543210",
    "qwertzuiop",
    "qwertyuiop",
    "asdfghjkl1",
    "password12",
    "password123",
    "password1234",
    "passw0rd123",
    "jelszo1234",
    "jelszó1234",
    "jelszo12345",
    "jelszavam1",
    "jelszavam123",
    "titkosjelszo",
    "iloveyou12",
    "princess12",
    "football12",
    "1q2w3e4r5t",
    "1qaz2wsx3edc",
    "qwerty1234",
    "qwerty12345",
    "abcdefghij",
    "abc1234567",
    "admin12345",
    "administrator",
    "adminadmin",
    "letmein123",
    "welcome123",
    "monkey1234",
    "dragon1234",
    "sunshine12",
    "superman12",
    "trustno123",
    "zaq12wsxcde",
    "temeto1234",
    "temetes123",
    "temisz1234",
    "temisz12345",
    "budapest12",
    "budapest123",
    "magyarorszag",
    "szeretlek1",
    "szeretlek12",
    "123456789a",
    "a123456789",
    "aaaaaaaaaa",
    "abcabcabc1",
    "kutyacica1",
    "macska1234",
    "nyilvantartas",
    "onkormanyzat",
    "polgarmester",
    "temetogondnok",
];

pub fn validate_password(password: &str) -> Result<()> {
    validate_password_for(password, &[])
}

/// Length, not a well-known password, not mostly the user's own name,
/// e-mail or organisation (`context`).
pub fn validate_password_for(password: &str, context: &[&str]) -> Result<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(CoreError::new("password_too_short", "password is too short").with("min", MIN_PASSWORD_LEN));
    }
    let norm = crate::text::normalize(password).replace(' ', "");
    let distinct = password.chars().collect::<std::collections::HashSet<_>>().len();
    if COMMON_PASSWORDS.iter().any(|c| crate::text::normalize(c) == norm) || distinct < 4 {
        return Err(CoreError::new("password_common", "this password is too easy to guess"));
    }
    for c in context {
        let c = crate::text::normalize(c.split('@').next().unwrap_or(c)).replace(' ', "");
        if c.chars().count() >= 4 && norm.contains(&c) && norm.chars().count() < c.chars().count() + 4 {
            return Err(CoreError::new("password_personal", "the password is too close to your name or e-mail"));
        }
    }
    Ok(())
}

pub const MAX_FAILED_ATTEMPTS: i64 = 5;
pub const LOCKOUT_MINUTES: i64 = 15;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_rules() {
        assert_eq!(validate_password("rovid-123").unwrap_err().code, "password_too_short");
        assert_eq!(validate_password("Jelszo1234").unwrap_err().code, "password_common");
        assert_eq!(validate_password("aaaaabbbbbaaaaa").unwrap_err().code, "password_common");
        assert_eq!(validate_password_for("kovacs.janos1", &["kovacs.janos@pelda.hu"]).unwrap_err().code, "password_personal");
        validate_password_for("hosszú kék kerítés 7", &["kovacs.janos@pelda.hu"]).unwrap();
    }

    #[test]
    fn argon2_roundtrip() {
        let h = hash_password("titkos-kert-kapu").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("titkos-kert-kapu", &h));
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
