//! AST module for C++20
//!
//! Converts C++ source code into a structured Abstract Syntax Tree,
//! modeled after the [`syn`](https://docs.rs/syn) crate architecture.
//!
//! Entry point: [`parse_file`] returns a [`File`] containing a list of [`Item`]s.

pub mod error;
pub mod expr;
pub mod item;
pub mod punct;
pub mod stmt;
pub mod ty;
pub mod visit;

pub use error::ParseError;
pub use expr::Expr;
pub use item::*;
pub use stmt::{Block, Stmt};
pub use ty::Type;

/// A complete C++ translation unit, analogous to `syn::File`.
///
/// Contains file-level attributes and a list of top-level items (declarations).
#[derive(Debug, Clone, PartialEq)]
pub struct File<'de> {
    /// File-level C++20 attributes `[[...]]`
    pub attrs: Vec<Attribute<'de>>,
    /// Top-level declarations
    pub items: Vec<Item<'de>>,
}

/// Parse a C++ source file into an AST.
///
/// Analogous to `syn::parse_file`.
///
/// # Errors
///
/// Returns a `ParseError` if the source code contains syntax errors.
pub fn parse_file(_content: &str) -> Result<File<'_>, ParseError> {
    todo!("parse_file implementation")
}
