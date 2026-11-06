//! Error type shared by every core operation.
//!
//! Each error carries a stable machine-readable `code` that the UI maps to a
//! Hungarian message through the i18n file (`errors.<code>`), plus optional
//! parameters for interpolation.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error, Serialize, Clone, PartialEq)]
#[error("{code}: {message}")]
pub struct CoreError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
}

impl CoreError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { code: code.to_string(), message: message.into(), params: BTreeMap::new() }
    }

    pub fn with(mut self, key: &str, value: impl ToString) -> Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }

    pub fn not_found(entity: &str, id: &str) -> Self {
        Self::new("not_found", format!("{entity} not found: {id}")).with("entity", entity)
    }

    pub fn forbidden(permission: &str) -> Self {
        Self::new("forbidden", format!("missing permission: {permission}")).with("permission", permission)
    }

    pub fn validation(field: &str, message: impl Into<String>) -> Self {
        Self::new("validation", message).with("field", field)
    }

    pub fn reason_required() -> Self {
        Self::new("reason_required", "a justification is required for this operation")
    }
}

impl From<crate::sql::Error> for CoreError {
    fn from(e: crate::sql::Error) -> Self {
        let msg = e.to_string();
        if msg.contains("physical delete forbidden") {
            return CoreError::new("delete_forbidden", msg);
        }
        if msg.contains("cash day is closed") {
            return CoreError::new("cash_day_closed", msg);
        }
        if msg.contains("cash closing is final") || msg.contains("immutable") || msg.contains("voided record is final") {
            return CoreError::new("immutable", msg);
        }
        if e.sqlstate() == Some("23505") {
            return CoreError::new("duplicate", msg);
        }
        CoreError::new("database", msg)
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(e: serde_json::Error) -> Self {
        CoreError::new("serialization", e.to_string())
    }
}

impl From<std::io::Error> for CoreError {
    fn from(e: std::io::Error) -> Self {
        CoreError::new("io", e.to_string())
    }
}

impl From<chrono::ParseError> for CoreError {
    fn from(e: chrono::ParseError) -> Self {
        CoreError::new("invalid_date", e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, CoreError>;
