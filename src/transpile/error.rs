//! Transpilation error types with rich diagnostics via miette

use miette::Diagnostic;
use thiserror::Error;

/// Errors that can occur during C++ → Rust type mapping
#[derive(Diagnostic, Debug, Error)]
pub enum TranspileError {
    /// C++ path has no registered mapping
    #[error("No mapping registered for C++ path `{path}`")]
    UnmappedPath {
        path: String,
        #[source_code]
        src: String,
        #[label = "no mapping for this path"]
        err_span: miette::SourceSpan,
    },
    /// C++ type variant cannot be mapped to Rust
    #[error("{message}: `{ty}`")]
    UnsupportedType {
        message: String,
        ty: String,
        #[source_code]
        src: String,
        #[label = "{message}"]
        err_span: miette::SourceSpan,
    },
    /// C++ expression cannot be transpiled to Rust
    #[error("{message}: `{expr}`")]
    UnsupportedExpr {
        message: String,
        expr: String,
        #[source_code]
        src: String,
        #[label = "{message}"]
        err_span: miette::SourceSpan,
    },
    /// Invalid Rust type syntax provided to the builder
    #[error("Invalid Rust type syntax `{rust_type}`: {reason}")]
    InvalidRustType { rust_type: String, reason: String },
}
