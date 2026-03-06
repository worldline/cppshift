//! Transpiler module to convert C++ ([`crate::ast`]) into Rust ([`syn`])

pub mod error;
mod ty;

pub use error::TranspileError;
pub use ty::*;
