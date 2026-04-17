use proc_macro2::TokenStream;
use quote::ToTokens;

use crate::ast::expr::{
    BinaryOp, Expr, ExprBinary, ExprBool, ExprNullptr, ExprParen, ExprUnary, UnaryOp,
};
use crate::transpile::{Transpile, Transpiler};

use super::error::TranspileError;

impl Transpile for BinaryOp {
    fn transpile(
        &self,
        _transpiler: &Transpiler,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        match self {
            // Arithmetic
            BinaryOp::Add => tokens.extend(quote::quote!(+)),
            BinaryOp::Sub => tokens.extend(quote::quote!(-)),
            BinaryOp::Mul => tokens.extend(quote::quote!(*)),
            BinaryOp::Div => tokens.extend(quote::quote!(/)),
            BinaryOp::Mod => tokens.extend(quote::quote!(%)),
            // Comparison
            BinaryOp::Less => tokens.extend(quote::quote!(<)),
            BinaryOp::LessEqual => tokens.extend(quote::quote!(<=)),
            BinaryOp::Greater => tokens.extend(quote::quote!(>)),
            BinaryOp::GreaterEqual => tokens.extend(quote::quote!(>=)),
            BinaryOp::ThreeWay => tokens.extend(quote::quote!(<=>)),
            BinaryOp::Equal => tokens.extend(quote::quote!(==)),
            BinaryOp::NotEqual => tokens.extend(quote::quote!(!=)),
            // Bitwise
            BinaryOp::BitAnd => tokens.extend(quote::quote!(&)),
            BinaryOp::BitXor => tokens.extend(quote::quote!(^)),
            BinaryOp::BitOr => tokens.extend(quote::quote!(|)),
            // Logical
            BinaryOp::LogicalAnd => tokens.extend(quote::quote!(&&)),
            BinaryOp::LogicalOr => tokens.extend(quote::quote!(||)),
            // Assignment
            BinaryOp::Assign => tokens.extend(quote::quote!(=)),
            BinaryOp::AddAssign => tokens.extend(quote::quote!(+=)),
            BinaryOp::SubAssign => tokens.extend(quote::quote!(-=)),
            BinaryOp::MulAssign => tokens.extend(quote::quote!(*=)),
            BinaryOp::DivAssign => tokens.extend(quote::quote!(/=)),
            BinaryOp::ModAssign => tokens.extend(quote::quote!(%=)),
            BinaryOp::ShiftLeftAssign => tokens.extend(quote::quote!(<<=)),
            BinaryOp::ShiftRightAssign => tokens.extend(quote::quote!(>>=)),
            BinaryOp::BitAndAssign => tokens.extend(quote::quote!(&=)),
            BinaryOp::BitXorAssign => tokens.extend(quote::quote!(^=)),
            BinaryOp::BitOrAssign => tokens.extend(quote::quote!(|=)),
            _ => {
                return Err(TranspileError::UnsupportedExpr {
                    message: format!("binary operator `{self:?}` cannot be transpiled to Rust"),
                    src: String::new(),
                    err_span: miette::SourceSpan::new(0.into(), 0),
                });
            }
        }

        Ok(())
    }
}

/// Build a [`TranspileError::UnsupportedExpr`] from an expression.
pub(crate) fn unsupported_from_expr(message: &str, expr: &Expr<'_>) -> TranspileError {
    match expr.span() {
        Some(span) => TranspileError::UnsupportedExpr {
            message: message.to_owned(),
            src: span.full_source().to_owned(),
            err_span: span.into(),
        },
        None => TranspileError::UnsupportedExpr {
            message: format!("{}: {:?}", message, expr),
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
                let rust_expr: syn::Expr = syn::parse_str(lit.span.src()).map_err(|_| {
                    TranspileError::UnsupportedExpr {
                        message: "cannot parse literal".to_owned(),
                        src: lit.span.full_source().to_owned(),
                        err_span: lit.span.into(),
                    }
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
                let rust_expr = syn::Expr::try_from(p.path.clone())
                    .map_err(|_| unsupported_from_expr("cannot transpile path expression", self))?;
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
            Expr::Binary(ExprBinary { lhs, op, rhs }) => {
                lhs.transpile(transpiler, tokens)?;
                op.transpile(transpiler, tokens)?;
                rhs.transpile(transpiler, tokens)?;
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

impl<'de> Transpile for ExprBinary<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        self.lhs.transpile(transpiler, tokens)?;
        self.op.transpile(transpiler, tokens)?;
        self.rhs.transpile(transpiler, tokens)
    }
}
