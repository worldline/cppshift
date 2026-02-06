//! Item types for C++20 top-level declarations
//!
//! Each variant of [`Item`] corresponds to a top-level declaration in a C++ translation unit,
//! following the naming conventions of `syn::Item`.

use crate::SourceSpan;
use crate::lex::Token;

use super::expr::Expr;
use super::punct::Punctuated;
use super::stmt::Block;
use super::ty::Type;

// ---------------------------------------------------------------------------
// Core support types
// ---------------------------------------------------------------------------

/// An identifier with its source span, analogous to `syn::Ident`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ident<'de> {
    pub sym: &'de str,
    pub span: SourceSpan<'de>,
}

/// Visibility of a declaration.
///
/// In C++, visibility applies within class/struct bodies via access specifiers.
/// At namespace scope, everything is effectively public.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Visibility {
    Public,
    Protected,
    Private,
    /// No explicit access specifier (default for context)
    #[default]
    Inherited,
}

/// A C++20 attribute `[[...]]`, analogous to `syn::Attribute`.
#[derive(Debug, Clone, PartialEq)]
pub struct Attribute<'de> {
    pub span: SourceSpan<'de>,
    pub path: Path<'de>,
    pub args: Vec<Token<'de>>,
}

/// A qualified path like `std::vector` or `::global::Foo`.
///
/// Analogous to `syn::Path`.
#[derive(Debug, Clone, PartialEq)]
pub struct Path<'de> {
    pub leading_colon: bool,
    pub segments: Vec<PathSegment<'de>>,
}

/// A single segment of a path, analogous to `syn::PathSegment`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathSegment<'de> {
    pub ident: Ident<'de>,
}

/// A base class specifier in a class/struct definition.
///
/// Example: `public Base`, `virtual protected Interface`
#[derive(Debug, Clone, PartialEq)]
pub struct BaseSpecifier<'de> {
    pub access: Visibility,
    pub virtual_token: bool,
    pub path: Path<'de>,
}

/// A field (data member) in a struct, class, or union.
///
/// Analogous to `syn::Field`.
#[derive(Debug, Clone, PartialEq)]
pub struct Field<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub vis: Visibility,
    pub ty: Type<'de>,
    pub ident: Option<Ident<'de>>,
    pub default_value: Option<Expr<'de>>,
}

/// Fields of a struct, class, or union.
///
/// Analogous to `syn::Fields`.
#[derive(Debug, Clone, PartialEq)]
pub enum Fields<'de> {
    /// Named fields with access specifier grouping: `{ public: int x; private: int y; }`
    Named(FieldsNamed<'de>),
    /// Forward declaration (no body): `struct Foo;`
    Unit,
}

/// Named fields grouped by access specifier.
///
/// Analogous to `syn::FieldsNamed`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldsNamed<'de> {
    pub members: Vec<Member<'de>>,
}

/// A member inside a class/struct body.
///
/// Can be a field, a method, a nested type, an access specifier, etc.
#[derive(Debug, Clone, PartialEq)]
pub enum Member<'de> {
    /// Access specifier: `public:`, `private:`, `protected:`
    AccessSpecifier(Visibility),
    /// Data member (field)
    Field(Field<'de>),
    /// Member function (method)
    Method(ItemFn<'de>),
    /// Constructor
    Constructor(ItemConstructor<'de>),
    /// Destructor
    Destructor(ItemDestructor<'de>),
    /// Nested type (class, struct, enum, etc.)
    Item(Box<Item<'de>>),
    /// Friend declaration
    Friend(ItemFriend<'de>),
    /// Using declaration inside a class
    Using(ItemUse<'de>),
    /// Static assertion
    StaticAssert(ItemStaticAssert<'de>),
}

/// A function argument, analogous to `syn::FnArg`.
#[derive(Debug, Clone, PartialEq)]
pub struct FnArg<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub ty: Type<'de>,
    pub ident: Option<Ident<'de>>,
    pub default_value: Option<Expr<'de>>,
}

/// A function signature, analogous to `syn::Signature`.
#[derive(Debug, Clone, PartialEq)]
pub struct Signature<'de> {
    pub constexpr_token: bool,
    pub consteval_token: bool,
    pub inline_token: bool,
    pub virtual_token: bool,
    pub static_token: bool,
    pub explicit_token: bool,
    pub return_type: Type<'de>,
    pub ident: Ident<'de>,
    pub inputs: Punctuated<'de, FnArg<'de>>,
    pub variadic: bool,
    // Trailing qualifiers
    pub const_token: bool,
    pub noexcept_token: bool,
    pub override_token: bool,
    pub final_token: bool,
    pub pure_virtual: bool,
    pub defaulted: bool,
    pub deleted: bool,
}

/// An enum variant, analogous to `syn::Variant`.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub ident: Ident<'de>,
    pub discriminant: Option<Expr<'de>>,
}

/// A template parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateParam<'de> {
    /// `typename T` or `class T`
    Type {
        ident: Option<Ident<'de>>,
        default: Option<Type<'de>>,
    },
    /// Non-type: `int N`
    NonType {
        ty: Type<'de>,
        ident: Option<Ident<'de>>,
        default: Option<Expr<'de>>,
    },
    /// Template template: `template<...> class C`
    Template {
        params: Vec<TemplateParam<'de>>,
        ident: Option<Ident<'de>>,
    },
    /// Parameter pack: `typename... Args`
    Pack { ident: Ident<'de> },
}

/// A foreign item inside an `extern "C"` block.
#[derive(Debug, Clone, PartialEq)]
pub enum ForeignItem<'de> {
    /// Function declaration
    Fn(ItemFn<'de>),
    /// Variable declaration
    Static(ItemStatic<'de>),
    /// Unparsed tokens
    Verbatim(ItemVerbatim<'de>),
}

/// A member initializer in a constructor initializer list.
///
/// Example: `m_x(x)`, `Base(arg)`
#[derive(Debug, Clone, PartialEq)]
pub struct MemberInit<'de> {
    pub member: Ident<'de>,
    pub args: Punctuated<'de, Expr<'de>>,
}

// ---------------------------------------------------------------------------
// Item enum and variants
// ---------------------------------------------------------------------------

/// A top-level item (declaration) in a C++ translation unit.
///
/// Analogous to `syn::Item`. Each variant corresponds to a kind of
/// declaration that can appear at file scope or namespace scope.
#[derive(Debug, Clone, PartialEq)]
pub enum Item<'de> {
    /// Function declaration or definition: `int foo() { ... }`
    Fn(ItemFn<'de>),
    /// Struct definition: `struct Foo { ... };`
    Struct(ItemStruct<'de>),
    /// Class definition: `class Foo { ... };`
    Class(ItemClass<'de>),
    /// Enum definition: `enum Color { Red, Green, Blue };`
    Enum(ItemEnum<'de>),
    /// Union definition: `union Data { int i; float f; };`
    Union(ItemUnion<'de>),
    /// Namespace: `namespace foo { ... }`
    Namespace(ItemNamespace<'de>),
    /// Using declaration/directive/alias: `using std::cout;`
    Use(ItemUse<'de>),
    /// Type alias: `using size_t = unsigned long;`
    Type(ItemType<'de>),
    /// Typedef: `typedef unsigned long size_t;`
    Typedef(ItemTypedef<'de>),
    /// Const/constexpr variable: `constexpr int MAX = 100;`
    Const(ItemConst<'de>),
    /// Static variable: `static int count;`
    Static(ItemStatic<'de>),
    /// Extern block: `extern "C" { ... }`
    ForeignMod(ItemForeignMod<'de>),
    /// Template declaration: `template<typename T> ...`
    Template(ItemTemplate<'de>),
    /// Static assertion: `static_assert(sizeof(int) == 4);`
    StaticAssert(ItemStaticAssert<'de>),
    /// Preprocessor directive: `#include <iostream>`
    Macro(ItemMacro<'de>),
    /// Tokens not interpreted by the parser
    Verbatim(ItemVerbatim<'de>),
}

/// A function declaration or definition, analogous to `syn::ItemFn`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemFn<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub vis: Visibility,
    pub sig: Signature<'de>,
    pub block: Option<Block<'de>>,
}

/// A struct definition, analogous to `syn::ItemStruct`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStruct<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub ident: Option<Ident<'de>>,
    pub generics: Option<Generics<'de>>,
    pub bases: Vec<BaseSpecifier<'de>>,
    pub fields: Fields<'de>,
}

/// A class definition (C++ specific).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemClass<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub ident: Option<Ident<'de>>,
    pub generics: Option<Generics<'de>>,
    pub bases: Vec<BaseSpecifier<'de>>,
    pub fields: Fields<'de>,
}

/// An enum definition, analogous to `syn::ItemEnum`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemEnum<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub ident: Option<Ident<'de>>,
    pub scoped: bool,
    pub underlying_type: Option<Type<'de>>,
    pub variants: Punctuated<'de, Variant<'de>>,
}

/// A union definition, analogous to `syn::ItemUnion`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemUnion<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub ident: Option<Ident<'de>>,
    pub fields: FieldsNamed<'de>,
}

/// A namespace declaration, analogous to `syn::ItemMod`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemNamespace<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub inline_token: bool,
    pub ident: Option<Ident<'de>>,
    pub content: Vec<Item<'de>>,
}

/// A using declaration, directive, or alias, analogous to `syn::ItemUse`.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemUse<'de> {
    /// `using std::cout;`
    Declaration {
        attrs: Vec<Attribute<'de>>,
        name: Path<'de>,
    },
    /// `using namespace std;`
    Directive {
        attrs: Vec<Attribute<'de>>,
        namespace: Path<'de>,
    },
    /// `using size_t = unsigned long;`
    Alias {
        attrs: Vec<Attribute<'de>>,
        ident: Ident<'de>,
        ty: Type<'de>,
    },
}

/// A type alias (`using X = Y`), analogous to `syn::ItemType`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemType<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub ident: Ident<'de>,
    pub generics: Option<Generics<'de>>,
    pub ty: Type<'de>,
}

/// A typedef declaration (C-style type alias).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemTypedef<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub ty: Type<'de>,
    pub ident: Ident<'de>,
}

/// A const or constexpr variable, analogous to `syn::ItemConst`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemConst<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub constexpr_token: bool,
    pub ty: Type<'de>,
    pub ident: Ident<'de>,
    pub expr: Expr<'de>,
}

/// A static variable, analogous to `syn::ItemStatic`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStatic<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub ty: Type<'de>,
    pub ident: Ident<'de>,
    pub expr: Option<Expr<'de>>,
}

/// An extern block (`extern "C" { ... }`), analogous to `syn::ItemForeignMod`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemForeignMod<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub abi: &'de str,
    pub items: Vec<ForeignItem<'de>>,
}

/// A template declaration (C++ specific).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemTemplate<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub params: Punctuated<'de, TemplateParam<'de>>,
    pub item: Box<Item<'de>>,
}

/// A static assertion (C++ specific).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStaticAssert<'de> {
    pub expr: Expr<'de>,
    pub message: Option<Expr<'de>>,
}

/// A preprocessor directive.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemMacro<'de> {
    pub span: SourceSpan<'de>,
    pub tokens: Vec<Token<'de>>,
}

/// Tokens not interpreted by the parser, analogous to `syn::Item::Verbatim`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemVerbatim<'de> {
    pub tokens: Vec<Token<'de>>,
}

/// A constructor declaration/definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemConstructor<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub explicit_token: bool,
    pub constexpr_token: bool,
    pub ident: Ident<'de>,
    pub inputs: Punctuated<'de, FnArg<'de>>,
    pub noexcept_token: bool,
    pub member_init_list: Vec<MemberInit<'de>>,
    pub block: Option<Block<'de>>,
    pub defaulted: bool,
    pub deleted: bool,
}

/// A destructor declaration/definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDestructor<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub virtual_token: bool,
    pub ident: Ident<'de>,
    pub noexcept_token: bool,
    pub block: Option<Block<'de>>,
    pub defaulted: bool,
    pub deleted: bool,
    pub pure_virtual: bool,
}

/// A friend declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemFriend<'de> {
    pub attrs: Vec<Attribute<'de>>,
    pub item: Box<Item<'de>>,
}

/// Template generics on a class/struct/function.
#[derive(Debug, Clone, PartialEq)]
pub struct Generics<'de> {
    pub params: Punctuated<'de, TemplateParam<'de>>,
}
