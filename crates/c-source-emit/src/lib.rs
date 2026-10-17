//! Pure conversion of templates and fixed text into C output functions.
pub mod literal;
pub mod templates;
pub type Result<T> = std::result::Result<T, String>;
