//! Rust-owned Japanese→Korean engine and selected legacy C symbol adapters.
//!
//! Compatibility boundary and semantics:
//! * The original Java JNA client uses C calling convention (Library).
//! * Unicode API is UTF-16; narrow API is Windows-932 → Windows-949.
//! * The vendor's grammar/dictionaries, its unknown configuration APIs, and
//!   every internal binary data format are NOT replicated.
//! * Only pointers allocated here may be passed to J2K_FreeMem.
#[cfg(feature = "legacy")]
pub mod dictionary;
#[cfg(feature = "legacy")]
pub mod morph;
#[cfg(feature = "legacy")]
pub mod native;
#[cfg(feature = "legacy")]
pub mod legacy_dat;
#[cfg(feature = "legacy")]
pub mod root_dll;
#[cfg(feature = "legacy")]
pub mod ehnd;
#[cfg(feature = "legacy")]
pub mod ehnd_regex;
#[cfg(feature = "legacy")]
pub mod ehnd_control;
pub mod config;
pub mod storage;
pub mod jobs;
pub mod models;
pub mod learning;
pub mod http_client;
#[cfg(feature = "offline-model")]
pub mod int8;
#[cfg(feature = "offline-model")]
mod ordered;
#[cfg(feature = "offline-model")]
pub mod offline;

#[cfg(feature = "legacy")]
mod legacy_abi;
#[cfg(feature = "legacy")]
pub use legacy_abi::*;
