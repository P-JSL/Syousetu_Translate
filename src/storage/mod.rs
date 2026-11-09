//! Shared persistence primitives. Uncommitted output is removed on drop.
mod atomic;
mod integrity;
mod text;
pub use atomic::{atomic_write, AtomicFile};
pub use integrity::sha256_file;
pub use text::decode_text;
