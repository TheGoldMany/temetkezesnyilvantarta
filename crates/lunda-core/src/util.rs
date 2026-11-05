use chrono::{NaiveDate, SecondsFormat, Utc};

use crate::error::{CoreError, Result};

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub fn now_ts() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| CoreError::new("invalid_date", format!("invalid date: {s}")).with("value", s))
}

pub fn fmt_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

pub fn opt_date(s: &Option<String>) -> Result<Option<NaiveDate>> {
    match s {
        Some(v) if !v.trim().is_empty() => Ok(Some(parse_date(v)?)),
        _ => Ok(None),
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(data))
}
