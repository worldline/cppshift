use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse_str;

use crate::ast::{Ident, Path};

impl<'de> From<Ident<'de>> for syn::Ident {
    fn from(ident: Ident<'de>) -> Self {
        syn::Ident::new(ident.sym, proc_macro2::Span::call_site())
    }
}

impl<'de> From<&Ident<'de>> for syn::Ident {
    fn from(ident: &Ident<'de>) -> Self {
        syn::Ident::new(ident.sym, proc_macro2::Span::call_site())
    }
}

impl<'de> ToTokens for Ident<'de> {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let ident: syn::Ident = self.into();
        ident.to_tokens(tokens);
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
