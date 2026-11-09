//! Optional recovery and proprietary-engine compatibility boundary.
pub mod dictionary;
pub mod ehnd;
pub mod ehnd_control;
pub mod ehnd_regex;
#[allow(unsafe_code)]
pub mod legacy_abi;
pub mod legacy_dat;
pub mod morph;
pub mod native;
pub mod root_dll;
