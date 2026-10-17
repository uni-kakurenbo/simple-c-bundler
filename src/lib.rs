//! Generate C output functions, compile C assets to assembly, and bundle C modules.
//!
//! Use [`Build`] with explicit entry sources. [`Settings`] configures include paths,
//! template directories, generated headers, and assembly recipes.
pub mod build;
pub mod compiler;
pub use build::{Build, BundleOutput, GeneratedOutput, Settings};
pub use c_source_bundle::{Result, read_text, write_text};
pub use c_source_emit::{literal, templates};
