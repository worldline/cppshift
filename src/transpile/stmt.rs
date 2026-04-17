use proc_macro2::TokenStream;

use crate::{
    ast::stmt::StmtLocal,
    transpile::{Transpile, TranspileError, Transpiler},
};

impl<'de> Transpile for StmtLocal<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        let ident = self.ident;
        tokens.extend(quote::quote! { let mut #ident: });
        self.ty.transpile(transpiler, tokens)?;

        if let Some(init) = &self.init {
            tokens.extend(quote::quote! { = });
            init.transpile(transpiler, tokens)?;
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
                        stmt.transpile_token_stream(&transpiler)?.to_string(),
                        "let mut x : i32 = 5 ;"
                    );
                }
            }
            item => panic!("expected ItemStmt, got {item:?}"),
        }

        Ok(())
    }
}
