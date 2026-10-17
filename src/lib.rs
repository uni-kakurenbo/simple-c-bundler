//! C generation and bundling through an external GCC-compatible compiler.
pub mod build;
pub mod compiler;
pub use build::{Build, BundleOutput, GeneratedOutput, Settings};
pub use c_source_bundle::{Result, read_text, write_text};
pub use c_source_emit::{literal, templates};
