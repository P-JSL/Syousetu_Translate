//! Isolated research host. CTranslate2 itself is a third-party C++ runtime.
//! This crate is not used by the application or included in release packages.
#![deny(unsafe_code)]
mod backend;
pub mod model;
pub use backend::Backend;
