//! C module discovery, declaration-aware name qualification, and typed templates.

pub mod bundle;
pub mod c_source;
pub mod modules;
pub mod templates;
mod text;

pub use text::{read_text, write_text};

pub type Result<T> = std::result::Result<T, String>;
