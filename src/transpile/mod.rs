//! Transpiler module to convert C++ ([`crate::ast`]) into Rust ([`syn`])

pub mod error;
pub mod expr;
pub mod item;
pub mod stmt;
pub mod ty;

use std::collections::HashSet;

pub use error::TranspileError;
use proc_macro2::TokenStream;
use serde::Deserialize;
pub use ty::*;

/// Transpiler struct, which is the configuration entrypoint for all transpilation operations.
#[derive(Debug, Default, Clone, Deserialize)]
pub struct Transpiler {
    /// List of type we don't want to transpile
    #[serde(default)]
    pub skip_types: HashSet<String>,
    /// Type mapper to map C++ types to Rust types
    #[serde(default)]
    pub ty_mapper: TypeMapper,
}

pub trait Transpile {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError>;

    /// Convert `self` with a `Transpiler` configuration into a `TokenStream` object.
    ///
    /// This method is implicitly implemented using `transpile`, and acts as a
    /// convenience method for consumers of the `Transpile` trait.
    fn transpile_token_stream(
        &self,
        transpiler: &Transpiler,
    ) -> Result<TokenStream, TranspileError> {
        let mut tokens = TokenStream::new();
        self.transpile(transpiler, &mut tokens)?;
        Ok(tokens)
    }

    /// Convert `self` with a `Transpiler` configuration into a `TokenStream` object.
    ///
    /// This method is implicitly implemented using `transpile`, and acts as a
    /// convenience method for consumers of the `Transpile` trait.
    fn transpile_into_token_stream(
        self,
        transpiler: &Transpiler,
    ) -> Result<TokenStream, TranspileError>
    where
        Self: Sized,
    {
        self.transpile_token_stream(transpiler)
    }
}
