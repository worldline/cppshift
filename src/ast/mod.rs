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
    #[tokio::test]
    async fn main_ast() {
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
                "Wrong first item: expected an include directive, got {:#?}",
                include_system_iostream
            );
        }

        let include_local_iostream = main_item_iter.next();
        if let Some(Item::Include(ItemInclude { span, path })) = include_local_iostream {
            assert_eq!(span.src(), "#include \"module/myheader.h\"");
            if let IncludePath::Local(path_span) = path {
                assert_eq!(path_span.src(), "module/myheader.h");
            } else {
                panic!("Expected a local include path, got {:#?}", path);
            }
        } else {
            panic!(
                "Wrong first item: expected an include directive, got {:#?}",
                include_local_iostream
            );
        }

        /*for item in &main_file.items {
            panic!("{:#?}", item);
        }*/

        assert!(!main_file.items.is_empty());
    }
}
