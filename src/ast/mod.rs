//! AST module for C++20
//!
//! Converts C++ source code into a structured Abstract Syntax Tree,
//! modeled after the [`syn`](https://docs.rs/syn) crate architecture.
//!
//! Entry point: [`parse_file`] returns a [`File`] containing a list of [`Item`]s.

pub mod error;
pub mod expr;
pub mod item;
mod parse;
pub mod punct;
pub mod stmt;
pub mod ty;
pub mod visit;

pub use error::AstError;
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
pub fn parse_file<'de>(content: &'de str) -> Result<File<'de>, AstError> {
    parse::parse_file(content)
}

#[cfg(test)]
mod tests {
    use crate::ast::expr::{BinaryOp, ExprBinary, ExprIdent, ExprLit, ExprPath, LitKind};
    use crate::ast::stmt::{StmtCase, StmtExpr, StmtReturn, StmtSwitch};
    use crate::ast::ty::{FundamentalKind, TypeArray, TypePtr};

    use super::*;

    /// Test the ast parser with the gtest main file from google
    #[tokio::test]
    async fn gtests_ast() {
        let gtest_src = reqwest::get("https://raw.githubusercontent.com/google/googletest/refs/heads/main/googletest/src/gtest.cc")
            .await.unwrap()
            .text()
            .await.unwrap();
        assert!(!gtest_src.is_empty());

        let parsed_file = parse_file(&gtest_src).unwrap();
        assert!(!parsed_file.items.is_empty());
    }

    /// Test the ast parser with a simple main function that includes a switch statement and a fallthrough attribute
    #[test]
    fn main_ast() {
        let main = r#"
            #include <iostream>
            #include "module/myheader.h"

            #define ArgText(x) \
                x##TEXT

            // main function
            int main(int argc, char* argv[]) {
                std::cout << "Hello, world" << std::endl;
                switch (argc)
                {
                    case 1:
                    case 2:
                        std::cout << "first and second" << std::endl;
                        [[fallthrough]];
                    case 3:
                        std::cout << "fallthrough" << std::endl;
                        break;
                }

                return 0;
            }
        "#;

        let main_file = parse_file(main).unwrap();
        assert!(!main_file.items.is_empty());
        let mut main_item_iter = main_file.items.iter();

        let include_system_iostream = main_item_iter.next();
        if let Some(Item::Include(ItemInclude { span, path })) = include_system_iostream {
            assert_eq!(span.src(), "#include <iostream>");
            if let IncludePath::System(path_span) = path {
                assert_eq!(path_span.src(), "iostream");
            } else {
                panic!("Expected a system include path, got {:#?}", path);
            }
        } else {
            panic!(
                "Wrong item: expected an include directive, got {:#?}",
                include_system_iostream
            );
        }

        let include_local_header = main_item_iter.next();
        if let Some(Item::Include(ItemInclude { span, path })) = include_local_header {
            assert_eq!(span.src(), "#include \"module/myheader.h\"");
            if let IncludePath::Local(path_span) = path {
                assert_eq!(path_span.src(), "module/myheader.h");
            } else {
                panic!("Expected a local include path, got {:#?}", path);
            }
        } else {
            panic!(
                "Wrong item: expected an include directive, got {:#?}",
                include_local_header
            );
        }

        let define_macro = main_item_iter.next();
        if let Some(Item::Macro(ItemMacro { span, tokens })) = define_macro {
            assert_eq!(span.src(), "#define ArgText(x) \\\n                x##TEXT");
            assert!(!tokens.is_empty())
        } else {
            panic!(
                "Wrong item: expected a define directive, got {:#?}",
                define_macro
            );
        }

        let main_function = main_item_iter.next();
        if let Some(Item::Fn(ItemFn {
            attrs,
            vis,
            sig,
            block,
        })) = main_function
        {
            assert!(attrs.is_empty());
            assert_eq!(Visibility::Inherited, *vis);

            // Check signature
            assert!(!sig.constexpr_token);
            assert!(!sig.consteval_token);
            assert!(!sig.inline_token);
            assert!(!sig.virtual_token);
            assert!(!sig.static_token);
            assert!(!sig.explicit_token);
            if let Type::Fundamental(ty) = &sig.return_type {
                assert_eq!(ty.span.src(), "int");
                assert_eq!(ty.kind, FundamentalKind::Int);
            } else {
                panic!(
                    "Expected a fundamental return type, got {:#?}",
                    sig.return_type
                );
            }
            assert_eq!(sig.ident.span.src(), "main");
            assert_eq!(sig.ident.sym, "main");
            let mut sig_inputs_iter = sig.inputs.iter();
            if let Some(FnArg {
                attrs,
                ty: Type::Fundamental(fn_arg_ty),
                ident: Some(fn_arg_ident),
                default_value,
            }) = sig_inputs_iter.next()
            {
                assert!(attrs.is_empty());
                assert_eq!(fn_arg_ty.span.src(), "int");
                assert_eq!(fn_arg_ty.kind, FundamentalKind::Int);
                assert_eq!(fn_arg_ident.span.src(), "argc");
                assert_eq!(fn_arg_ident.sym, "argc");
                assert_eq!(&None, default_value);
            } else {
                panic!(
                    "Expected a typed argument for argc, got {:#?}",
                    sig.inputs.iter().next()
                );
            }
            if let Some(FnArg {
                attrs,
                ty: Type::Array(TypeArray { element, size }),
                ident: Some(fn_arg_ident),
                default_value,
            }) = sig_inputs_iter.next()
            {
                assert!(attrs.is_empty());
                if let Type::Ptr(TypePtr { cv, pointee }) = &**element {
                    assert!(!cv.const_token);
                    assert!(!cv.volatile_token);
                    if let Type::Fundamental(fn_arg_ty) = &**pointee {
                        assert_eq!(fn_arg_ty.span.src(), "char");
                        assert_eq!(fn_arg_ty.kind, FundamentalKind::Char);
                    } else {
                        panic!(
                            "Expected a fundamental pointee type for argv, got {:#?}",
                            pointee
                        );
                    }
                } else {
                    panic!(
                        "Expected a fundamental element type for argv, got {:#?}",
                        element
                    );
                }
                assert_eq!(size, &None);
                assert_eq!(fn_arg_ident.span.src(), "argv");
                assert_eq!(fn_arg_ident.sym, "argv");
                assert_eq!(&None, default_value);
            } else {
                panic!(
                    "Expected a typed argument for argv, got {:#?}",
                    sig.inputs.iter().next()
                );
            }
            assert!(!sig.variadic);
            // Trailing qualifiers
            assert!(!sig.const_token);
            assert!(!sig.noexcept_token);
            assert!(!sig.override_token);
            assert!(!sig.final_token);
            assert!(!sig.pure_virtual);
            assert!(!sig.defaulted);
            assert!(!sig.deleted);

            let block = block.as_ref().expect("main function should have a block");
            assert_eq!(block.stmts.len(), 3);
            let mut stmts = block.stmts.iter();

            // stmt 1: std::cout << "Hello, world" << std::endl;
            let stmt1 = stmts.next().unwrap();
            if let Stmt::Expr(StmtExpr {
                expr: Expr::Binary(ExprBinary { lhs, op, rhs }),
            }) = stmt1
            {
                assert_eq!(*op, BinaryOp::ShiftLeft);
                if let Expr::Path(ExprPath { path }) = rhs.as_ref() {
                    assert_eq!(path.segments[0].ident.sym, "std");
                    assert_eq!(path.segments[1].ident.sym, "endl");
                } else {
                    panic!("Expected std::endl, got {:#?}", rhs);
                }
                if let Expr::Binary(ExprBinary {
                    lhs: inner_lhs,
                    op: inner_op,
                    rhs: inner_rhs,
                }) = lhs.as_ref()
                {
                    assert_eq!(*inner_op, BinaryOp::ShiftLeft);
                    if let Expr::Path(ExprPath { path }) = inner_lhs.as_ref() {
                        assert_eq!(path.segments[0].ident.sym, "std");
                        assert_eq!(path.segments[1].ident.sym, "cout");
                    } else {
                        panic!("Expected std::cout, got {:#?}", inner_lhs);
                    }
                    if let Expr::Lit(ExprLit { span, kind }) = inner_rhs.as_ref() {
                        assert_eq!(*kind, LitKind::String);
                        assert_eq!(span.src(), "\"Hello, world\"");
                    } else {
                        panic!("Expected string literal, got {:#?}", inner_rhs);
                    }
                } else {
                    panic!("Expected binary lhs for cout, got {:#?}", lhs);
                }
            } else {
                panic!("Expected cout expression statement, got {:#?}", stmt1);
            }

            // stmt 2: switch (argc) { ... }
            let stmt2 = stmts.next().unwrap();
            if let Stmt::Switch(StmtSwitch { expr, body }) = stmt2 {
                if let Expr::Ident(ExprIdent { ident }) = expr {
                    assert_eq!(ident.sym, "argc");
                } else {
                    panic!("Expected argc, got {:#?}", expr);
                }
                assert_eq!(body.stmts.len(), 7);
                let mut switch_stmts = body.stmts.iter();

                // case 1:
                if let Stmt::Case(StmtCase {
                    value: Expr::Lit(ExprLit { span, kind }),
                }) = switch_stmts.next().unwrap()
                {
                    assert_eq!(*kind, LitKind::Integer);
                    assert_eq!(span.src(), "1");
                } else {
                    panic!("Expected case 1");
                }

                // case 2:
                if let Stmt::Case(StmtCase {
                    value: Expr::Lit(ExprLit { span, kind }),
                }) = switch_stmts.next().unwrap()
                {
                    assert_eq!(*kind, LitKind::Integer);
                    assert_eq!(span.src(), "2");
                } else {
                    panic!("Expected case 2");
                }

                // std::cout << "first and second" << std::endl;
                if let Stmt::Expr(StmtExpr {
                    expr: Expr::Binary(ExprBinary { lhs, op, rhs }),
                }) = switch_stmts.next().unwrap()
                {
                    assert_eq!(*op, BinaryOp::ShiftLeft);
                    if let Expr::Path(ExprPath { path }) = rhs.as_ref() {
                        assert_eq!(path.segments[1].ident.sym, "endl");
                    } else {
                        panic!("Expected std::endl, got {:#?}", rhs);
                    }
                    if let Expr::Binary(ExprBinary { rhs: inner_rhs, .. }) = lhs.as_ref() {
                        if let Expr::Lit(ExprLit { span, kind }) = inner_rhs.as_ref() {
                            assert_eq!(*kind, LitKind::String);
                            assert_eq!(span.src(), "\"first and second\"");
                        } else {
                            panic!("Expected string literal, got {:#?}", inner_rhs);
                        }
                    } else {
                        panic!("Expected binary lhs for cout, got {:#?}", lhs);
                    }
                } else {
                    panic!("Expected cout << \"first and second\" << endl");
                }

                // [[fallthrough]]; is parsed as an empty statement
                assert_eq!(switch_stmts.next().unwrap(), &Stmt::Empty);

                // case 3:
                if let Stmt::Case(StmtCase {
                    value: Expr::Lit(ExprLit { span, kind }),
                }) = switch_stmts.next().unwrap()
                {
                    assert_eq!(*kind, LitKind::Integer);
                    assert_eq!(span.src(), "3");
                } else {
                    panic!("Expected case 3");
                }

                // std::cout << "fallthrough" << std::endl;
                if let Stmt::Expr(StmtExpr {
                    expr: Expr::Binary(ExprBinary { lhs, .. }),
                }) = switch_stmts.next().unwrap()
                {
                    if let Expr::Binary(ExprBinary { rhs: inner_rhs, .. }) = lhs.as_ref() {
                        if let Expr::Lit(ExprLit { span, kind }) = inner_rhs.as_ref() {
                            assert_eq!(*kind, LitKind::String);
                            assert_eq!(span.src(), "\"fallthrough\"");
                        } else {
                            panic!("Expected string literal, got {:#?}", inner_rhs);
                        }
                    } else {
                        panic!("Expected binary lhs for cout, got {:#?}", lhs);
                    }
                } else {
                    panic!("Expected cout << \"fallthrough\" << endl");
                }

                // break;
                assert!(matches!(switch_stmts.next().unwrap(), Stmt::Break(_)));
            } else {
                panic!("Expected switch statement, got {:#?}", stmt2);
            }

            // stmt 3: return 0;
            let stmt3 = stmts.next().unwrap();
            if let Stmt::Return(StmtReturn {
                expr: Some(Expr::Lit(ExprLit { span, kind })),
            }) = stmt3
            {
                assert_eq!(*kind, LitKind::Integer);
                assert_eq!(span.src(), "0");
            } else {
                panic!("Expected return 0, got {:#?}", stmt3);
            }
        } else {
            panic!(
                "Wrong item: expected a function definition, got {:#?}",
                main_function
            );
        }

        assert_eq!(None, main_item_iter.next());
    }

    /// Test the ast parser with a simple class definition that includes a constructor, a member function, and a member variable
    #[test]
    fn class_header_ast() {
        let class_header_src = r#"
            #include <iostream>

            /**
             * This is a simple class definition for testing the AST parser.
             * It includes a constructor, a member function, and a member variable.
             */
            class MyClass: public MyMotherClass
            {
                MACRO_DEF(param1, 1);
                typedef MyMotherClass BaseClass;

                public:
                static const string STATIC_VALUE;

                private: int member_var;
                public: MyClass(int x) : member_var(x) {}
                public: void member_function();
            };
        "#;

        let class_header_file = parse_file(class_header_src).unwrap();
        assert!(!class_header_file.items.is_empty());
        let mut class_header_item_iter = class_header_file.items.iter();

        let include_system_iostream = class_header_item_iter.next();
        if let Some(Item::Include(ItemInclude { span, path })) = include_system_iostream {
            assert_eq!(span.src(), "#include <iostream>");
            if let IncludePath::System(path_span) = path {
                assert_eq!(path_span.src(), "iostream");
            } else {
                panic!("Expected a system include path, got {:#?}", path);
            }
        } else {
            panic!(
                "Wrong item: expected an include directive, got {:#?}",
                include_system_iostream
            );
        }

        let class_header = class_header_item_iter.next();
        if let Some(Item::Class(ItemClass {
            attrs,
            ident,
            generics,
            bases,
            fields: Fields::Named(fields_named),
        })) = class_header
        {
            assert_eq!(attrs.len(), 0);
            assert_eq!(Some("MyClass"), ident.as_ref().map(|id| id.sym));
            assert_eq!(&None, generics);

            if bases.len() == 1
                && let Some(base) = bases.first()
            {
                assert_eq!(base.access, Visibility::Public);
                assert!(!base.virtual_token);
                assert_eq!(base.path.to_string(), "MyMotherClass");
            } else {
                panic!(
                    "Wrong class.bases: expected an inheritance, got {:#?}",
                    bases
                );
            }

            let mut fields_named_iter = fields_named.members.iter();

            // 1. MACRO_DEF(param1, 1); → Verbatim
            let verbatim_item = fields_named_iter.next();
            if let Some(Member::Item(item)) = verbatim_item
                && let Item::Verbatim(verbatim) = item.as_ref()
            {
                assert!(!verbatim.tokens.is_empty());
            } else {
                panic!("Expected a macro verbatim, got {:#?}", verbatim_item);
            }

            // 2. typedef MyMotherClass BaseClass; → Typedef
            let typedef_item = fields_named_iter.next();
            if let Some(Member::Item(item)) = typedef_item
                && let Item::Typedef(td) = item.as_ref()
            {
                assert_eq!(td.ident.sym, "BaseClass");
            } else {
                panic!("Expected a typedef, got {:#?}", typedef_item);
            }

            // 3. public: → AccessSpecifier
            let access_public_static = fields_named_iter.next();
            assert_eq!(
                Some(&Member::AccessSpecifier(Visibility::Public)),
                access_public_static,
            );

            // 4. static const string STATIC_VALUE; → Field
            let static_field = fields_named_iter.next();
            if let Some(Member::Field(field)) = static_field {
                assert_eq!(Some("STATIC_VALUE"), field.ident.as_ref().map(|id| id.sym));
                assert!(field.static_token);
                assert_eq!(field.default_value, None);
            } else {
                panic!("Expected a static field, got {:#?}", static_field);
            }

            // 5. private: → AccessSpecifier
            let access_private = fields_named_iter.next();
            assert_eq!(
                Some(&Member::AccessSpecifier(Visibility::Private)),
                access_private,
            );

            // 6. int member_var; → Field
            let field_member_var = fields_named_iter.next();
            if let Some(Member::Field(field)) = field_member_var {
                assert_eq!(Some("member_var"), field.ident.as_ref().map(|id| id.sym));
                assert_eq!(field.default_value, None);
            } else {
                panic!("Expected a field, got {:#?}", field_member_var);
            }

            // 7. public: → AccessSpecifier
            let access_public1 = fields_named_iter.next();
            assert_eq!(
                Some(&Member::AccessSpecifier(Visibility::Public)),
                access_public1,
            );

            // 8. MyClass(int x) : member_var(x) {} → Constructor
            let constructor = fields_named_iter.next();
            if let Some(Member::Constructor(ctor)) = constructor {
                assert_eq!(ctor.ident.sym, "MyClass");
                assert!(!ctor.explicit_token);
                assert!(!ctor.constexpr_token);
                assert!(!ctor.noexcept_token);
                assert!(!ctor.defaulted);
                assert!(!ctor.deleted);
                assert_eq!(ctor.inputs.len(), 1);
                assert_eq!(ctor.member_init_list.len(), 1);
                assert_eq!(ctor.member_init_list[0].member.sym, "member_var");
                assert!(ctor.block.is_some());
            } else {
                panic!("Expected a constructor, got {:#?}", constructor);
            }

            // 9. public: → AccessSpecifier
            let access_public2 = fields_named_iter.next();
            assert_eq!(
                Some(&Member::AccessSpecifier(Visibility::Public)),
                access_public2,
            );

            // 10. void member_function(); → Method
            let method = fields_named_iter.next();
            if let Some(Member::Method(m)) = method {
                assert_eq!(m.sig.ident.sym, "member_function");
                assert!(m.block.is_none());
            } else {
                panic!("Expected a method, got {:#?}", method);
            }

            // No more members
            assert_eq!(None, fields_named_iter.next());
        } else {
            panic!(
                "Wrong item: expected a class definition, got {:#?}",
                class_header
            );
        }

        assert_eq!(None, class_header_item_iter.next());
    }
}
