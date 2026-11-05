//! Lunda Temető core library.
//!
//! All business rules and permission checks live here so they can be tested
//! without the desktop shell (see docs/PLAN.md).

#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::large_enum_variant)]

pub mod audit;
pub mod auth;
pub mod db;
pub mod error;
pub mod model;
pub mod rules;
pub mod settings;
pub mod store;
pub mod text;
pub mod util;

pub use error::{CoreError, Result};
pub use store::Store;
