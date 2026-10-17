//! Generate C output functions from typed templates or fixed byte sequences.
pub mod literal;
pub mod templates;
pub type Result<T> = std::result::Result<T, String>;
