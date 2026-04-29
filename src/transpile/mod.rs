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
    dyn for<'de> Fn(
            &Expr<'de>,
            &Transpiler,
            &mut TranspileContext,
            &mut TokenStream,
        ) -> Result<(), TranspileError>
        + Send
        + Sync,
>;

/// Transient context for a single transpilation session.
///
/// Tracks class member names and a stack of local variable scopes
/// so that `Expr::Ident` can determine whether to emit `self.x` or `x`.
#[derive(Debug, Clone, Default)]
pub struct TranspileContext {
    /// Names of non-static data members of the current class (if any).
    member_vars: HashSet<String>,
    /// Stack of local scopes. Each scope is the set of identifiers
    /// declared in that scope (locals, parameters, for-range vars).
    scope_stack: Vec<HashSet<String>>,
}

impl TranspileContext {
    /// Create a new empty context (no class, no scopes).
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a context populated with the member variable names
    /// from a class/struct.
    pub fn with_members(member_names: impl IntoIterator<Item = String>) -> Self {
        Self {
            member_vars: member_names.into_iter().collect(),
            scope_stack: Vec::new(),
        }
    }

    /// Add a member variable name to the context.
    pub fn add_member(&mut self, name: String) {
        self.member_vars.insert(name);
    }

    /// Push a new local scope (e.g., entering a block).
    pub fn push_scope(&mut self) {
        self.scope_stack.push(HashSet::new());
    }

    /// Pop the innermost local scope (e.g., exiting a block).
    pub fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }

    /// Declare a local variable in the current (innermost) scope.
    pub fn declare_local(&mut self, name: String) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.insert(name);
        }
    }

    /// Check if an identifier is a known local variable (in any active scope).
    pub fn is_local(&self, name: &str) -> bool {
        self.scope_stack.iter().any(|scope| scope.contains(name))
    }

    /// Check if an identifier is a known class member variable.
    pub fn is_member(&self, name: &str) -> bool {
        self.member_vars.contains(name)
    }

    /// Determine if an identifier needs `self.` prefix.
    /// Returns true if the name is a member AND not shadowed by a local.
    pub fn needs_self_prefix(&self, name: &str) -> bool {
        self.is_member(name) && !self.is_local(name)
    }
}

/// Transpiler struct, which is the configuration entrypoint for all transpilation operations.
#[derive(Default, Clone, Deserialize)]
pub struct Transpiler {
    /// List of type we don't want to transpile
    #[serde(default)]
    pub skip_types: HashSet<String>,
    /// Type mapper to map C++ types to Rust types
    #[serde(default)]
    pub ty_mapper: TypeMapper,
    /// Whether to generate a `Default` variant as the first value of transpiled enums
    #[serde(default)]
    pub enum_default_variant: bool,
    /// Optional fallback handler for otherwise-unsupported expressions
    #[serde(skip)]
    pub fallback_expr_handler: Option<FallbackExprHandler>,
}

impl std::fmt::Debug for Transpiler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Transpiler")
            .field("skip_types", &self.skip_types)
            .field("ty_mapper", &self.ty_mapper)
            .field("enum_default_variant", &self.enum_default_variant)
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
        ctx: &mut TranspileContext,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError>;

    /// Convert `self` with a `Transpiler` configuration into a `TokenStream` object.
    ///
    /// This method is implicitly implemented using `transpile`, and acts as a
    /// convenience method for consumers of the `Transpile` trait.
    fn transpile_token_stream(
        &self,
        transpiler: &Transpiler,
        ctx: &mut TranspileContext,
    ) -> Result<TokenStream, TranspileError> {
        let mut tokens = TokenStream::new();
        self.transpile(transpiler, ctx, &mut tokens)?;
        Ok(tokens)
    }

    /// Convert `self` with a `Transpiler` configuration into a `TokenStream` object.
    ///
    /// This method is implicitly implemented using `transpile`, and acts as a
    /// convenience method for consumers of the `Transpile` trait.
    fn transpile_into_token_stream(
        self,
        transpiler: &Transpiler,
        ctx: &mut TranspileContext,
    ) -> Result<TokenStream, TranspileError>
    where
        Self: Sized,
    {
        self.transpile_token_stream(transpiler, ctx)
    }
}
