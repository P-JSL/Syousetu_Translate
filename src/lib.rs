//! Rust novel parsing, translation, model lifecycle and desktop task services.
#![deny(unsafe_code)]
pub mod cli;
pub mod config;
pub mod http_client;
#[cfg(feature = "offline-model")]
pub mod int8;
pub mod jobs;
pub mod learning;
pub mod models;
pub mod novel;
#[cfg(feature = "offline-model")]
pub mod offline;
pub mod storage;
pub mod translation;

#[cfg(feature = "legacy")]
pub mod legacy;
#[cfg(feature = "legacy")]
pub use legacy::legacy_abi::*;
#[cfg(feature = "legacy")]
pub use legacy::{dictionary, ehnd, ehnd_control, ehnd_regex, legacy_dat, morph, native, root_dll};
