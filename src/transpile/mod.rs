//! Transpiler module to convert C++ ([`crate::ast`]) into Rust ([`syn`])

pub mod error;
pub mod expr;
pub mod item;
pub mod stmt;
pub mod ty;

use std::{collections::HashSet, sync::Arc};

pub use error::TranspileError;
use proc_macro2::TokenStream;
use serde::Deserialize;
pub use ty::*;

use crate::ast::Expr;

/// Type alias for the fallback expression handler.
type FallbackExprHandler = Arc<
    dyn for<'de> Fn(&Expr<'de>, &Transpiler, &mut TokenStream) -> Result<(), TranspileError>
        + Send
        + Sync,
>;

/// Transpiler struct, which is the configuration entrypoint for all transpilation operations.
#[derive(Default, Clone, Deserialize)]
pub struct Transpiler {
    /// List of type we don't want to transpile
    #[serde(default)]
    pub skip_types: HashSet<String>,
    /// Type mapper to map C++ types to Rust types
    #[serde(default)]
    pub ty_mapper: TypeMapper,
    /// Optional fallback handler for otherwise-unsupported expressions
    #[serde(skip)]
    pub fallback_expr_handler: Option<FallbackExprHandler>,
}

impl std::fmt::Debug for Transpiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transpiler")
            .field("skip_types", &self.skip_types)
            .field("ty_mapper", &self.ty_mapper)
            .field(
                "fallback_expr_handler",
                &self.fallback_expr_handler.as_ref().map(|_| "<closure>"),
            )
            .finish()
    }
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
