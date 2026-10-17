//! Compiler-independent C discovery, qualification, and header expansion.
pub mod bundle;
pub mod c_source;
pub mod modules;
mod text;
pub use text::{read_text, write_text};
pub type Result<T> = std::result::Result<T, String>;
