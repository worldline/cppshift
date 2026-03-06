use std::collections::HashMap;

use crate::ast::expr::{Expr, LitKind};
use crate::ast::item::Path;
use crate::ast::ty::{FundamentalKind, TemplateArg, Type};

use super::error::TranspileError;

impl From<FundamentalKind> for syn::Type {
    fn from(kind: FundamentalKind) -> Self {
        use FundamentalKind::*;
        let s = match kind {
            Void => "()",
            Bool => "bool",
            Char | Char8 | UnsignedChar => "u8",
            Char16 | UnsignedShort => "u16",
            Char32 | Wchar | UnsignedInt => "u32",
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
                        let n: usize = lit.span.src().parse().map_err(|_| {
                            TranspileError::UnsupportedType {
                                message: format!(
                                    "invalid array size literal `{}`",
                                    lit.span.src()
                                ),
                                src: lit.span.full_source().to_owned(),
                                err_span: lit.span.into(),
                            }
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
                let path_str = path_to_string(&t.path);
                let base_ty =
                    self.paths
                        .get(&path_str)
                        .cloned()
                        .ok_or_else(|| unmapped_path_error(&path_str, &t.path))?;
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
                    last_seg.arguments = syn::PathArguments::AngleBracketed(
                        syn::AngleBracketedGenericArguments {
                            colon2_token: None,
                            lt_token: syn::token::Lt::default(),
                            args: mapped_args
                                .into_iter()
                                .map(syn::GenericArgument::Type)
                                .collect(),
                            gt_token: syn::token::Gt::default(),
                        },
                    );
                }
                Ok(result)
            }
            Type::Auto(a) => Err(TranspileError::UnsupportedType {
                message: "auto type cannot be mapped to Rust".to_owned(),
                src: a.span.full_source().to_owned(),
                err_span: a.span.into(),
            }),
            Type::Decltype(_) => Err(unsupported_from_type(
                "decltype cannot be mapped to Rust",
                ty,
            )),
        }
    }

    fn resolve_path(&self, path: &Path<'_>) -> Result<syn::Type, TranspileError> {
        let key = path_to_string(path);
        self.paths
            .get(&key)
            .cloned()
            .ok_or_else(|| unmapped_path_error(&key, path))
    }
}

impl Default for TypeMapper {
    fn default() -> Self {
        Self::new()
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

fn path_to_string(path: &Path<'_>) -> String {
    path.segments
        .iter()
        .map(|s| s.ident.sym)
        .collect::<Vec<_>>()
        .join("::")
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
    match type_span(ty) {
        Some(span) => TranspileError::UnsupportedType {
            message: message.to_owned(),
            src: span.full_source().to_owned(),
            err_span: span.into(),
        },
        None => TranspileError::UnsupportedType {
            message: message.to_owned(),
            src: String::new(),
            err_span: miette::SourceSpan::new(0.into(), 0),
        },
    }
}

/// Extract the most relevant source span from a type, if available.
fn type_span<'de>(ty: &Type<'de>) -> Option<crate::SourceSpan<'de>> {
    match ty {
        Type::Fundamental(f) => Some(f.span),
        Type::Path(p) => p.path.segments.first().map(|s| s.ident.span),
        Type::Auto(a) => Some(a.span),
        Type::Decltype(d) => expr_span(&d.expr),
        Type::Ptr(p) => type_span(&p.pointee),
        Type::Reference(r) => type_span(&r.referent),
        Type::RvalueReference(r) => type_span(&r.referent),
        Type::Array(a) => type_span(&a.element),
        Type::FnPtr(f) => type_span(&f.return_type),
        Type::Qualified(q) => type_span(&q.ty),
        Type::TemplateInst(t) => t.path.segments.first().map(|s| s.ident.span),
    }
}

/// Extract a source span from an expression (best-effort).
fn expr_span<'de>(expr: &Expr<'de>) -> Option<crate::SourceSpan<'de>> {
    match expr {
        Expr::Lit(l) => Some(l.span),
        Expr::Ident(i) => Some(i.ident.span),
        Expr::Path(p) => p.path.segments.first().map(|s| s.ident.span),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    use crate::ast::expr::ExprLit;
    use crate::ast::item::{Ident, PathSegment};
    use crate::ast::punct::Punctuated;
    use crate::ast::ty::*;
    use crate::SourceSpan;

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

    // ---- Fundamental types (table-driven) ----

    #[test]
    fn fundamental_defaults() -> Result<(), TranspileError> {
        use FundamentalKind::*;
        let mapper = TypeMapper::new();
        let cases: &[(FundamentalKind, &str, &str)] = &[
            (Void, "void", "()"),
            (Bool, "bool", "bool"),
            (Char, "char", "u8"),
            (Char8, "char8_t", "u8"),
            (Char16, "char16_t", "u16"),
            (Char32, "char32_t", "u32"),
            (Wchar, "wchar_t", "u32"),
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
    fn path_unknown_returns_err() {
        let mapper = TypeMapper::new();
        let src = "Unknown";
        let ty = make_path(src, &[src]);
        assert!(mapper.map_type(&ty).is_err());
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
    fn template_inst_unknown_path() {
        let mapper = TypeMapper::new();
        let path_src = "std::deque";
        let inner_src = "int";
        let inner = make_fundamental(inner_src, FundamentalKind::Int);
        let ty = Type::TemplateInst(TypeTemplateInst {
            path: make_raw_path(path_src, &[&path_src[..3], &path_src[5..]]),
            args: vec![TemplateArg::Type(inner)],
        });
        assert!(mapper.map_type(&ty).is_err());
    }

    #[test]
    fn template_inst_unmappable_arg() -> Result<(), TranspileError> {
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
        assert!(mapper.map_type(&ty).is_err());
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
    fn ptr_unknown_inner_returns_err() {
        let mapper = TypeMapper::new();
        let src = "Unknown";
        let inner = make_path(src, &[src]);
        let ty = Type::Ptr(TypePtr {
            cv: CvQualifiers::default(),
            pointee: Box::new(inner),
        });
        assert!(mapper.map_type(&ty).is_err());
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
    fn fn_ptr_unmappable_param() {
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
        assert!(mapper.map_type(&ty).is_err());
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
        let src = "Unknown";
        let ty = make_path(src, &[src]);
        let err = mapper.map_type(&ty).unwrap_err();
        // TranspileError implements miette::Diagnostic
        let diagnostic: &dyn miette::Diagnostic = &err;
        assert!(diagnostic.source_code().is_some());
        assert!(diagnostic.labels().is_some());
    }
}
