//! Discover C modules, expand local includes, and qualify private identifiers.
pub mod bundle;
pub mod c_source;
pub mod modules;
mod text;
pub use text::{read_text, write_text};
pub type Result<T> = std::result::Result<T, String>;
