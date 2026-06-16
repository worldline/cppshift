//! Type AST nodes for C++20
//!
//! Analogous to `syn::Type`.

use crate::{SourceCodeSpan, SourceSpan, source_code_span_impl};

use super::expr::Expr;
use super::item::Path;
use super::punct::Punctuated;

/// The kind of a fundamental (built-in) type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FundamentalKind {
    Void,
    Bool,
    Char,
    Char8,
    Char16,
    Char32,
    Wchar,
    Short,
    Int,
    Long,
    LongLong,
    Float,
    Double,
    LongDouble,
    SignedChar,
    UnsignedChar,
    UnsignedShort,
    UnsignedInt,
    UnsignedLong,
    UnsignedLongLong,
}

/// CV (const/volatile) qualifiers.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CvQualifiers {
    pub const_token: bool,
    pub volatile_token: bool,
}

/// A C++ type, analogous to `syn::Type`.
#[derive(Debug, Clone, PartialEq)]
pub enum Type<'de> {
    /// Fundamental type: `int`, `double`, `void`, etc.
    Fundamental(TypeFundamental<'de>),
    /// Named type: `MyClass`, `std::string`
    Path(TypePath<'de>),
    /// Pointer: `T*`
    Ptr(TypePtr<'de>),
    /// Lvalue reference: `T&`
    Reference(TypeReference<'de>),
    /// Rvalue reference: `T&&`
    RvalueReference(TypeRvalueReference<'de>),
    /// Array: `T[N]`
    Array(TypeArray<'de>),
    /// Function pointer: `int(*)(int, int)`
    FnPtr(TypeFnPtr<'de>),
    /// `auto`
    Auto(TypeAuto<'de>),
    /// `decltype(expr)`
    Decltype(TypeDecltype<'de>),
    /// Template instantiation: `vector<int>`, `map<string, int>`
    TemplateInst(TypeTemplateInst<'de>),
    /// CV-qualified type wrapper
    Qualified(TypeQualified<'de>),
}

impl<'de> Type<'de> {
    /// Check if the type is `auto`.
    pub fn is_auto(&self) -> bool {
        match self {
            Type::Auto(_) => true,
            Type::Qualified(q) => q.ty.is_auto(),
            _ => false,
        }
    }

    /// Remove reference qualifiers from the type, if any.
    pub fn remove_ref(&mut self) {
        match self {
            Type::Reference(r) => {
                *self = *r.referent.clone();
            }
            Type::RvalueReference(r) => {
                *self = *r.referent.clone();
            }
            Type::Qualified(q) => q.ty.remove_ref(),
            _ => {}
        }
    }
}

impl<'de> SourceCodeSpan<'de> for Type<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match self {
            Type::Fundamental(f) => Some(f.span),
            Type::Path(p) => p.span(),
            Type::Auto(a) => Some(a.span),
            Type::Decltype(d) => d.span(),
            Type::Ptr(p) => p.span(),
            Type::Reference(r) => r.span(),
            Type::RvalueReference(r) => r.span(),
            Type::Array(a) => a.span(),
            Type::FnPtr(f) => f.span(),
            Type::Qualified(q) => q.span(),
            Type::TemplateInst(t) => t.span(),
        }
    }
}

/// A fundamental (built-in) type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeFundamental<'de> {
    pub span: SourceSpan<'de>,
    pub kind: FundamentalKind,
}

source_code_span_impl!(TypeFundamental, Some, span);

/// A named type via a path: `MyClass`, `std::string`.
///
/// Analogous to `syn::TypePath`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypePath<'de> {
    pub path: Path<'de>,
}

source_code_span_impl!(TypePath, path);

/// Pointer type: `T*`.
///
/// Analogous to `syn::TypePtr`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypePtr<'de> {
    pub cv: CvQualifiers,
    pub pointee: Box<Type<'de>>,
}

source_code_span_impl!(TypePtr, pointee);

/// Lvalue reference type: `T&`.
///
/// Analogous to `syn::TypeReference`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeReference<'de> {
    pub cv: CvQualifiers,
    pub referent: Box<Type<'de>>,
}

source_code_span_impl!(TypeReference, referent);

/// Rvalue reference type: `T&&` (C++ specific).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRvalueReference<'de> {
    pub referent: Box<Type<'de>>,
}

source_code_span_impl!(TypeRvalueReference, referent);

/// Array type: `T[N]`.
///
/// Analogous to `syn::TypeArray`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeArray<'de> {
    pub element: Box<Type<'de>>,
    pub size: Option<Expr<'de>>,
}

source_code_span_impl!(TypeArray, and_then, size, element);

/// Function pointer type: `int(*)(int, int)`.
///
/// Analogous to `syn::TypeBareFn`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeFnPtr<'de> {
    pub return_type: Box<Type<'de>>,
    pub params: Punctuated<'de, Type<'de>>,
}

source_code_span_impl!(TypeFnPtr, return_type, params);

/// `auto` type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeAuto<'de> {
    pub span: SourceSpan<'de>,
}

source_code_span_impl!(TypeAuto, Some, span);

/// `decltype(expr)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecltype<'de> {
    pub expr: Expr<'de>,
}

source_code_span_impl!(TypeDecltype, expr);

/// Template instantiation type: `vector<int>`, `map<string, int>`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeTemplateInst<'de> {
    pub path: Path<'de>,
    pub args: Vec<TemplateArg<'de>>,
}

source_code_span_impl!(TypeTemplateInst, path, args);

/// A template argument (type or expression).
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateArg<'de> {
    Type(Type<'de>),
    Expr(Expr<'de>),
}

impl<'de> SourceCodeSpan<'de> for TemplateArg<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match &self {
            TemplateArg::Type(ty) => ty.span(),
            TemplateArg::Expr(expr) => expr.span(),
        }
    }
}

/// A CV-qualified type: `const T`, `volatile T`, `const volatile T`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeQualified<'de> {
    pub cv: CvQualifiers,
    pub ty: Box<Type<'de>>,
}

source_code_span_impl!(TypeQualified, ty);

/// A template argument for use in paths.
#[derive(Debug, Clone, PartialEq)]
pub struct AngleBracketedArgs<'de> {
    pub args: Vec<TemplateArg<'de>>,
}

source_code_span_impl!(AngleBracketedArgs, args);
