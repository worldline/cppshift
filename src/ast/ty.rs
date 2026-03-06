//! Type AST nodes for C++20
//!
//! Analogous to `syn::Type`.

use crate::SourceSpan;

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

/// A fundamental (built-in) type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeFundamental<'de> {
    pub span: SourceSpan<'de>,
    pub kind: FundamentalKind,
}

/// A named type via a path: `MyClass`, `std::string`.
///
/// Analogous to `syn::TypePath`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypePath<'de> {
    pub path: Path<'de>,
}

/// Pointer type: `T*`.
///
/// Analogous to `syn::TypePtr`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypePtr<'de> {
    pub cv: CvQualifiers,
    pub pointee: Box<Type<'de>>,
}

/// Lvalue reference type: `T&`.
///
/// Analogous to `syn::TypeReference`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeReference<'de> {
    pub cv: CvQualifiers,
    pub referent: Box<Type<'de>>,
}

/// Rvalue reference type: `T&&` (C++ specific).
#[derive(Debug, Clone, PartialEq)]
pub struct TypeRvalueReference<'de> {
    pub referent: Box<Type<'de>>,
}

/// Array type: `T[N]`.
///
/// Analogous to `syn::TypeArray`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeArray<'de> {
    pub element: Box<Type<'de>>,
    pub size: Option<Expr<'de>>,
}

/// Function pointer type: `int(*)(int, int)`.
///
/// Analogous to `syn::TypeBareFn`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeFnPtr<'de> {
    pub return_type: Box<Type<'de>>,
    pub params: Punctuated<'de, Type<'de>>,
}

/// `auto` type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TypeAuto<'de> {
    pub span: SourceSpan<'de>,
}

/// `decltype(expr)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecltype<'de> {
    pub expr: Expr<'de>,
}

/// Template instantiation type: `vector<int>`, `map<string, int>`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeTemplateInst<'de> {
    pub path: Path<'de>,
    pub args: Vec<TemplateArg<'de>>,
}

/// A template argument (type or expression).
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateArg<'de> {
    Type(Type<'de>),
    Expr(Expr<'de>),
}

/// A CV-qualified type: `const T`, `volatile T`, `const volatile T`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeQualified<'de> {
    pub cv: CvQualifiers,
    pub ty: Box<Type<'de>>,
}

/// A template argument for use in paths.
#[derive(Debug, Clone, PartialEq)]
pub struct AngleBracketedArgs<'de> {
    pub args: Vec<TemplateArg<'de>>,
}
