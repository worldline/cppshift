use proc_macro2::TokenStream;
use quote::ToTokens;

use crate::ast::expr::{Expr, ExprBool, ExprNullptr, ExprParen, ExprUnary, UnaryOp};
use crate::transpile::{Transpile, Transpiler};

use super::error::TranspileError;

/// Extract a source span from an expression (best-effort).
pub(crate) fn expr_span<'de>(expr: &Expr<'de>) -> Option<crate::SourceSpan<'de>> {
    match expr {
        Expr::Lit(l) => Some(l.span),
        Expr::Ident(i) => Some(i.ident.span),
        Expr::Path(p) => p.path.segments.first().map(|s| s.ident.span),
        _ => None,
    }
}

/// Build a [`TranspileError::UnsupportedExpr`] from an expression.
fn unsupported_from_expr(message: &str, expr: &Expr<'_>) -> TranspileError {
    match expr_span(expr) {
        Some(span) => TranspileError::UnsupportedExpr {
            message: message.to_owned(),
            src: span.full_source().to_owned(),
            err_span: span.into(),
        },
        None => TranspileError::UnsupportedExpr {
            message: message.to_owned(),
            src: String::new(),
            err_span: miette::SourceSpan::new(0.into(), 0),
        },
    }
}

impl<'de> Transpile for Expr<'de> {
    #[allow(clippy::only_used_in_recursion)]
    fn transpile(
        &self,
        transpiler: &Transpiler,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        match self {
            Expr::Lit(lit) => {
                let rust_expr: syn::Expr =
                    syn::parse_str(lit.span.src()).map_err(|_| TranspileError::UnsupportedExpr {
                        message: format!("cannot parse literal `{}`", lit.span.src()),
                        src: lit.span.full_source().to_owned(),
                        err_span: lit.span.into(),
                    })?;
                tokens.extend(quote::quote!(#rust_expr));
            }
            Expr::Bool(ExprBool { value, .. }) => {
                tokens.extend(quote::quote!(#value));
            }
            Expr::Nullptr(ExprNullptr { .. }) => {
                tokens.extend(quote::quote!(std::ptr::null()));
            }
            Expr::Ident(i) => {
                i.ident.to_tokens(tokens);
            }
            Expr::Path(p) => {
                let rust_expr = syn::Expr::try_from(p.path.clone()).map_err(|_| {
                    unsupported_from_expr("cannot transpile path expression", self)
                })?;
                tokens.extend(quote::quote!(#rust_expr));
            }
            Expr::Unary(ExprUnary {
                op: UnaryOp::Negate,
                operand,
            }) => {
                let mut inner_tokens = TokenStream::new();
                operand.transpile(transpiler, &mut inner_tokens)?;
                tokens.extend(quote::quote!(- #inner_tokens));
            }
            Expr::Paren(ExprParen { expr }) => {
                let mut inner_tokens = TokenStream::new();
                expr.transpile(transpiler, &mut inner_tokens)?;
                tokens.extend(quote::quote!((#inner_tokens)));
            }
            other => {
                return Err(unsupported_from_expr(
                    "expression cannot be transpiled to Rust",
                    other,
                ));
            }
        }

        Ok(())
    }
}
