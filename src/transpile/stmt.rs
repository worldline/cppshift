use proc_macro2::TokenStream;

use crate::{
    SourceCodeSpan,
    ast::{Stmt, stmt::StmtLocal},
    transpile::{Transpile, TranspileContext, TranspileError, Transpiler},
};

/// Build a [`TranspileError::UnsupportedStmt`] from a statement, with the correct span information if available.
pub fn unsupported_from_stmt(message: &str, stmt: &Stmt<'_>) -> TranspileError {
    match stmt.span() {
        Some(span) => TranspileError::UnsupportedStmt {
            message: message.to_owned(),
            src: span.full_source().to_owned(),
            err_span: span.into(),
        },
        None => TranspileError::UnsupportedStmt {
            message: format!("{}: {:?}", message, stmt),
            src: String::new(),
            err_span: miette::SourceSpan::new(0.into(), 0),
        },
    }
}

impl<'de> Transpile for StmtLocal<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        ctx: &mut TranspileContext,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        let ident = self.ident;
        ctx.declare_local(ident.sym.to_owned());
        tokens.extend(quote::quote! { let mut #ident: });
        self.ty.transpile(transpiler, ctx, tokens)?;

        if let Some(init) = &self.init {
            tokens.extend(quote::quote! { = });
            init.transpile(transpiler, ctx, tokens)?;
            tokens.extend(quote::quote! { ; });
        } else {
            tokens.extend(quote::quote! { = Default::default(); });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::panic;

    use super::*;
    use crate::ast::{self, parse_file};

    #[test]
    fn stmt_local_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "void function() {
            int x = 5;
        }";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            ast::Item::Fn(ast::ItemFn {
                block: Some(block), ..
            }) => {
                if let ast::Stmt::Local(stmt) = &block.stmts[0] {
                    assert_eq!(
                        stmt.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                            .to_string(),
                        "let mut x : i32 = 5 ;"
                    );
                }
            }
            item => panic!("expected ItemStmt, got {item:?}"),
        }

        Ok(())
    }
}
