use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::ToTokens as _;
use serde::Deserialize;
use serde::de::{self, MapAccess, Visitor};

use crate::SourceCodeSpan as _;
use crate::ast::ItemTypedef;
use crate::ast::expr::{Expr, ExprLit, LitKind};
use crate::ast::item::{ItemConst, ItemStatic, Path};
use crate::ast::ty::{FundamentalKind, TemplateArg, Type};
use crate::transpile::{Transpile, TranspileContext, Transpiler};

use super::error::TranspileError;

impl From<FundamentalKind> for syn::Type {
    fn from(kind: FundamentalKind) -> Self {
        use FundamentalKind::*;
        let s = match kind {
            Void => "()",
            Bool => "bool",
            Char | Char8 | Char16 | Char32 | Wchar => "char",
            UnsignedChar => "u8",
            UnsignedShort => "u16",
            UnsignedInt => "u32",
            Short => "i16",
            Int => "i32",
            Long | LongLong => "i64",
            Float => "f32",
            Double | LongDouble => "f64",
            SignedChar => "i8",
            UnsignedLong | UnsignedLongLong => "u64",
        };
        syn::parse_str(s).unwrap()
    }
}

/// Configurable mapper from C++ AST types to `syn::Type`.
///
/// Built via [`TypeMapper::builder()`] or [`TypeMapper::new()`] (defaults only).
///
/// ```
/// use cppshift::transpile::TypeMapper;
/// use cppshift::ast::ty::{Type, TypeFundamental, FundamentalKind};
/// use cppshift::SourceSpan;
///
/// let mapper = TypeMapper::new();
/// let src = "int";
/// let ty = Type::Fundamental(TypeFundamental {
///     span: SourceSpan::new(src, 0, 3),
///     kind: FundamentalKind::Int,
/// });
/// let rust_ty = mapper.map_type(&ty).expect("fundamental types always map");
/// assert_eq!(quote::quote!(#rust_ty).to_string(), "i32");
/// ```
#[derive(Debug, Clone)]
pub struct TypeMapper {
    paths: HashMap<String, syn::Type>,
}

/// Builder for [`TypeMapper`].
pub struct TypeMapperBuilder {
    paths: HashMap<String, syn::Type>,
}

impl TypeMapper {
    /// Create a builder for configuring type mappings.
    pub fn builder() -> TypeMapperBuilder {
        TypeMapperBuilder {
            paths: HashMap::new(),
        }
    }

    /// Create a mapper with default fundamental type mappings only.
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Map a C++ AST type to a `syn::Type`.
    ///
    /// # Errors
    ///
    /// Returns [`TranspileError`] if the type cannot be mapped (e.g. unknown path,
    /// `auto`, `decltype`, unsized array).
    pub fn map_type(&self, ty: &Type<'_>) -> Result<syn::Type, TranspileError> {
        match ty {
            Type::Fundamental(f) => Ok(syn::Type::from(f.kind)),
            Type::Path(p) => self.resolve_path(&p.path),
            Type::Ptr(p) => {
                let inner = self.map_type(&p.pointee)?;
                if p.cv.const_token {
                    Ok(syn::parse_quote!(*const #inner))
                } else {
                    Ok(syn::parse_quote!(*mut #inner))
                }
            }
            Type::Reference(r) => {
                let inner = self.map_type(&r.referent)?;
                if r.cv.const_token {
                    Ok(syn::parse_quote!(&#inner))
                } else {
                    Ok(syn::parse_quote!(&mut #inner))
                }
            }
            Type::RvalueReference(r) => self.map_type(&r.referent),
            Type::Array(a) => {
                let inner = self.map_type(&a.element)?;
                match &a.size {
                    Some(Expr::Lit(lit)) if lit.kind == LitKind::Integer => {
                        let n: usize =
                            lit.span.src().parse().map_err(|_| {
                                unsupported_from_type("invalid array size literal", ty)
                            })?;
                        let lit_n =
                            syn::LitInt::new(&n.to_string(), proc_macro2::Span::call_site());
                        Ok(syn::parse_quote!([#inner; #lit_n]))
                    }
                    _ => Err(unsupported_from_type("unsized or dynamic array", ty)),
                }
            }
            Type::FnPtr(f) => {
                let ret = self.map_type(&f.return_type)?;
                let params: Result<Vec<syn::Type>, _> =
                    f.params.iter().map(|p| self.map_type(p)).collect();
                let params = params?;
                Ok(syn::parse_quote!(fn(#(#params),*) -> #ret))
            }
            Type::Qualified(q) => self.map_type(&q.ty),
            Type::TemplateInst(t) => {
                let base_ty = self.resolve_path(&t.path)?;
                let mapped_args: Result<Vec<syn::Type>, _> = t
                    .args
                    .iter()
                    .map(|arg| match arg {
                        TemplateArg::Type(ty) => self.map_type(ty),
                        TemplateArg::Expr(_) => {
                            Err(unsupported_from_type("expression template argument", ty))
                        }
                    })
                    .collect();
                let mapped_args = mapped_args?;

                let mut result = base_ty;
                if let syn::Type::Path(ref mut type_path) = result
                    && let Some(last_seg) = type_path.path.segments.last_mut()
                {
                    last_seg.arguments =
                        syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
                            colon2_token: None,
                            lt_token: syn::token::Lt::default(),
                            args: mapped_args
                                .into_iter()
                                .map(syn::GenericArgument::Type)
                                .collect(),
                            gt_token: syn::token::Gt::default(),
                        });
                }
                Ok(result)
            }
            Type::Auto(_) => Err(unsupported_from_type(
                "auto type cannot be mapped to Rust",
                ty,
            )),
            Type::Decltype(_) => Err(unsupported_from_type(
                "decltype cannot be mapped to Rust",
                ty,
            )),
        }
    }

    fn resolve_path(&self, path: &Path<'_>) -> Result<syn::Type, TranspileError> {
        let key = path.to_string();
        if let Some(ty) = self.paths.get(&key) {
            Ok(ty.clone())
        } else {
            syn::Type::try_from(path.clone()).map_err(|_| unmapped_path_error(&key, path))
        }
    }
}

impl Default for TypeMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl<'de> Deserialize<'de> for TypeMapper {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TypeMapperVisitor;

        impl<'de> Visitor<'de> for TypeMapperVisitor {
            type Value = TypeMapper;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a map of C++ type paths to Rust type strings")
            }

            fn visit_map<M>(self, mut access: M) -> Result<TypeMapper, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut builder = TypeMapper::builder();
                while let Some((cpp_path, rust_type)) = access.next_entry::<String, String>()? {
                    builder = builder
                        .map_path(&cpp_path, &rust_type)
                        .map_err(de::Error::custom)?;
                }
                Ok(builder.build())
            }
        }

        deserializer.deserialize_map(TypeMapperVisitor)
    }
}

impl TypeMapperBuilder {
    /// Register a C++ path → Rust type mapping.
    ///
    /// The `rust_type` string is parsed via `syn::parse_str`.
    ///
    /// # Errors
    ///
    /// Returns [`TranspileError::InvalidRustType`] if `rust_type` is not valid Rust syntax.
    pub fn map_path(mut self, cpp_path: &str, rust_type: &str) -> Result<Self, TranspileError> {
        let ty: syn::Type =
            syn::parse_str(rust_type).map_err(|e| TranspileError::InvalidRustType {
                rust_type: rust_type.to_owned(),
                reason: e.to_string(),
            })?;
        self.paths.insert(cpp_path.to_owned(), ty);
        Ok(self)
    }

    /// Register a C++ path → Rust type mapping with a pre-built `syn::Type`.
    pub fn map_path_to_type(mut self, cpp_path: &str, ty: syn::Type) -> Self {
        self.paths.insert(cpp_path.to_owned(), ty);
        self
    }

    /// Build the [`TypeMapper`].
    pub fn build(self) -> TypeMapper {
        TypeMapper { paths: self.paths }
    }
}

/// Build an [`TranspileError::UnmappedPath`] from a path string and AST path.
fn unmapped_path_error(path_str: &str, path: &Path<'_>) -> TranspileError {
    let span = path.segments.first().map(|s| s.ident.span);
    match span {
        Some(span) => TranspileError::UnmappedPath {
            path: path_str.to_owned(),
            src: span.full_source().to_owned(),
            err_span: span.into(),
        },
        None => TranspileError::UnmappedPath {
            path: path_str.to_owned(),
            src: String::new(),
            err_span: miette::SourceSpan::new(0.into(), 0),
        },
    }
}

/// Build an [`TranspileError::UnsupportedType`] by extracting the best span from a [`Type`].
fn unsupported_from_type(message: &str, ty: &Type<'_>) -> TranspileError {
    match ty.span() {
        Some(span) => TranspileError::UnsupportedType {
            message: message.to_owned(),
            src: span.full_source().to_owned(),
            err_span: span.into(),
        },
        None => TranspileError::UnsupportedType {
            message: format!("{}: {:?}", message, ty),
            src: String::new(),
            err_span: miette::SourceSpan::new(0.into(), 0),
        },
    }
}

impl<'de> Transpile for Type<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        _ctx: &mut TranspileContext,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        transpiler.ty_mapper.map_type(self)?.to_tokens(tokens);
        Ok(())
    }
}

impl<'de> Transpile for ItemTypedef<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        _ctx: &mut TranspileContext,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        let name = self.ident;
        // char array typedefs → &str
        if let Type::Array(arr) = &self.ty
            && is_char_element_type(&arr.element)
        {
            tokens.extend(quote::quote! {
                #[doc = concat!(" Auto-transpiled type for ", stringify!(#name))]
                pub type #name = &str;
            });
        } else if let Type::Path(p) = &self.ty {
            let rust_ty = transpiler.ty_mapper.resolve_path(&p.path)?;
            tokens.extend(quote::quote! {
                #[doc = concat!(" Auto-transpiled type for ", stringify!(#name))]
                pub type #name = #rust_ty;
            });
        } else {
            let rust_ty = transpiler.ty_mapper.map_type(&self.ty)?;
            tokens.extend(quote::quote! {
                #[doc = concat!(" Auto-transpiled type for ", stringify!(#name))]
                pub type #name = #rust_ty;
            });
        }

        Ok(())
    }
}

/// Returns `true` if `ty` is a byte-sized char type (char, char8_t, unsigned char, signed char),
/// stripping CV-qualifiers.
fn is_char_element_type(ty: &Type<'_>) -> bool {
    use FundamentalKind::*;
    match ty {
        Type::Fundamental(f) => matches!(f.kind, Char | Char8 | Char16 | Char32 | Wchar),
        Type::Qualified(q) => is_char_element_type(&q.ty),
        _ => false,
    }
}

/// Try to transpile an unsized char array initialised with a string literal.
///
/// C++: `const char foo[] = "ALPN";` / `static char foo[] = "ALPN";`
/// Rust: `pub static foo: &str = "ALPN";`
///
/// `keyword` is the Rust storage keyword to emit (`const` or `static`).
/// Returns `None` if the pattern doesn't match and normal mapping should proceed.
fn try_transpile_char_array_from_str_lit<'de>(
    name: crate::ast::item::Ident<'de>,
    element: &Type<'de>,
    expr: &Expr<'de>,
    transpiler: &Transpiler,
    ctx: &mut TranspileContext,
    keyword: &str,
    tokens: &mut TokenStream,
) -> Option<Result<(), TranspileError>> {
    if !is_char_element_type(element) {
        return None;
    }
    let Expr::Lit(ExprLit {
        kind: LitKind::String,
        ..
    }) = expr
    else {
        return None;
    };

    let mut expr_tokens = TokenStream::new();
    if let Err(e) = expr.transpile(transpiler, ctx, &mut expr_tokens) {
        return Some(Err(e));
    }

    let keyword_tok: proc_macro2::TokenStream = keyword.parse().unwrap();
    tokens.extend(quote::quote! {
        pub #keyword_tok #name: &str = #expr_tokens;
    });
    Some(Ok(()))
}

impl<'de> Transpile for ItemConst<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        ctx: &mut TranspileContext,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        let name = self.ident;

        // Special case: `const char foo[] = "ALPN";` → `pub const foo: [u8; 5] = *b"ALPN\0";`
        if let Type::Array(arr) = &self.ty
            && arr.size.is_none()
            && let Some(result) = try_transpile_char_array_from_str_lit(
                name,
                &arr.element,
                &self.expr,
                transpiler,
                ctx,
                "const",
                tokens,
            )
        {
            return result;
        }

        let mut expr_tokens = TokenStream::new();
        self.expr.transpile(transpiler, ctx, &mut expr_tokens)?;

        match &self.expr {
            // C++ string constants with string literal init → `&str`
            Expr::Lit(ExprLit {
                kind: LitKind::String,
                ..
            }) => {
                tokens.extend(quote::quote! {
                    #[doc = " Auto-transpiled &str const"]
                    pub const #name: &str = #expr_tokens;
                });
            }
            Expr::Lit(ExprLit { kind, .. }) => {
                let rust_ty = transpiler.ty_mapper.map_type(&self.ty)?;
                if kind.match_type(&self.ty) {
                    tokens.extend(quote::quote! {
                        #[doc = concat!(" Auto-transpiled const literal ", stringify!(#rust_ty))]
                        pub const #name: #rust_ty = #expr_tokens;
                    });
                } else {
                    tokens.extend(quote::quote! {
                        #[doc = concat!(" Auto-transpiled const literal ", stringify!(#rust_ty))]
                        pub const #name: #rust_ty = #expr_tokens as #rust_ty;
                    });
                }
            }
            _ => {
                let rust_ty = transpiler.ty_mapper.map_type(&self.ty)?;
                tokens.extend(quote::quote! {
                    #[doc = concat!(" Auto-transpiled const ", stringify!(#rust_ty))]
                    pub const #name: #rust_ty = #expr_tokens;
                });
            }
        }

        Ok(())
    }
}

impl<'de> Transpile for ItemStatic<'de> {
    fn transpile(
        &self,
        transpiler: &Transpiler,
        ctx: &mut TranspileContext,
        tokens: &mut TokenStream,
    ) -> Result<(), TranspileError> {
        let name = self.ident;
        let expr = self
            .expr
            .as_ref()
            .ok_or_else(|| TranspileError::UnsupportedExpr {
                message: "Rust statics require an initializer".to_owned(),
                src: name.span.full_source().to_owned(),
                err_span: name.span.into(),
            })?;

        // Special case: `static char foo[] = "ALPN";` → `pub static foo: [u8; 5] = *b"ALPN\0";`
        if let Type::Array(arr) = &self.ty
            && arr.size.is_none()
            && let Some(result) = try_transpile_char_array_from_str_lit(
                name,
                &arr.element,
                expr,
                transpiler,
                ctx,
                "static",
                tokens,
            )
        {
            return result;
        }

        let mut expr_tokens = TokenStream::new();
        expr.transpile(transpiler, ctx, &mut expr_tokens)?;

        match expr {
            // C++ string statics with string literal init → `&str`
            Expr::Lit(ExprLit {
                kind: LitKind::String,
                ..
            }) => {
                tokens.extend(quote::quote! {
                    #[doc = " Auto-transpiled &str static"]
                    pub static #name: &str = #expr_tokens;
                });
            }
            Expr::Lit(ExprLit { kind, .. }) => {
                let rust_ty = transpiler.ty_mapper.map_type(&self.ty)?;
                if kind.match_type(&self.ty) {
                    tokens.extend(quote::quote! {
                        #[doc = concat!(" Auto-transpiled static literal ", stringify!(#rust_ty))]
                        pub static #name: #rust_ty = #expr_tokens;
                    });
                } else {
                    tokens.extend(quote::quote! {
                        #[doc = concat!(" Auto-transpiled static literal ", stringify!(#rust_ty))]
                        pub static #name: #rust_ty = #expr_tokens as #rust_ty;
                    });
                }
            }
            _ => {
                let rust_ty = transpiler.ty_mapper.map_type(&self.ty)?;
                tokens.extend(quote::quote! {
                    #[doc = concat!(" Auto-transpiled static ", stringify!(#rust_ty))]
                    pub static #name: #rust_ty = #expr_tokens;
                });
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    use crate::SourceSpan;
    use crate::ast::expr::ExprLit;
    use crate::ast::item::{Ident, PathSegment};
    use crate::ast::punct::Punctuated;
    use crate::ast::{parse_file, ty::*};

    fn ty_str(ty: &syn::Type) -> String {
        quote!(#ty).to_string()
    }

    fn make_fundamental(src: &str, kind: FundamentalKind) -> Type<'_> {
        Type::Fundamental(TypeFundamental {
            span: SourceSpan::new(src, 0, src.len()),
            kind,
        })
    }

    fn make_path<'a>(src: &'a str, segments: &[&'a str]) -> Type<'a> {
        Type::Path(TypePath {
            path: make_raw_path(src, segments),
        })
    }

    fn make_raw_path<'a>(src: &'a str, segments: &[&'a str]) -> Path<'a> {
        Path {
            leading_colon: false,
            segments: segments
                .iter()
                .map(|s| {
                    let offset = s.as_ptr() as usize - src.as_ptr() as usize;
                    PathSegment {
                        ident: Ident {
                            sym: s,
                            span: SourceSpan::new(src, offset, s.len()),
                        },
                    }
                })
                .collect(),
        }
    }

    #[test]
    fn typedef_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler {
            ty_mapper: TypeMapper::builder()
                .map_path("std::string", "BytesMut")?
                .build(),
            ..Default::default()
        };

        let typedef_header = r#"
            typedef Custom::int16 MyInt16;
            typedef std::string MyString;
            typedef char type24[3];
        "#;

        let typedef_file = parse_file(typedef_header).unwrap();
        let mut typedef_iter = typedef_file.items.iter();

        match typedef_iter.next() {
            Some(crate::ast::Item::Typedef(t)) => {
                assert_eq!(
                    "# [doc = concat ! (\" Auto-transpiled type for \" , stringify ! (MyInt16))] pub type MyInt16 = Custom :: int16 ;",
                    t.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string()
                );
            }
            t => panic!("unexpected typedef {t:?}"),
        };

        match typedef_iter.next() {
            Some(crate::ast::Item::Typedef(t)) => {
                assert_eq!(
                    "# [doc = concat ! (\" Auto-transpiled type for \" , stringify ! (MyString))] pub type MyString = BytesMut ;",
                    t.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string()
                );
            }
            t => panic!("unexpected typedef {t:?}"),
        };

        match typedef_iter.next() {
            Some(crate::ast::Item::Typedef(t)) => {
                assert_eq!(
                    "# [doc = concat ! (\" Auto-transpiled type for \" , stringify ! (type24))] pub type type24 = & str ;",
                    t.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string()
                );
            }
            t => panic!("unexpected typedef {t:?}"),
        };

        Ok(())
    }

    // ---- Fundamental types (table-driven) ----

    #[test]
    fn fundamental_defaults() -> Result<(), TranspileError> {
        use FundamentalKind::*;
        let mapper = TypeMapper::new();
        let cases: &[(FundamentalKind, &str, &str)] = &[
            (Void, "void", "()"),
            (Bool, "bool", "bool"),
            (Char, "char", "char"),
            (Char8, "char8_t", "char"),
            (Char16, "char16_t", "char"),
            (Char32, "char32_t", "char"),
            (Wchar, "wchar_t", "char"),
            (Short, "short", "i16"),
            (Int, "int", "i32"),
            (Long, "long", "i64"),
            (LongLong, "long long", "i64"),
            (Float, "float", "f32"),
            (Double, "double", "f64"),
            (LongDouble, "long double", "f64"),
            (SignedChar, "signed char", "i8"),
            (UnsignedChar, "unsigned char", "u8"),
            (UnsignedShort, "unsigned short", "u16"),
            (UnsignedInt, "unsigned int", "u32"),
            (UnsignedLong, "unsigned long", "u64"),
            (UnsignedLongLong, "unsigned long long", "u64"),
        ];
        for &(kind, src, expected) in cases {
            let ty = make_fundamental(src, kind);
            let result = mapper.map_type(&ty)?;
            assert_eq!(ty_str(&result), expected, "failed for {kind:?}");
        }
        Ok(())
    }

    // ---- Path mappings ----

    #[test]
    fn path_mapping_custom() -> Result<(), TranspileError> {
        let mapper = TypeMapper::builder()
            .map_path("Custom::int32", "i32")?
            .map_path("std::string", "String")?
            .build();

        let src = "Custom::int32";
        let ty = make_path(src, &[&src[..6], &src[8..]]);
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "i32");

        let src2 = "std::string";
        let ty2 = make_path(src2, &[&src2[..3], &src2[5..]]);
        assert_eq!(ty_str(&mapper.map_type(&ty2)?), "String");
        Ok(())
    }

    #[test]
    fn path_unknown_passes_through() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let src = "Unknown";
        let ty = make_path(src, &[src]);
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "Unknown");
        Ok(())
    }

    // ---- Composite types ----

    #[test]
    fn const_ptr() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let src = "int";
        let inner = make_fundamental(src, FundamentalKind::Int);
        let ty = Type::Ptr(TypePtr {
            cv: CvQualifiers {
                const_token: true,
                volatile_token: false,
            },
            pointee: Box::new(inner),
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "* const i32");
        Ok(())
    }

    #[test]
    fn mut_ptr() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let src = "int";
        let inner = make_fundamental(src, FundamentalKind::Int);
        let ty = Type::Ptr(TypePtr {
            cv: CvQualifiers::default(),
            pointee: Box::new(inner),
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "* mut i32");
        Ok(())
    }

    #[test]
    fn reference_mut() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let src = "int";
        let inner = make_fundamental(src, FundamentalKind::Int);
        let ty = Type::Reference(TypeReference {
            cv: CvQualifiers::default(),
            referent: Box::new(inner),
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "& mut i32");
        Ok(())
    }

    #[test]
    fn reference_const() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let src = "int";
        let inner = make_fundamental(src, FundamentalKind::Int);
        let ty = Type::Reference(TypeReference {
            cv: CvQualifiers {
                const_token: true,
                volatile_token: false,
            },
            referent: Box::new(inner),
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "& i32");
        Ok(())
    }

    #[test]
    fn rvalue_reference() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let src = "int";
        let inner = make_fundamental(src, FundamentalKind::Int);
        let ty = Type::RvalueReference(TypeRvalueReference {
            referent: Box::new(inner),
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "i32");
        Ok(())
    }

    #[test]
    fn array_with_size() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let src_elem = "int";
        let src_size = "10";
        let inner = make_fundamental(src_elem, FundamentalKind::Int);
        let ty = Type::Array(TypeArray {
            element: Box::new(inner),
            size: Some(Expr::Lit(ExprLit {
                span: SourceSpan::new(src_size, 0, 2),
                kind: LitKind::Integer,
            })),
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "[i32 ; 10]");
        Ok(())
    }

    #[test]
    fn array_without_size() {
        let mapper = TypeMapper::new();
        let src = "int";
        let inner = make_fundamental(src, FundamentalKind::Int);
        let ty = Type::Array(TypeArray {
            element: Box::new(inner),
            size: None,
        });
        assert!(mapper.map_type(&ty).is_err());
    }

    // ---- CV-qualified ----

    #[test]
    fn cv_qualified_strips() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let src = "int";
        let inner = make_fundamental(src, FundamentalKind::Int);
        let ty = Type::Qualified(TypeQualified {
            cv: CvQualifiers {
                const_token: true,
                volatile_token: false,
            },
            ty: Box::new(inner),
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "i32");
        Ok(())
    }

    // ---- Template instantiation ----

    #[test]
    fn template_inst_with_mapped_args() -> Result<(), TranspileError> {
        let mapper = TypeMapper::builder()
            .map_path("std::vector", "Vec")?
            .build();

        let path_src = "std::vector";
        let inner_src = "int";
        let inner = make_fundamental(inner_src, FundamentalKind::Int);
        let ty = Type::TemplateInst(TypeTemplateInst {
            path: make_raw_path(path_src, &[&path_src[..3], &path_src[5..]]),
            args: vec![TemplateArg::Type(inner)],
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "Vec < i32 >");
        Ok(())
    }

    #[test]
    fn template_inst_unknown_path_passes_through() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let path_src = "std::deque";
        let inner_src = "int";
        let inner = make_fundamental(inner_src, FundamentalKind::Int);
        let ty = Type::TemplateInst(TypeTemplateInst {
            path: make_raw_path(path_src, &[&path_src[..3], &path_src[5..]]),
            args: vec![TemplateArg::Type(inner)],
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "std :: deque < i32 >");
        Ok(())
    }

    #[test]
    fn template_inst_unmapped_arg_passes_through() -> Result<(), TranspileError> {
        let mapper = TypeMapper::builder()
            .map_path("std::vector", "Vec")?
            .build();

        let path_src = "std::vector";
        let inner_src = "Unknown";
        let inner = make_path(inner_src, &[inner_src]);
        let ty = Type::TemplateInst(TypeTemplateInst {
            path: make_raw_path(path_src, &[&path_src[..3], &path_src[5..]]),
            args: vec![TemplateArg::Type(inner)],
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "Vec < Unknown >");
        Ok(())
    }

    // ---- auto / decltype ----

    #[test]
    fn auto_returns_err() {
        let mapper = TypeMapper::new();
        let src = "auto";
        let ty = Type::Auto(TypeAuto {
            span: SourceSpan::new(src, 0, 4),
        });
        assert!(mapper.map_type(&ty).is_err());
    }

    #[test]
    fn decltype_returns_err() {
        let mapper = TypeMapper::new();
        let src = "x";
        let ty = Type::Decltype(TypeDecltype {
            expr: Expr::Ident(crate::ast::expr::ExprIdent {
                ident: Ident {
                    sym: src,
                    span: SourceSpan::new(src, 0, 1),
                },
            }),
        });
        assert!(mapper.map_type(&ty).is_err());
    }

    // ---- Inner type unknown in composite ----

    #[test]
    fn ptr_unknown_inner_passes_through() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let src = "Unknown";
        let inner = make_path(src, &[src]);
        let ty = Type::Ptr(TypePtr {
            cv: CvQualifiers::default(),
            pointee: Box::new(inner),
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "* mut Unknown");
        Ok(())
    }

    // ---- Function pointer ----

    #[test]
    fn fn_ptr() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let ret_src = "int";
        let p1_src = "double";
        let p2_src = "float";

        let ret = make_fundamental(ret_src, FundamentalKind::Int);
        let p1 = make_fundamental(p1_src, FundamentalKind::Double);
        let p2 = make_fundamental(p2_src, FundamentalKind::Float);

        let mut params = Punctuated::new();
        params.push_value(p1);
        params.push_value(p2);

        let ty = Type::FnPtr(TypeFnPtr {
            return_type: Box::new(ret),
            params,
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "fn (f64 , f32) -> i32");
        Ok(())
    }

    #[test]
    fn fn_ptr_unmapped_param_passes_through() -> Result<(), TranspileError> {
        let mapper = TypeMapper::new();
        let ret_src = "int";
        let p_src = "Unknown";

        let ret = make_fundamental(ret_src, FundamentalKind::Int);
        let p = make_path(p_src, &[p_src]);

        let mut params = Punctuated::new();
        params.push_value(p);

        let ty = Type::FnPtr(TypeFnPtr {
            return_type: Box::new(ret),
            params,
        });
        assert_eq!(ty_str(&mapper.map_type(&ty)?), "fn (Unknown) -> i32");
        Ok(())
    }

    // ---- Builder error ----

    #[test]
    fn builder_invalid_rust_type() {
        let result = TypeMapper::builder().map_path("foo", "not a {{ valid type");
        assert!(result.is_err());
    }

    // ---- Error diagnostics ----

    #[test]
    fn error_is_diagnostic() {
        let mapper = TypeMapper::new();
        let src = "auto";
        let ty = Type::Auto(TypeAuto {
            span: SourceSpan::new(src, 0, 4),
        });
        let err = mapper.map_type(&ty).unwrap_err();
        // TranspileError implements miette::Diagnostic
        let diagnostic: &dyn miette::Diagnostic = &err;
        assert!(diagnostic.source_code().is_some());
        assert!(diagnostic.labels().is_some());
    }

    // ---- ItemConst / ItemStatic transpilation ----

    #[test]
    fn const_int_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "const int MAX = 100;\nconst int MyClass::MIN = 10;";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Const(c) => {
                assert_eq!(
                    c.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    "# [doc = concat ! (\" Auto-transpiled const literal \" , stringify ! (i32))] pub const MAX : i32 = 100 ;"
                );
            }
            item => panic!("expected ItemConst, got {item:?}"),
        }
        match &file.items[1] {
            crate::ast::Item::Const(c) => {
                let class_path = c.class_path.as_ref().expect("expected class_path");
                assert_eq!(class_path.segments[0].ident.sym, "MyClass");
            }
            item => panic!("expected ItemConst, got {item:?}"),
        }
        Ok(())
    }

    #[test]
    fn constexpr_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "constexpr double PI = 3.14;";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Const(c) => {
                assert_eq!(
                    c.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    "# [doc = concat ! (\" Auto-transpiled const literal \" , stringify ! (f64))] pub const PI : f64 = 3.14 ;"
                );
            }
            item => panic!("expected ItemConst, got {item:?}"),
        }
        Ok(())
    }

    #[test]
    fn const_bool_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "const bool FLAG = true;";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Const(c) => {
                assert_eq!(
                    c.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    "# [doc = concat ! (\" Auto-transpiled const \" , stringify ! (bool))] pub const FLAG : bool = true ;"
                );
            }
            item => panic!("expected ItemConst, got {item:?}"),
        }
        Ok(())
    }

    #[test]
    fn const_char_literal_casts() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "
            constexpr char CONST_CHAR_VALUE = 'W';
            constexpr CustomType CONST_CUSTOM_VALUE = 'W';";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Const(c) => {
                assert_eq!(
                    c.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    "# [doc = concat ! (\" Auto-transpiled const literal \" , stringify ! (char))] pub const CONST_CHAR_VALUE : char = 'W' ;"
                );
            }
            item => panic!("expected ItemConst, got {item:?}"),
        }
        match &file.items[1] {
            crate::ast::Item::Const(c) => {
                assert_eq!(
                    c.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    "# [doc = concat ! (\" Auto-transpiled const literal \" , stringify ! (CustomType))] pub const CONST_CUSTOM_VALUE : CustomType = 'W' as CustomType ;"
                );
            }
            item => panic!("expected ItemConst, got {item:?}"),
        }
        Ok(())
    }

    #[test]
    fn const_string_uses_str_ref() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = r#"constexpr string WRONG_RETURN_CODE = "404";"#;
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Const(c) => {
                assert_eq!(
                    c.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    r#"# [doc = " Auto-transpiled &str const"] pub const WRONG_RETURN_CODE : & str = "404" ;"#
                );
            }
            item => panic!("expected ItemConst, got {item:?}"),
        }
        Ok(())
    }

    #[test]
    fn static_int_transpiles() -> Result<(), TranspileError> {
        let transpiler = Transpiler::default();
        let src = "static int count = 0;\nstatic int MyClass::instance = 42;";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Static(s) => {
                assert_eq!(
                    s.transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                        .to_string(),
                    "# [doc = concat ! (\" Auto-transpiled static literal \" , stringify ! (i32))] pub static count : i32 = 0 ;"
                );
                assert!(s.class_path.is_none());
            }
            item => panic!("expected ItemStatic, got {item:?}"),
        }
        match &file.items[1] {
            crate::ast::Item::Static(s) => {
                let class_path = s.class_path.as_ref().expect("expected class_path");
                assert_eq!(class_path.segments[0].ident.sym, "MyClass");
                assert_eq!(s.ident.sym, "instance");
            }
            item => panic!("expected ItemStatic, got {item:?}"),
        }
        Ok(())
    }

    #[test]
    fn char_array_from_string_literal() -> Result<(), TranspileError> {
        // `const char` is parsed as Const with a const-qualified element type.
        // The transpiler should infer the size (4 chars + null terminator = 5).
        let transpiler = Transpiler::default();
        let src = r#"const char listOfChars[] = "ALPN";"#;
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Const(c) => {
                let out = c
                    .transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                    .to_string();
                assert_eq!(out, r#"pub const listOfChars : & str = "ALPN" ;"#);
            }
            item => panic!("expected ItemConst, got {item:?}"),
        }

        Ok(())
    }

    #[test]
    fn char_array_with_escape_sequence() -> Result<(), TranspileError> {
        // `\n` counts as one character: size = 3 + 1 = 4.
        let transpiler = Transpiler::default();
        let src = r#"const char nl[] = "a\nb";"#;
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Const(c) => {
                let out = c
                    .transpile_token_stream(&transpiler, &mut TranspileContext::default())?
                    .to_string();
                assert_eq!(out, r#"pub const nl : & str = "a\nb" ;"#);
            }
            item => panic!("expected ItemConst, got {item:?}"),
        }

        Ok(())
    }

    #[test]
    fn static_no_init_errors() {
        let transpiler = Transpiler::default();
        let src = "static int count;";
        let file = parse_file(src).unwrap();
        match &file.items[0] {
            crate::ast::Item::Static(s) => {
                assert!(
                    s.transpile_token_stream(&transpiler, &mut TranspileContext::default())
                        .is_err()
                );
            }
            item => panic!("expected ItemStatic, got {item:?}"),
        }
    }
}
