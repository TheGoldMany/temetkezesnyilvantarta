//! A small rusqlite-shaped layer over a synchronous Postgres client.
//!
//! The service layer was written against rusqlite (`query_row`, `prepare`,
//! `params!`, `?1` placeholders, loosely typed values). Keeping that surface
//! lets the business logic move to Postgres almost unchanged:
//!
//! * `?`/`?N` placeholders are rewritten to `$N` (outside string literals);
//! * parameters are [`Value`]s that adapt to the type Postgres infers
//!   (an integer bound to a BOOL column becomes `true`, to TEXT `"1"`, ...);
//! * result columns are read back with SQLite-like affinity (BOOL and
//!   NUMERIC come back as integers when possible);
//! * transactions nest with savepoints ([`tx`]);
//! * every connection works inside one tenant schema (`search_path`).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt;

use postgres::types::{FromSql, IsNull, ToSql, Type};
use postgres::{Client, Statement as PgStatement};

// ---------------------------------------------------------------- errors

#[derive(Debug)]
pub enum Error {
    /// `query_row` found no row (rusqlite's QueryReturnedNoRows).
    NoRows,
    Pg(postgres::Error),
    Conversion(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoRows => write!(f, "query returned no rows"),
            Error::Pg(e) => match e.as_db_error() {
                Some(db) => write!(f, "{} ({})", db.message(), db.code().code()),
                None => write!(f, "{e}"),
            },
            Error::Conversion(m) => write!(f, "conversion: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<postgres::Error> for Error {
    fn from(e: postgres::Error) -> Self {
        Error::Pg(e)
    }
}

impl Error {
    /// SQLSTATE of a database error (e.g. "23505" unique violation).
    pub fn sqlstate(&self) -> Option<&str> {
        match self {
            Error::Pg(e) => e.code().map(|c| c.code()),
            _ => None,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// rusqlite's `OptionalExtension`: turns "no rows" into `None`.
pub trait OptionalExtension<T> {
    fn optional(self) -> Result<Option<T>>;
}

impl<T> OptionalExtension<T> for Result<T> {
    fn optional(self) -> Result<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(Error::NoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ---------------------------------------------------------------- values

/// A loosely typed SQL value (same variants as `rusqlite::Value`).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

type BoxErr = Box<dyn std::error::Error + Sync + Send>;

impl ToSql for Value {
    fn to_sql(&self, ty: &Type, out: &mut bytes::BytesMut) -> std::result::Result<IsNull, BoxErr> {
        let as_text = |s: &str, out: &mut bytes::BytesMut| -> std::result::Result<IsNull, BoxErr> {
            out.extend_from_slice(s.as_bytes());
            Ok(IsNull::No)
        };
        match self {
            Value::Null => Ok(IsNull::Yes),
            Value::Integer(i) => match *ty {
                Type::BOOL => (*i != 0).to_sql(ty, out),
                Type::INT2 => i16::try_from(*i)?.to_sql(ty, out),
                Type::INT4 => i32::try_from(*i)?.to_sql(ty, out),
                Type::INT8 => i.to_sql(ty, out),
                Type::FLOAT4 => (*i as f32).to_sql(ty, out),
                Type::FLOAT8 => (*i as f64).to_sql(ty, out),
                Type::NUMERIC => encode_numeric_i64(*i, out),
                _ => as_text(&i.to_string(), out),
            },
            Value::Real(r) => match *ty {
                Type::FLOAT4 => (*r as f32).to_sql(ty, out),
                Type::FLOAT8 => r.to_sql(ty, out),
                Type::INT2 | Type::INT4 | Type::INT8 => Value::Integer(r.round() as i64).to_sql(ty, out),
                Type::BOOL => (*r != 0.0).to_sql(ty, out),
                Type::NUMERIC if r.fract() == 0.0 => encode_numeric_i64(*r as i64, out),
                _ => as_text(&r.to_string(), out),
            },
            Value::Text(s) => match *ty {
                Type::DATE => {
                    let d = chrono::NaiveDate::parse_from_str(&s[..10.min(s.len())], "%Y-%m-%d").map_err(|_| format!("not a date: {s:?}"))?;
                    let days = (d - chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap()).num_days() as i32;
                    days.to_sql(&Type::INT4, out)
                }
                Type::TIMESTAMPTZ | Type::TIMESTAMP => {
                    let t = chrono::DateTime::parse_from_rfc3339(s).map_err(|_| format!("not a timestamp: {s:?}"))?;
                    let base = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc();
                    let micros = (t.with_timezone(&chrono::Utc) - base).num_microseconds().ok_or("timestamp out of range")?;
                    micros.to_sql(&Type::INT8, out)
                }
                Type::INT2 | Type::INT4 | Type::INT8 => {
                    let n: i64 = s.trim().parse().map_err(|_| format!("not an integer: {s:?}"))?;
                    Value::Integer(n).to_sql(ty, out)
                }
                Type::FLOAT4 | Type::FLOAT8 => {
                    let n: f64 = s.trim().parse().map_err(|_| format!("not a number: {s:?}"))?;
                    Value::Real(n).to_sql(ty, out)
                }
                Type::BOOL => matches!(s.as_str(), "1" | "true" | "t").to_sql(ty, out),
                Type::BYTEA => s.as_bytes().to_sql(ty, out),
                _ => as_text(s, out),
            },
            Value::Blob(b) => match *ty {
                Type::BYTEA => b.as_slice().to_sql(ty, out),
                _ => as_text(&String::from_utf8_lossy(b), out),
            },
        }
    }

    fn accepts(_: &Type) -> bool {
        true
    }

    postgres::types::to_sql_checked!();
}

/// Binary NUMERIC for an integer (base-10000 digits).
fn encode_numeric_i64(v: i64, out: &mut bytes::BytesMut) -> std::result::Result<IsNull, BoxErr> {
    use bytes::BufMut;
    let neg = v < 0;
    let mut n = v.unsigned_abs();
    let mut digits = Vec::new();
    while n > 0 {
        digits.push((n % 10000) as i16);
        n /= 10000;
    }
    digits.reverse();
    out.put_i16(digits.len() as i16);
    out.put_i16(digits.len().saturating_sub(1) as i16);
    out.put_u16(if neg { 0x4000 } else { 0 });
    out.put_u16(0);
    for d in digits {
        out.put_i16(d);
    }
    Ok(IsNull::No)
}

/// Reads any supported column type into a [`Value`].
struct AnyValue(Value);

impl<'a> FromSql<'a> for AnyValue {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> std::result::Result<Self, BoxErr> {
        Ok(AnyValue(match *ty {
            Type::BOOL => Value::Integer(bool::from_sql(ty, raw)? as i64),
            Type::INT2 => Value::Integer(i16::from_sql(ty, raw)? as i64),
            Type::INT4 => Value::Integer(i32::from_sql(ty, raw)? as i64),
            Type::INT8 => Value::Integer(i64::from_sql(ty, raw)?),
            Type::OID => Value::Integer(u32::from_sql(ty, raw)? as i64),
            Type::FLOAT4 => Value::Real(f32::from_sql(ty, raw)? as f64),
            Type::FLOAT8 => Value::Real(f64::from_sql(ty, raw)?),
            Type::NUMERIC => decode_numeric(raw)?,
            Type::BYTEA => Value::Blob(raw.to_vec()),
            Type::DATE => {
                let days = i32::from_sql(&Type::INT4, raw)?;
                let d = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap() + chrono::Duration::days(days as i64);
                Value::Text(d.format("%Y-%m-%d").to_string())
            }
            Type::TIMESTAMPTZ | Type::TIMESTAMP => {
                let micros = i64::from_sql(&Type::INT8, raw)?;
                let base = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc();
                Value::Text((base + chrono::Duration::microseconds(micros)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            }
            _ => Value::Text(String::from_utf8(raw.to_vec())?),
        }))
    }

    fn from_sql_null(_: &Type) -> std::result::Result<Self, BoxErr> {
        Ok(AnyValue(Value::Null))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(
            *ty,
            Type::BOOL
                | Type::INT2
                | Type::INT4
                | Type::INT8
                | Type::OID
                | Type::FLOAT4
                | Type::FLOAT8
                | Type::NUMERIC
                | Type::BYTEA
                | Type::DATE
                | Type::TIMESTAMP
                | Type::TIMESTAMPTZ
                | Type::TEXT
                | Type::VARCHAR
                | Type::BPCHAR
                | Type::NAME
                | Type::UNKNOWN
        )
    }
}

fn decode_numeric(raw: &[u8]) -> std::result::Result<Value, BoxErr> {
    let rd =
        |i: usize| -> std::result::Result<i16, BoxErr> { raw.get(i..i + 2).map(|b| i16::from_be_bytes([b[0], b[1]])).ok_or_else(|| "short numeric".into()) };
    let ndigits = rd(0)? as usize;
    let weight = rd(2)? as i32;
    let sign = rd(4)? as u16;
    let dscale = rd(6)?;
    if sign == 0xC000 {
        return Ok(Value::Real(f64::NAN));
    }
    let mut v = 0f64;
    for i in 0..ndigits {
        let d = rd(8 + i * 2)? as f64;
        v += d * 10000f64.powi(weight - i as i32);
    }
    if sign == 0x4000 {
        v = -v;
    }
    if dscale == 0 && v.abs() < 9.0e15 {
        Ok(Value::Integer(v.round() as i64))
    } else {
        Ok(Value::Real(v))
    }
}

/// Conversion from a [`Value`] (SQLite-like affinity).
pub trait FromValue: Sized {
    fn from_value(v: Value) -> Result<Self>;
}

fn conv<T>(what: &str, v: &Value) -> Result<T> {
    Err(Error::Conversion(format!("cannot read {v:?} as {what}")))
}

impl FromValue for Value {
    fn from_value(v: Value) -> Result<Self> {
        Ok(v)
    }
}

impl FromValue for i64 {
    fn from_value(v: Value) -> Result<Self> {
        match v {
            Value::Integer(i) => Ok(i),
            Value::Real(r) if r.fract() == 0.0 => Ok(r as i64),
            Value::Text(ref s) => s.trim().parse().or_else(|_| conv("i64", &v)),
            _ => conv("i64", &v),
        }
    }
}

macro_rules! int_from_value {
    ($($t:ty),*) => {$(
        impl FromValue for $t {
            fn from_value(v: Value) -> Result<Self> {
                let i = i64::from_value(v)?;
                <$t>::try_from(i).map_err(|_| Error::Conversion(format!("{i} out of range for {}", stringify!($t))))
            }
        }
    )*};
}
int_from_value!(i32, u32, i16, u16, u8, usize, u64);

impl FromValue for f64 {
    fn from_value(v: Value) -> Result<Self> {
        match v {
            Value::Integer(i) => Ok(i as f64),
            Value::Real(r) => Ok(r),
            Value::Text(ref s) => s.trim().parse().or_else(|_| conv("f64", &v)),
            _ => conv("f64", &v),
        }
    }
}

impl FromValue for f32 {
    fn from_value(v: Value) -> Result<Self> {
        f64::from_value(v).map(|f| f as f32)
    }
}

impl FromValue for bool {
    fn from_value(v: Value) -> Result<Self> {
        match v {
            Value::Integer(i) => Ok(i != 0),
            Value::Real(r) => Ok(r != 0.0),
            Value::Text(ref s) => Ok(matches!(s.as_str(), "1" | "true" | "t")),
            _ => conv("bool", &v),
        }
    }
}

impl FromValue for String {
    fn from_value(v: Value) -> Result<Self> {
        match v {
            Value::Text(s) => Ok(s),
            Value::Integer(i) => Ok(i.to_string()),
            Value::Real(r) => Ok(r.to_string()),
            Value::Blob(b) => String::from_utf8(b).map_err(|e| Error::Conversion(e.to_string())),
            Value::Null => conv("String", &v),
        }
    }
}

impl FromValue for Vec<u8> {
    fn from_value(v: Value) -> Result<Self> {
        match v {
            Value::Blob(b) => Ok(b),
            Value::Text(s) => Ok(s.into_bytes()),
            _ => conv("bytes", &v),
        }
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(v: Value) -> Result<Self> {
        match v {
            Value::Null => Ok(None),
            v => T::from_value(v).map(Some),
        }
    }
}

/// Conversion into a [`Value`] for binding.
pub trait ToValue {
    fn to_value(&self) -> Value;
}

impl ToValue for Value {
    fn to_value(&self) -> Value {
        self.clone()
    }
}

macro_rules! int_to_value {
    ($($t:ty),*) => {$(
        impl ToValue for $t {
            fn to_value(&self) -> Value { Value::Integer(*self as i64) }
        }
    )*};
}
int_to_value!(i64, i32, i16, u32, u16, u8, usize, u64);

impl ToValue for bool {
    fn to_value(&self) -> Value {
        Value::Integer(*self as i64)
    }
}
impl ToValue for f64 {
    fn to_value(&self) -> Value {
        Value::Real(*self)
    }
}
impl ToValue for f32 {
    fn to_value(&self) -> Value {
        Value::Real(*self as f64)
    }
}
impl ToValue for str {
    fn to_value(&self) -> Value {
        Value::Text(self.to_string())
    }
}
impl ToValue for String {
    fn to_value(&self) -> Value {
        Value::Text(self.clone())
    }
}
impl ToValue for [u8] {
    fn to_value(&self) -> Value {
        Value::Blob(self.to_vec())
    }
}
impl ToValue for Vec<u8> {
    fn to_value(&self) -> Value {
        Value::Blob(self.clone())
    }
}
impl<T: ToValue + ?Sized> ToValue for &T {
    fn to_value(&self) -> Value {
        (**self).to_value()
    }
}
impl<T: ToValue> ToValue for Option<T> {
    fn to_value(&self) -> Value {
        match self {
            Some(v) => v.to_value(),
            None => Value::Null,
        }
    }
}

/// Anything that can be bound as the parameter list of a statement.
pub trait Params {
    fn into_values(self) -> Vec<Value>;
}

impl Params for &[&dyn ToValue] {
    fn into_values(self) -> Vec<Value> {
        self.iter().map(|v| v.to_value()).collect()
    }
}

impl<const N: usize> Params for &[&dyn ToValue; N] {
    fn into_values(self) -> Vec<Value> {
        self.iter().map(|v| v.to_value()).collect()
    }
}

impl Params for [&dyn ToValue; 0] {
    fn into_values(self) -> Vec<Value> {
        Vec::new()
    }
}

macro_rules! array_params {
    ($($n:literal)*) => {$(
        impl<T: ToValue> Params for [T; $n] {
            fn into_values(self) -> Vec<Value> {
                self.iter().map(|v| v.to_value()).collect()
            }
        }
    )*};
}
array_params!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16);

pub struct ParamsFromIter(Vec<Value>);

impl Params for ParamsFromIter {
    fn into_values(self) -> Vec<Value> {
        self.0
    }
}

pub fn params_from_iter<I>(iter: I) -> ParamsFromIter
where
    I: IntoIterator,
    I::Item: ToValue,
{
    ParamsFromIter(iter.into_iter().map(|v| v.to_value()).collect())
}

#[macro_export]
macro_rules! params {
    () => { &[] as &[&dyn $crate::sql::ToValue] };
    ($($p:expr),+ $(,)?) => { &[$(&$p as &dyn $crate::sql::ToValue),+] as &[&dyn $crate::sql::ToValue] };
}
pub use crate::params;

// ---------------------------------------------------------------- rows

pub struct Row {
    inner: postgres::Row,
}

/// Column index: position or name.
pub trait RowIndex {
    fn index(&self, row: &postgres::Row) -> Result<usize>;
}

impl RowIndex for usize {
    fn index(&self, row: &postgres::Row) -> Result<usize> {
        if *self < row.len() {
            Ok(*self)
        } else {
            Err(Error::Conversion(format!("column index {self} out of range")))
        }
    }
}

impl RowIndex for &str {
    fn index(&self, row: &postgres::Row) -> Result<usize> {
        row.columns().iter().position(|c| c.name() == *self).ok_or_else(|| Error::Conversion(format!("no column {self:?}")))
    }
}

impl Row {
    pub fn get<I: RowIndex, T: FromValue>(&self, idx: I) -> Result<T> {
        let i = idx.index(&self.inner)?;
        let v: AnyValue = self.inner.try_get(i).map_err(|e| Error::Conversion(format!("column {i}: {e}")))?;
        T::from_value(v.0).map_err(|e| match e {
            Error::Conversion(m) => Error::Conversion(format!("column {:?}: {m}", self.inner.columns()[i].name())),
            e => e,
        })
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Buffered result rows (rusqlite's `Rows`).
pub struct Rows {
    rows: std::vec::IntoIter<postgres::Row>,
    current: Option<Row>,
}

impl Rows {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<&Row>> {
        self.current = self.rows.next().map(|inner| Row { inner });
        Ok(self.current.as_ref())
    }
}

// ---------------------------------------------------------------- connection

/// Rewrites `?`/`?N` placeholders to `$N`, leaving string literals,
/// quoted identifiers and comments alone.
pub fn translate(sql: &str) -> String {
    let b = sql.as_bytes();
    let mut out = String::with_capacity(sql.len() + 8);
    let mut i = 0;
    let mut max = 0usize;
    while i < b.len() {
        let c = b[i];
        match c {
            b'\'' | b'"' => {
                let q = c;
                let start = i;
                i += 1;
                while i < b.len() {
                    if b[i] == q {
                        if i + 1 < b.len() && b[i + 1] == q {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i = (i + 1).min(b.len());
                out.push_str(&sql[start..i]);
            }
            b'-' if i + 1 < b.len() && b[i + 1] == b'-' => {
                let start = i;
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                out.push_str(&sql[start..i]);
            }
            b'?' => {
                i += 1;
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                let n = if i > start { sql[start..i].parse::<usize>().unwrap_or(0) } else { max + 1 };
                max = max.max(n);
                out.push('$');
                out.push_str(&n.to_string());
            }
            _ => {
                // copy one UTF-8 character
                let ch = sql[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
    }
    out
}

pub struct Connection {
    client: RefCell<Client>,
    depth: Cell<u32>,
    cache: RefCell<HashMap<String, PgStatement>>,
    schema: RefCell<Option<String>>,
    /// Drop this schema when the connection is dropped (tests).
    temp_schema: Cell<bool>,
    /// Tenant schemas already brought to the latest version on this connection.
    current: RefCell<std::collections::HashSet<String>>,
}

static SP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Connection {
    pub fn new(client: Client) -> Self {
        Self {
            client: RefCell::new(client),
            depth: Cell::new(0),
            cache: RefCell::new(HashMap::new()),
            schema: RefCell::new(None),
            temp_schema: Cell::new(false),
            current: RefCell::new(std::collections::HashSet::new()),
        }
    }

    /// Connects with TLS when the URL asks for it (`sslmode=require`, Neon).
    pub fn connect(url: &str) -> Result<Self> {
        let client = if url.contains("sslmode=require") || url.contains("sslmode=verify") {
            let mut roots = rustls::RootCertStore::empty();
            roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
            let config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()
                .map_err(|e| Error::Conversion(e.to_string()))?
                .with_root_certificates(roots)
                .with_no_client_auth();
            Client::connect(url, tokio_postgres_rustls::MakeRustlsConnect::new(config))?
        } else {
            Client::connect(url, postgres::NoTls)?
        };
        Ok(Self::new(client))
    }

    /// Switches to a tenant schema (created by the caller).
    pub fn use_schema(&self, schema: &str) -> Result<()> {
        if !schema.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(Error::Conversion(format!("invalid schema name {schema:?}")));
        }
        if self.schema.borrow().as_deref() == Some(schema) {
            return Ok(());
        }
        // Prepared statements stay valid: Postgres re-plans them when the
        // search_path changes, and every tenant schema has the same tables.
        self.client.borrow_mut().batch_execute(&format!("SET search_path TO \"{schema}\", public"))?;
        *self.schema.borrow_mut() = Some(schema.to_string());
        // a tenant created by an older version gets the newer migrations the
        // first time it is opened (once per connection)
        if schema.starts_with("t_") && !self.current.borrow().contains(schema) {
            crate::db::migrate(self).map_err(|e| Error::Conversion(format!("migrating {schema}: {e}")))?;
            self.current.borrow_mut().insert(schema.to_string());
        }
        Ok(())
    }

    pub fn schema(&self) -> Option<String> {
        self.schema.borrow().clone()
    }

    pub fn set_temp_schema(&self, v: bool) {
        self.temp_schema.set(v);
    }

    fn statement(&self, sql: &str, vals: &[Value]) -> Result<PgStatement> {
        if let Some(s) = self.cache.borrow().get(sql) {
            return Ok(s.clone());
        }
        let text = translate(sql);
        // Postgres cannot infer a parameter that only appears in `?N IS NULL`;
        // declare such parameters from the bound value and retry.
        let mut types: Vec<Type> = Vec::new();
        // candidate types for the parameter currently being declared
        let mut pending: Vec<Type> = Vec::new();
        let st = loop {
            let res = self.client.borrow_mut().prepare_typed(&text, &types);
            match res {
                Ok(st) => break st,
                Err(e) if matches!(e.code().map(|c| c.code()), Some("42P08" | "42P18")) => {
                    let n = e
                        .as_db_error()
                        .and_then(|d| d.message().rsplit('$').next().and_then(|x| x.trim().parse::<usize>().ok()))
                        .filter(|n| *n >= 1 && *n > types.len());
                    let Some(n) = n else { return Err(Error::Pg(e)) };
                    while types.len() < n - 1 {
                        types.push(Type::UNKNOWN);
                    }
                    pending = match vals.get(n - 1) {
                        Some(Value::Integer(_)) => vec![Type::INT8, Type::TEXT],
                        Some(Value::Real(_)) => vec![Type::FLOAT8],
                        Some(Value::Blob(_)) => vec![Type::BYTEA],
                        Some(Value::Text(_)) => vec![Type::TEXT],
                        _ => vec![Type::TEXT, Type::INT8, Type::FLOAT8, Type::BOOL],
                    };
                    types.push(pending.remove(0));
                }
                // the declared type did not fit the rest of the query: try the next candidate
                Err(e) if !pending.is_empty() && matches!(e.code().map(|c| c.code()), Some("42883" | "42804" | "42725")) => {
                    types.pop();
                    types.push(pending.remove(0));
                }
                Err(e) => {
                    if cfg!(debug_assertions) {
                        let msg = e.as_db_error().map(|d| d.message().to_string()).unwrap_or_else(|| e.to_string());
                        eprintln!("SQL prepare failed: {msg}\n{sql}");
                    }
                    return Err(Error::Pg(e));
                }
            }
        };
        let mut cache = self.cache.borrow_mut();
        if cache.len() > 500 {
            cache.clear();
        }
        cache.insert(sql.to_string(), st.clone());
        Ok(st)
    }

    fn run(&self, sql: &str, params: Vec<Value>) -> Result<Vec<postgres::Row>> {
        let st = self.statement(sql, &params)?;
        let refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|v| v as &(dyn ToSql + Sync)).collect();
        Ok(self.client.borrow_mut().query(&st, &refs)?)
    }

    pub fn execute<P: Params>(&self, sql: &str, params: P) -> Result<usize> {
        let vals = params.into_values();
        let st = self.statement(sql, &vals)?;
        let refs: Vec<&(dyn ToSql + Sync)> = vals.iter().map(|v| v as &(dyn ToSql + Sync)).collect();
        Ok(self.client.borrow_mut().execute(&st, &refs)? as usize)
    }

    /// Runs several statements without parameters.
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        Ok(self.client.borrow_mut().batch_execute(sql)?)
    }

    pub fn query_row<T, P: Params, F: FnOnce(&Row) -> Result<T>>(&self, sql: &str, params: P, f: F) -> Result<T> {
        let mut rows = self.run(sql, params.into_values())?;
        if rows.is_empty() {
            return Err(Error::NoRows);
        }
        f(&Row { inner: rows.swap_remove(0) })
    }

    pub fn prepare_cached(&self, sql: &str) -> Result<Statement<'_>> {
        self.prepare(sql)
    }

    pub fn prepare(&self, sql: &str) -> Result<Statement<'_>> {
        Ok(Statement { conn: self, sql: sql.to_string() })
    }

    pub fn in_transaction(&self) -> bool {
        self.depth.get() > 0
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if self.temp_schema.get() {
            if let Some(s) = self.schema.borrow().clone() {
                let _ = self.client.borrow_mut().batch_execute(&format!("DROP SCHEMA IF EXISTS \"{s}\" CASCADE"));
            }
        }
    }
}

pub struct Statement<'c> {
    conn: &'c Connection,
    sql: String,
}

impl Statement<'_> {
    pub fn query_map<T, P: Params, F: FnMut(&Row) -> Result<T>>(&mut self, params: P, mut f: F) -> Result<std::vec::IntoIter<Result<T>>> {
        let rows = self.conn.run(&self.sql, params.into_values())?;
        Ok(rows.into_iter().map(|inner| f(&Row { inner })).collect::<Vec<_>>().into_iter())
    }

    pub fn query<P: Params>(&mut self, params: P) -> Result<Rows> {
        let rows = self.conn.run(&self.sql, params.into_values())?;
        Ok(Rows { rows: rows.into_iter(), current: None })
    }

    pub fn query_row<T, P: Params, F: FnOnce(&Row) -> Result<T>>(&mut self, params: P, f: F) -> Result<T> {
        self.conn.query_row(&self.sql, params, f)
    }

    pub fn execute<P: Params>(&mut self, params: P) -> Result<usize> {
        self.conn.execute(&self.sql, params)
    }

    pub fn exists<P: Params>(&mut self, params: P) -> Result<bool> {
        Ok(!self.conn.run(&self.sql, params.into_values())?.is_empty())
    }

    pub fn column_names(&self) -> Result<Vec<String>> {
        let st = self.conn.statement(&self.sql, &[])?;
        Ok(st.columns().iter().map(|c| c.name().to_string()).collect())
    }

    pub fn column_count(&self) -> Result<usize> {
        Ok(self.conn.statement(&self.sql, &[])?.columns().len())
    }
}

// ---------------------------------------------------------------- transactions

/// Nestable transaction: BEGIN at the outermost level, SAVEPOINT inside.
/// Rolls back on drop unless committed.
pub struct Tx<'a> {
    conn: &'a Connection,
    name: Option<String>,
    done: bool,
}

pub fn tx(conn: &Connection) -> Result<Tx<'_>> {
    let depth = conn.depth.get();
    let name = if depth == 0 {
        conn.execute_batch("BEGIN")?;
        None
    } else {
        let n = format!("sp{}", SP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        conn.execute_batch(&format!("SAVEPOINT {n}"))?;
        Some(n)
    };
    conn.depth.set(depth + 1);
    Ok(Tx { conn, name, done: false })
}

impl Tx<'_> {
    pub fn commit(mut self) -> Result<()> {
        self.done = true;
        self.conn.depth.set(self.conn.depth.get() - 1);
        match &self.name {
            None => self.conn.execute_batch("COMMIT"),
            Some(n) => self.conn.execute_batch(&format!("RELEASE {n}")),
        }
    }
}

impl std::ops::Deref for Tx<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.conn
    }
}

impl Drop for Tx<'_> {
    fn drop(&mut self) {
        if !self.done {
            self.conn.depth.set(self.conn.depth.get() - 1);
            let _ = match &self.name {
                None => {
                    // a rolled-back transaction also undoes SET search_path
                    *self.conn.schema.borrow_mut() = None;
                    self.conn.execute_batch("ROLLBACK")
                }
                Some(n) => self.conn.execute_batch(&format!("ROLLBACK TO {n}; RELEASE {n}")),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::translate;

    #[test]
    fn placeholders() {
        assert_eq!(translate("SELECT ?1, ?2, ?1"), "SELECT $1, $2, $1");
        assert_eq!(translate("a = ? AND b = ?"), "a = $1 AND b = $2");
        assert_eq!(translate("x = ?3 AND y = ?"), "x = $3 AND y = $4");
        assert_eq!(translate("coalesce(x,'?') || ? -- ?\n, \"a?\""), "coalesce(x,'?') || $1 -- ?\n, \"a?\"");
        assert_eq!(translate("név = 'ő?' AND ű = ?"), "név = 'ő?' AND ű = $1");
    }

    #[test]
    fn dates_round_trip() {
        let c = crate::db::open_in_memory().unwrap();
        let d: String = c.query_row("SELECT CAST(?1 AS DATE) + 1", crate::params!["2026-12-31"], |r| r.get(0)).unwrap();
        assert_eq!(d, "2027-01-01");
        let t: String = c.query_row("SELECT CAST(?1 AS TIMESTAMPTZ) + interval '1 hour'", crate::params!["2026-11-05T10:00:00Z"], |r| r.get(0)).unwrap();
        assert_eq!(t, "2026-11-05T11:00:00Z");
    }
}
