use std::collections::{HashMap, HashSet};

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse_str;

use crate::{
    ast::{Field, Ident, ItemEnum, Path, Visibility},
    transpile::{Transpile, TranspileContext, TranspileError, Transpiler},
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
        ctx: &mut TranspileContext,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        if let Some(ident) = self.ident {
            self.vis.to_tokens(tokens);
            ident.to_tokens(tokens);
            tokens.extend(quote::quote! { : });
            self.ty.transpile(transpiler, ctx, tokens)?;
        }

        Ok(())
    }
}

impl<'de> Transpile for ItemEnum<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        ctx: &mut TranspileContext,
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
        let (ident_ty_attr, repr_attr) = match &self.underlying_type {
            Some(ty) => {
                let rust_ty = transpiler.ty_mapper.map_type(ty)?;
                (rust_ty.clone(), quote::quote! { #[repr(#rust_ty)] })
            }
            None => (syn::parse_quote!(i32), quote::quote! { #[repr(i32)] }),
        };

        // Pre-pass: compute effective values and deduplicate (keep last variant per value)
        let mut effective_values: Vec<Option<i128>> = Vec::with_capacity(self.variants.len());
        let mut auto_value: Option<i128> = Some(0);
        let mut value_to_last_index: HashMap<i128, usize> = HashMap::new();

        for (i, variant) in self.variants.iter().enumerate() {
            let val = match &variant.discriminant {
                Some(disc) => {
                    let evaluated = disc.const_eval_integer();
                    auto_value = evaluated.map(|v| v + 1);
                    evaluated
                }
                None => {
                    let val = auto_value;
                    auto_value = auto_value.map(|v| v + 1);
                    val
                }
            };
            if let Some(v) = val {
                value_to_last_index.insert(v, i);
            }
            effective_values.push(val);
        }

        let keep_indices: HashSet<usize> = (0..self.variants.len())
            .filter(|&i| match effective_values[i] {
                Some(val) => value_to_last_index[&val] == i,
                None => true,
            })
            .collect();

        // Build variant tokens
        let mut variant_tokens = TokenStream::new();
        let mut is_first_kept = true;
        for (i, variant) in self.variants.iter().enumerate() {
            if !keep_indices.contains(&i) {
                continue;
            }
            if is_first_kept && transpiler.enum_default_variant {
                variant_tokens.extend(quote::quote! { #[default] });
            }
            is_first_kept = false;
            let v_name: syn::Ident = (&variant.ident).into();
            if let Some(ref disc) = variant.discriminant {
                let mut expr_tokens = TokenStream::new();
                disc.transpile(transpiler, ctx, &mut expr_tokens)?;
                variant_tokens.extend(quote::quote! { #v_name = #expr_tokens, });
            } else {
                variant_tokens.extend(quote::quote! { #v_name, });
            }
        }

        // Collect match arms from the kept enum variants
        let variants_match_arms = self
            .variants
            .iter()
            .enumerate()
            .filter(|(i, _)| keep_indices.contains(i))
            .map(|(_, v)| {
                let variant_name = &v.ident;
                quote::quote! {
                    x if x == #name::#variant_name as #ident_ty_attr => Ok(#name::#variant_name),
                }
            });

        let derive_attr = if transpiler.enum_default_variant && !self.variants.is_empty() {
            quote::quote! { #[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)] }
        } else {
            quote::quote! { #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] }
        };

        tokens.extend(quote::quote! {
            #[doc = concat!(" Auto-transpiled enum for ", stringify!(#name))]
            #derive_attr
            #repr_attr
            pub enum #name { #variant_tokens }
        });

        // Implement try_from
        let try_from_impl = quote::quote! {
            impl std::convert::TryFrom<#ident_ty_attr> for #name {
                type Error = String;

                fn try_from(value: #ident_ty_attr) -> Result<Self, Self::Error> {
                    match value {
                        #(#variants_match_arms)*
                        _ => Err(format!("Invalid value {} for enum {}", value, stringify!(#name))),
                    }
                }
            }
        };
        tokens.extend(try_from_impl);

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
                    e.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    "# [doc = concat ! (\" Auto-transpiled enum for \" , stringify ! (Color))] # [derive (Debug , Clone , Copy , PartialEq , Eq , Hash)] # [repr (i32)] pub enum Color { Red , Green , Blue , } impl std :: convert :: TryFrom < i32 > for Color { type Error = String ; fn try_from (value : i32) -> Result < Self , Self :: Error > { match value { x if x == Color :: Red as i32 => Ok (Color :: Red) , x if x == Color :: Green as i32 => Ok (Color :: Green) , x if x == Color :: Blue as i32 => Ok (Color :: Blue) , _ => Err (format ! (\"Invalid value {} for enum {}\" , value , stringify ! (Color))) , } } }"
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
                    e.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    "# [doc = concat ! (\" Auto-transpiled enum for \" , stringify ! (Color))] # [derive (Debug , Clone , Copy , PartialEq , Eq , Hash)] # [repr (i32)] pub enum Color { Red , Green , Blue , } impl std :: convert :: TryFrom < i32 > for Color { type Error = String ; fn try_from (value : i32) -> Result < Self , Self :: Error > { match value { x if x == Color :: Red as i32 => Ok (Color :: Red) , x if x == Color :: Green as i32 => Ok (Color :: Green) , x if x == Color :: Blue as i32 => Ok (Color :: Blue) , _ => Err (format ! (\"Invalid value {} for enum {}\" , value , stringify ! (Color))) , } } }"
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
                    e.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    "# [doc = concat ! (\" Auto-transpiled enum for \" , stringify ! (Color))] # [derive (Debug , Clone , Copy , PartialEq , Eq , Hash)] # [repr (u8)] pub enum Color { A = 1 , B = 2 , } impl std :: convert :: TryFrom < u8 > for Color { type Error = String ; fn try_from (value : u8) -> Result < Self , Self :: Error > { match value { x if x == Color :: A as u8 => Ok (Color :: A) , x if x == Color :: B as u8 => Ok (Color :: B) , _ => Err (format ! (\"Invalid value {} for enum {}\" , value , stringify ! (Color))) , } } }"
                );
            }
            item => panic!("expected ItemEnum, got {item:?}"),
        }

        Ok(())
    }

    #[test]
    fn enum_with_default_variant_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler {
            enum_default_variant: true,
            ..Default::default()
        };
        let src = "enum class Color { Red, Green, Blue };";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Enum(e) => {
                let result = e
                    .transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                    .to_string();
                assert!(result.contains("# [default]"));
                assert!(result.contains(
                    "# [derive (Debug , Default , Clone , Copy , PartialEq , Eq , Hash)]"
                ));
            }
            item => panic!("expected ItemEnum, got {item:?}"),
        }

        Ok(())
    }

    #[test]
    fn enum_duplicate_values_keeps_last() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "enum class E { TOTO = 1, TITI = 1 };";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Enum(e) => {
                let result = e
                    .transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                    .to_string();
                assert!(
                    !result.contains("TOTO"),
                    "TOTO should be removed as duplicate"
                );
                assert!(result.contains("TITI = 1"), "TITI should be kept");
            }
            item => panic!("expected ItemEnum, got {item:?}"),
        }
        Ok(())
    }

    #[test]
    fn enum_duplicate_auto_increment_keeps_last() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        // A=0, B=1 (auto), C=1 (explicit) → B is duplicate of C, keep C
        let src = "enum class E { A = 0, B, C = 1 };";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Enum(e) => {
                let result = e
                    .transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                    .to_string();
                assert!(result.contains("A = 0"), "A should be kept");
                assert!(!result.contains("B ,"), "B should be removed as duplicate");
                assert!(result.contains("C = 1"), "C should be kept");
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
                            .transpile_token_stream(&transpiler, &mut TranspileContext::default())
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
                            .transpile_token_stream(&transpiler, &mut TranspileContext::default())
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
                            .transpile_token_stream(&transpiler, &mut TranspileContext::default())
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
