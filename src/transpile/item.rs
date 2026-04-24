use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse_str;

use crate::{
    ast::{Field, Ident, ItemEnum, Path, Visibility},
    transpile::{Transpile, TranspileError, Transpiler},
};

impl<'de> From<&Ident<'de>> for syn::Ident {
    fn from(ident: &Ident<'de>) -> Self {
        syn::Ident::new(ident.sym, proc_macro2::Span::call_site())
    }
}

impl<'de> From<Ident<'de>> for syn::Ident {
    fn from(ident: Ident<'de>) -> Self {
        Self::from(&ident)
    }
}

impl<'de> ToTokens for Ident<'de> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ident: syn::Ident = self.into();
        ident.to_tokens(tokens);
    }
}

impl From<&Visibility> for syn::Visibility {
    fn from(visibility: &Visibility) -> Self {
        match visibility {
            Visibility::Public => syn::parse_str("pub").expect("Failed to parse pub visibility"),
            Visibility::Protected => {
                syn::parse_str("pub(crate)").expect("Failed to parse pub(crate) visibility")
            }
            Visibility::Inherited | Visibility::Private => syn::Visibility::Inherited,
        }
    }
}

impl From<Visibility> for syn::Visibility {
    fn from(visibility: Visibility) -> Self {
        Self::from(&visibility)
    }
}

impl ToTokens for Visibility {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let visibility: syn::Visibility = self.into();
        visibility.to_tokens(tokens);
    }
}

macro_rules! impl_try_from_path {
    ($($target:ty),* $(,)?) => {
        $(
            impl<'de> TryFrom<Path<'de>> for $target {
                type Error = syn::Error;

                fn try_from(path: Path<'de>) -> Result<Self, Self::Error> {
                    parse_str(&path.to_string())
                }
            }
        )*
    };
}
impl_try_from_path!(syn::Type, syn::Path, syn::Expr);

impl<'de> Transpile for Field<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        if let Some(ident) = self.ident {
            self.vis.to_tokens(tokens);
            ident.to_tokens(tokens);
            tokens.extend(quote::quote! { : });
            self.ty.transpile(transpiler, tokens)?;
        }

        Ok(())
    }
}

impl<'de> Transpile for ItemEnum<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        let name: syn::Ident = self
            .ident
            .as_ref()
            .ok_or_else(|| TranspileError::UnsupportedType {
                message: "anonymous enums cannot be transpiled".to_owned(),
                src: String::new(),
                err_span: miette::SourceSpan::new(0.into(), 0),
            })?
            .into();

        // Build #[repr(...)] if an underlying type is specified
        let repr_attr = match &self.underlying_type {
            Some(ty) => {
                let rust_ty = transpiler.ty_mapper.map_type(ty)?;
                quote::quote! { #[repr(#rust_ty)] }
            }
            None => quote::quote! { #[repr(i32)] },
        };

        // Build variant tokens
        let mut variant_tokens = TokenStream::new();
        for variant in self.variants.iter() {
            let v_name: syn::Ident = (&variant.ident).into();
            if let Some(ref disc) = variant.discriminant {
                let mut expr_tokens = TokenStream::new();
                disc.transpile(transpiler, &mut expr_tokens)?;
                variant_tokens.extend(quote::quote! { #v_name = #expr_tokens, });
            } else {
                variant_tokens.extend(quote::quote! { #v_name, });
            }
        }

        tokens.extend(quote::quote! {
            #[doc = concat!(" Auto-transpiled enum for ", stringify!(#name))]
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            #repr_attr
            pub enum #name { #variant_tokens }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use core::panic;

    use super::*;
    use crate::ast::{self, parse_file};

    // ---- ItemEnum transpilation ----

    #[test]
    fn enum_class_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "enum class Color { Red, Green, Blue };";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Enum(e) => {
                assert_eq!(
                    e.transpile_token_stream(&transpiler)?.to_string(),
                    "# [doc = concat ! (\" Auto-transpiled enum for \" , stringify ! (Color))] # [derive (Debug , Clone , Copy , PartialEq , Eq)] # [repr (i32)] pub enum Color { Red , Green , Blue , }"
                );
            }
            item => panic!("expected ItemEnum, got {item:?}"),
        }

        Ok(())
    }

    #[test]
    fn enum_with_underlying_type_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "enum Color : int { Red, Green, Blue };";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Enum(e) => {
                assert_eq!(
                    e.transpile_token_stream(&transpiler)?.to_string(),
                    "# [doc = concat ! (\" Auto-transpiled enum for \" , stringify ! (Color))] # [derive (Debug , Clone , Copy , PartialEq , Eq)] # [repr (i32)] pub enum Color { Red , Green , Blue , }"
                );
            }
            item => panic!("expected ItemEnum, got {item:?}"),
        }

        Ok(())
    }

    #[test]
    fn enum_with_discriminants_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "enum class Color : unsigned char { A = 1, B = 2 };";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Enum(e) => {
                assert_eq!(
                    e.transpile_token_stream(&transpiler)?.to_string(),
                    "# [doc = concat ! (\" Auto-transpiled enum for \" , stringify ! (Color))] # [derive (Debug , Clone , Copy , PartialEq , Eq)] # [repr (u8)] pub enum Color { A = 1 , B = 2 , }"
                );
            }
            item => panic!("expected ItemEnum, got {item:?}"),
        }

        Ok(())
    }

    #[test]
    fn class_member_variable_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src =
            "class Color { public: int R; private: unsigned long long G; protected: short B; };";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            ast::Item::Class(ast::ItemClass {
                fields: ast::Fields::Named(named_fields),
                ..
            }) => {
                if let ast::Member::Field(field) = &named_fields.members[1] {
                    assert_eq!(
                        field
                            .transpile_token_stream(&transpiler)
                            .expect("Failed to transpile field[0]")
                            .to_string(),
                        "pub R : i32",
                    );
                } else {
                    panic!(
                        "expected field member[0], got {:?}",
                        &named_fields.members[0]
                    );
                }

                if let ast::Member::Field(field) = &named_fields.members[3] {
                    assert_eq!(
                        field
                            .transpile_token_stream(&transpiler)
                            .expect("Failed to transpile field[3]")
                            .to_string(),
                        "G : u64",
                    );
                } else {
                    panic!(
                        "expected field member[3], got {:?}",
                        &named_fields.members[3]
                    );
                }

                if let ast::Member::Field(field) = &named_fields.members[5] {
                    assert_eq!(
                        field
                            .transpile_token_stream(&transpiler)
                            .expect("Failed to transpile field[5]")
                            .to_string(),
                        "pub (crate) B : i16",
                    );
                } else {
                    panic!(
                        "expected field member[5], got {:?}",
                        &named_fields.members[5]
                    );
                }
            }
            item => panic!("expected ItemClass, got {item:?}"),
        }

        Ok(())
    }
}
