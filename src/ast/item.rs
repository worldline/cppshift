//! Item types for C++20 top-level declarations
//!
//! Each variant of [`Item`] corresponds to a top-level declaration in a C++ translation unit,
//! following the naming conventions of `syn::Item`.

use std::cmp::Ordering;
use std::collections::LinkedList;
use std::fmt;
use std::hash::{Hash, Hasher};

use crate::lex::Token;
use crate::{SourceCodeSpan, SourceSpan, source_code_span_impl};

use super::expr::Expr;
use super::punct::Punctuated;
use super::stmt::Block;
use super::ty::Type;

// ---------------------------------------------------------------------------
// Core support types
// ---------------------------------------------------------------------------

/// An identifier with its source span, analogous to `syn::Ident`.
#[derive(Debug, Clone, Copy, Eq)]
pub struct Ident<'de> {
    pub sym: &'de str,
    pub span: SourceSpan<'de>,
}

source_code_span_impl!(Ident, Some, span);

impl<'de> PartialEq for Ident<'de> {
    fn eq(&self, ident: &Ident<'de>) -> bool {
        self.sym == ident.sym
    }
}

impl<'de> PartialEq<str> for Ident<'de> {
    fn eq(&self, sym: &str) -> bool {
        self.sym == sym
    }
}

impl<'de> PartialEq<Ident<'de>> for str {
    fn eq(&self, ident: &Ident<'de>) -> bool {
        self == ident.sym
    }
}

impl<'de> PartialEq<&str> for Ident<'de> {
    fn eq(&self, sym: &&str) -> bool {
        self.sym == *sym
    }
}

impl<'de> PartialEq<Ident<'de>> for &str {
    fn eq(&self, ident: &Ident<'de>) -> bool {
        *self == ident.sym
    }
}

impl<'de> PartialEq<String> for Ident<'de> {
    fn eq(&self, sym: &String) -> bool {
        self.sym == sym.as_str()
    }
}

impl<'de> PartialEq<Ident<'de>> for String {
    fn eq(&self, ident: &Ident<'de>) -> bool {
        self.as_str() == ident.sym
    }
}

impl<'de> Ord for Ident<'de> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sym.cmp(other.sym)
    }
}

impl<'de> PartialOrd for Ident<'de> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'de> Hash for Ident<'de> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.sym.hash(state);
    }
}

impl<'de> fmt::Display for Ident<'de> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.sym)
    }
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
    /// Source location of the entire `[[...]]`.
    pub span: SourceSpan<'de>,
    /// Attribute name/path: `nodiscard`, `gnu::unused`, etc.
    pub path: Path<'de>,
    /// Attribute arguments as raw tokens (e.g. the `"reason"` in `[[deprecated("reason")]]`).
    pub args: Vec<Token<'de>>,
}

source_code_span_impl!(Attribute, Some, span);

/// A qualified path like `std::vector` or `::global::Foo`.
///
/// Analogous to `syn::Path`.
#[derive(Debug, Clone, PartialEq)]
pub struct Path<'de> {
    /// `true` if the path starts with `::` (absolute/global scope).
    pub leading_colon: bool,
    /// The segments of the path: `std::vector` → `[std, vector]`.
    pub segments: Vec<PathSegment<'de>>,
}

source_code_span_impl!(Path, segments);

impl<'de> fmt::Display for Path<'de> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            self.segments
                .iter()
                .map(|s| s.ident.sym)
                .collect::<Vec<_>>()
                .join("::")
        )
    }
}

/// A single segment of a path, analogous to `syn::PathSegment`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathSegment<'de> {
    pub ident: Ident<'de>,
}

source_code_span_impl!(PathSegment, ident);

/// A base class specifier in a class/struct definition.
///
/// Example: `public Base`, `virtual protected Interface`
#[derive(Debug, Clone, PartialEq)]
pub struct BaseSpecifier<'de> {
    /// Access specifier for the inheritance: `public`, `protected`, or `private`.
    pub access: Visibility,
    /// `true` for virtual inheritance (diamond problem mitigation).
    pub virtual_token: bool,
    /// The base class name (possibly qualified: `std::Base`).
    pub path: Path<'de>,
}

source_code_span_impl!(BaseSpecifier, path);

/// A field (data member) in a struct, class, or union.
///
/// Analogous to `syn::Field`.
#[derive(Debug, Clone, PartialEq)]
pub struct Field<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// Access specifier (`public`, `private`, `protected`).
    pub vis: Visibility,
    /// `true` if declared `static`.
    pub static_token: bool,
    /// The field's type.
    pub ty: Type<'de>,
    /// Field name. `None` for anonymous fields (e.g. anonymous unions).
    pub ident: Option<Ident<'de>>,
    /// Default member initializer (C++11): `int x = 42;`.
    pub default_value: Option<Expr<'de>>,
}

source_code_span_impl!(Field, attrs, ty, and_then, ident, and_then, default_value);

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

impl<'de> SourceCodeSpan<'de> for Fields<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        if let Fields::Named(fields_named) = self {
            fields_named.span()
        } else {
            None
        }
    }
}

/// Named fields grouped by access specifier.
///
/// Analogous to `syn::FieldsNamed`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldsNamed<'de> {
    pub members: Vec<Member<'de>>,
}

source_code_span_impl!(FieldsNamed, members);

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

impl<'de> SourceCodeSpan<'de> for Member<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match self {
            Member::Field(field) => field.span(),
            Member::Method(item_fn) => item_fn.span(),
            Member::Constructor(item_constructor) => item_constructor.span(),
            Member::Destructor(item_destructor) => item_destructor.span(),
            Member::Item(item) => item.span(),
            Member::Friend(item_friend) => item_friend.span(),
            Member::Using(item_use) => item_use.span(),
            Member::StaticAssert(item_static_assert) => item_static_assert.span(),
            _ => None,
        }
    }
}

/// A function argument, analogous to `syn::FnArg`.
///
/// Example: `[[maybe_unused]] int count = 0`
#[derive(Debug, Clone, PartialEq)]
pub struct FnArg<'de> {
    /// C++20 attributes: `[[maybe_unused]]`, etc.
    pub attrs: Vec<Attribute<'de>>,
    /// The parameter's type.
    pub ty: Type<'de>,
    /// Parameter name. `None` for unnamed parameters (e.g. `void foo(int)`).
    pub ident: Option<Ident<'de>>,
    /// Default argument value: `int count = 0`.
    pub default_value: Option<Expr<'de>>,
}

source_code_span_impl!(FnArg, attrs, ty, and_then, ident, and_then, default_value);

/// A function signature, analogous to `syn::Signature`.
///
/// Contains all specifiers, qualifiers, and the parameter list.
///
/// Example: `constexpr inline virtual int compute(int x) const noexcept override`
#[derive(Debug, Clone, PartialEq)]
pub struct Signature<'de> {
    // --- Leading specifiers ---
    /// `true` if declared `constexpr`.
    pub constexpr_token: bool,
    /// `true` if declared `consteval` (C++20, immediate function).
    pub consteval_token: bool,
    /// `true` if declared `inline`.
    pub inline_token: bool,
    /// `true` if declared `virtual`.
    pub virtual_token: bool,
    /// `true` if declared `static`.
    pub static_token: bool,
    /// `true` if declared `explicit` (for conversion operators).
    pub explicit_token: bool,
    /// The return type (e.g. `int`, `void`, `auto`).
    pub return_type: Type<'de>,
    /// Optional qualifying class/namespace path for out-of-line definitions.
    /// For `void MyClass::myFunction()`, this is `MyClass`.
    pub class_path: Option<Path<'de>>,
    /// The function name.
    pub ident: Ident<'de>,
    /// Function parameters, comma-separated.
    pub inputs: Punctuated<'de, FnArg<'de>>,
    /// `true` if the function accepts variadic arguments (`...`).
    pub variadic: bool,
    // --- Trailing qualifiers ---
    /// `true` if the member function is `const`-qualified.
    pub const_token: bool,
    /// `true` if declared `noexcept`.
    pub noexcept_token: bool,
    /// `true` if declared `override` (virtual method override).
    pub override_token: bool,
    /// `true` if declared `final` (prevents further overriding).
    pub final_token: bool,
    /// `true` if `= 0` (pure virtual / abstract method).
    pub pure_virtual: bool,
    /// `true` if `= default` (compiler-generated implementation).
    pub defaulted: bool,
    /// `true` if `= delete` (explicitly deleted function).
    pub deleted: bool,
    /// `true` if this is a destructor (`~ClassName`).
    pub destructor_token: bool,
    /// Member initializer list for out-of-line constructors: `: m_x(x), m_y(y)`.
    pub member_init_list: Vec<MemberInit<'de>>,
}

source_code_span_impl!(
    Signature,
    return_type,
    and_then,
    class_path,
    ident,
    inputs,
    member_init_list
);

impl<'de> Signature<'de> {
    /// Returns true if this is a class constructor.
    pub fn is_class_constructor(&self) -> bool {
        !self.destructor_token
            && self
                .class_path
                .as_ref()
                .is_some_and(|cp| self.ident == cp.to_string())
    }

    /// Returns true if this is a class destructor.
    pub fn is_class_destructor(&self) -> bool {
        self.destructor_token
    }

    /// Checks if this function can be called with no arguments
    /// (either no parameters or all parameters have defaults).
    pub fn has_no_required_params(&self) -> bool {
        if let Some((fn_arg, _)) = self.inputs.inner.first()
            && fn_arg.default_value.is_none()
        {
            false
        } else {
            !self.deleted
        }
    }
}

/// An enum variant, analogous to `syn::Variant`.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// The variant name (e.g. `Red`).
    pub ident: Ident<'de>,
    /// Explicit discriminant value: `Red = 1`.
    pub discriminant: Option<Expr<'de>>,
}

source_code_span_impl!(Variant, attrs, ident, and_then, discriminant);

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

impl<'de> SourceCodeSpan<'de> for TemplateParam<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match self {
            TemplateParam::Type { ident, default } => {
                if let Some(ident_span) = ident.as_ref().map(|i| i.span) {
                    if let Some(default_span) = default.as_ref().and_then(|d| d.span()) {
                        Some(ident_span.extend(default_span))
                    } else {
                        Some(ident_span)
                    }
                } else {
                    default.as_ref().and_then(|d| d.span())
                }
            }
            TemplateParam::NonType { ty, ident, default } => {
                let mut ret_span = ty.span();

                if let Some(ident_span) = ident.as_ref().map(|i| i.span) {
                    ret_span = if let Some(span) = ret_span {
                        Some(span.extend(ident_span))
                    } else {
                        Some(ident_span)
                    }
                }

                if let Some(default_span) = default.as_ref().and_then(|d| d.span()) {
                    if let Some(span) = ret_span {
                        Some(span.extend(default_span))
                    } else {
                        Some(default_span)
                    }
                } else {
                    ret_span
                }
            }
            TemplateParam::Template { params, ident } => {
                if let Some(ident_span) = ident.as_ref().map(|i| i.span) {
                    if let Some(params_span) = params.span() {
                        Some(ident_span.extend(params_span))
                    } else {
                        Some(ident_span)
                    }
                } else {
                    params.span()
                }
            }
            TemplateParam::Pack { ident } => Some(ident.span),
        }
    }
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

impl<'de> SourceCodeSpan<'de> for ForeignItem<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match self {
            ForeignItem::Fn(item_fn) => item_fn.span(),
            ForeignItem::Static(item_static) => item_static.span(),
            ForeignItem::Verbatim(item_verbatim) => item_verbatim.span(),
        }
    }
}

/// A member initializer in a constructor initializer list.
///
/// Example: `m_x(x)`, `Base(arg)`, `::std::runtime_error(msg)`
#[derive(Debug, Clone, PartialEq)]
pub struct MemberInit<'de> {
    /// The member or base class being initialized (may be a qualified path).
    pub member: Path<'de>,
    /// The initializer arguments: `m_x(x)` → args is `[x]`.
    pub args: Punctuated<'de, Expr<'de>>,
}

source_code_span_impl!(MemberInit, member, args);

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
    /// Variable declaration: `int x = 42;`
    Var(ItemVar<'de>),
    /// Extern block: `extern "C" { ... }`
    ForeignMod(ItemForeignMod<'de>),
    /// Template declaration: `template<typename T> ...`
    Template(ItemTemplate<'de>),
    /// Static assertion: `static_assert(sizeof(int) == 4);`
    StaticAssert(ItemStaticAssert<'de>),
    /// Include directive: `#include <iostream>` or `#include "myfile.h"`
    Include(ItemInclude<'de>),
    /// Preprocessor directive: `#include <iostream>`
    Macro(ItemMacro<'de>),
    /// Tokens not interpreted by the parser
    Verbatim(ItemVerbatim<'de>),
}

impl<'de> SourceCodeSpan<'de> for Item<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match self {
            Item::Fn(item_fn) => item_fn.span(),
            Item::Struct(item_struct) => item_struct.span(),
            Item::Class(item_class) => item_class.span(),
            Item::Enum(item_enum) => item_enum.span(),
            Item::Union(item_union) => item_union.span(),
            Item::Namespace(item_namespace) => item_namespace.span(),
            Item::Use(item_use) => item_use.span(),
            Item::Type(item_type) => item_type.span(),
            Item::Typedef(item_typedef) => item_typedef.span(),
            Item::Const(item_const) => item_const.span(),
            Item::Static(item_static) => item_static.span(),
            Item::Var(item_var) => item_var.span(),
            Item::ForeignMod(item_foreign_mod) => item_foreign_mod.span(),
            Item::Template(item_template) => item_template.span(),
            Item::StaticAssert(item_static_assert) => item_static_assert.span(),
            Item::Include(item_include) => item_include.span(),
            Item::Macro(item_macro) => item_macro.span(),
            Item::Verbatim(item_verbatim) => item_verbatim.span(),
        }
    }
}

/// A function declaration or definition, analogous to `syn::ItemFn`.
///
/// Example: `[[nodiscard]] int add(int a, int b) { return a + b; }`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemFn<'de> {
    /// C++20 attributes: `[[nodiscard]]`, `[[deprecated]]`, etc.
    pub attrs: Vec<Attribute<'de>>,
    /// Access specifier when this function is a class member.
    pub vis: Visibility,
    /// Function signature: name, return type, parameters, and qualifiers.
    pub sig: Signature<'de>,
    /// Function body. `None` for declarations (`;`), `Some` for definitions (`{ ... }`).
    pub block: Option<Block<'de>>,
}

source_code_span_impl!(ItemFn, attrs, sig, and_then, block);

/// A struct definition, analogous to `syn::ItemStruct`.
///
/// Example: `struct Point : public Base { int x; int y; };`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStruct<'de> {
    /// C++20 attributes: `[[deprecated]]`, `[[nodiscard]]`, etc.
    pub attrs: Vec<Attribute<'de>>,
    /// Struct name. `None` for anonymous structs.
    pub ident: Option<Ident<'de>>,
    /// Template parameters if this is a template specialization: `struct Foo<T>`.
    pub generics: Option<Generics<'de>>,
    /// Base class specifiers: `public Base, virtual Interface`.
    pub bases: Vec<BaseSpecifier<'de>>,
    /// Struct body with members, or `Unit` for forward declarations.
    pub fields: Fields<'de>,
}

source_code_span_impl!(
    ItemStruct, attrs, and_then, ident, and_then, generics, bases, fields
);

/// A class definition (C++ specific).
///
/// Identical to [`ItemStruct`] but with `private` as the default access specifier.
///
/// Example: `class Widget : public Base { public: void draw(); };`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemClass<'de> {
    /// C++20 attributes: `[[deprecated]]`, `[[nodiscard]]`, etc.
    pub attrs: Vec<Attribute<'de>>,
    /// Class name. `None` for anonymous classes.
    pub ident: Option<Ident<'de>>,
    /// Template parameters if this is a template specialization: `class Foo<T>`.
    pub generics: Option<Generics<'de>>,
    /// Base class specifiers: `public Base, virtual Interface`.
    pub bases: Vec<BaseSpecifier<'de>>,
    /// Class body with members, or `Unit` for forward declarations.
    pub fields: Fields<'de>,
}

source_code_span_impl!(
    ItemClass, attrs, and_then, ident, and_then, generics, bases, fields
);

/// An enum definition, analogous to `syn::ItemEnum`.
///
/// Covers both unscoped (`enum Color { ... }`) and scoped (`enum class Color { ... }`) enums.
///
/// Example: `enum class Color : uint8_t { Red, Green, Blue };`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemEnum<'de> {
    /// C++20 attributes: `[[nodiscard]]`, etc.
    pub attrs: Vec<Attribute<'de>>,
    /// Enum name. `None` for anonymous enums.
    pub ident: Option<Ident<'de>>,
    /// `true` for `enum class` / `enum struct` (scoped enums, C++11).
    pub scoped: bool,
    /// Explicit underlying type: `enum Color : uint8_t`.
    pub underlying_type: Option<Type<'de>>,
    /// Enum variants, comma-separated.
    pub variants: Punctuated<'de, Variant<'de>>,
}

source_code_span_impl!(
    ItemEnum,
    attrs,
    and_then,
    ident,
    and_then,
    underlying_type,
    variants
);

/// A union definition, analogous to `syn::ItemUnion`.
///
/// Example: `union Data { int i; float f; double d; };`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemUnion<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// Union name. `None` for anonymous unions.
    pub ident: Option<Ident<'de>>,
    /// Union members (all share the same memory location).
    pub fields: FieldsNamed<'de>,
}

source_code_span_impl!(ItemUnion, attrs, and_then, ident, fields);

/// A namespace declaration, analogous to `syn::ItemMod`.
///
/// Example: `inline namespace v2 { void foo(); }`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemNamespace<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// `true` for `inline namespace` (symbols visible in enclosing namespace).
    pub inline_token: bool,
    /// Namespace name. `None` for anonymous namespaces.
    pub ident: Option<Ident<'de>>,
    /// Items declared inside this namespace.
    pub content: Vec<Item<'de>>,
}

source_code_span_impl!(ItemNamespace, attrs, and_then, ident, content);

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

impl<'de> SourceCodeSpan<'de> for ItemUse<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match self {
            ItemUse::Declaration { attrs, name } => {
                if let Some(attrs_span) = attrs.span() {
                    if let Some(name_span) = name.span() {
                        Some(attrs_span.extend(name_span))
                    } else {
                        Some(attrs_span)
                    }
                } else {
                    name.span()
                }
            }
            ItemUse::Directive { attrs, namespace } => {
                if let Some(attrs_span) = attrs.span() {
                    if let Some(namespace_span) = namespace.span() {
                        Some(attrs_span.extend(namespace_span))
                    } else {
                        Some(attrs_span)
                    }
                } else {
                    namespace.span()
                }
            }
            ItemUse::Alias { attrs, ident, ty } => {
                if let Some(attrs_span) = attrs.span() {
                    if let Some(ty_span) = ty.span() {
                        Some(ident.span.extend(attrs_span.extend(ty_span)))
                    } else {
                        Some(ident.span.extend(attrs_span))
                    }
                } else if let Some(ty_span) = ty.span() {
                    Some(ident.span.extend(ty_span))
                } else {
                    Some(ident.span)
                }
            }
        }
    }
}

/// A type alias (`using X = Y`), analogous to `syn::ItemType`.
///
/// Example: `template<typename T> using Vec = std::vector<T>;`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemType<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// The alias name.
    pub ident: Ident<'de>,
    /// Template parameters for alias templates.
    pub generics: Option<Generics<'de>>,
    /// The aliased type.
    pub ty: Type<'de>,
}

source_code_span_impl!(ItemType, attrs, ident, and_then, generics, ty);

/// A typedef declaration (C-style type alias).
///
/// Example: `typedef unsigned long size_t;`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemTypedef<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// The original type being aliased.
    pub ty: Type<'de>,
    /// The new alias name.
    pub ident: Ident<'de>,
}

source_code_span_impl!(ItemTypedef, attrs, ty, ident);

/// A const or constexpr variable, analogous to `syn::ItemConst`.
///
/// Example: `constexpr int MAX_SIZE = 1024;`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemConst<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// `true` for `constexpr` (compile-time evaluated), `false` for plain `const`.
    pub constexpr_token: bool,
    /// The constant's type.
    pub ty: Type<'de>,
    /// Optional qualifying class path.
    /// For `const int MyClass::MIN = 10;`, this is `MyClass`.
    pub class_path: Option<Path<'de>>,
    /// The constant's name.
    pub ident: Ident<'de>,
    /// The initializer expression.
    pub expr: Expr<'de>,
}

source_code_span_impl!(ItemConst, attrs, ty, and_then, class_path, ident, expr);

/// A static variable, analogous to `syn::ItemStatic`.
///
/// Example: `static int instance_count = 0;`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStatic<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// The variable's type.
    pub ty: Type<'de>,
    /// Optional qualifying class path.
    /// For `static int MyClass::count = 0;`, this is `MyClass`.
    pub class_path: Option<Path<'de>>,
    /// The variable's name.
    pub ident: Ident<'de>,
    /// Optional initializer. `None` for uninitialized declarations.
    pub expr: Option<Expr<'de>>,
}

source_code_span_impl!(
    ItemStatic, attrs, ty, and_then, class_path, ident, and_then, expr
);

/// A variable declaration (not `static`, not `const`/`constexpr`).
///
/// Example: `int x = 42;`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemVar<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// The variable's type.
    pub ty: Type<'de>,
    /// The variable's name.
    pub ident: Ident<'de>,
    /// Optional initializer. `None` for uninitialized declarations.
    pub expr: Option<Expr<'de>>,
}

source_code_span_impl!(ItemVar, attrs, ty, ident, and_then, expr);

/// An extern block (`extern "C" { ... }`), analogous to `syn::ItemForeignMod`.
///
/// Example: `extern "C" { void c_function(); }`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemForeignMod<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// The ABI string: `"C"`, `"C++"`, etc.
    pub abi: &'de str,
    /// Declarations inside the extern block.
    pub items: Vec<ForeignItem<'de>>,
}

source_code_span_impl!(ItemForeignMod, attrs, items);

/// A template declaration (C++ specific).
///
/// Example: `template<typename T, int N> class Array { ... };`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemTemplate<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// Template parameters: `<typename T, int N>`.
    pub params: Punctuated<'de, TemplateParam<'de>>,
    /// The templated declaration (function, class, struct, etc.).
    pub item: Box<Item<'de>>,
}

source_code_span_impl!(ItemTemplate, attrs, params, item);

/// A static assertion (C++ specific).
///
/// Example: `static_assert(sizeof(int) == 4, "int must be 4 bytes");`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStaticAssert<'de> {
    /// The boolean condition to assert at compile time.
    pub expr: Expr<'de>,
    /// Optional error message string literal (C++17 made this optional).
    pub message: Option<Expr<'de>>,
}

source_code_span_impl!(ItemStaticAssert, expr, and_then, message);

/// The path in a `#include` directive.
#[derive(Debug, Clone, PartialEq)]
pub enum IncludePath<'de> {
    /// System header enclosed in angle brackets: `<iostream>`
    System(SourceSpan<'de>),
    /// Local header enclosed in quotes: `"myheader.h"`
    Local(SourceSpan<'de>),
}

impl<'de> From<IncludePath<'de>> for SourceSpan<'de> {
    fn from(include_path: IncludePath<'de>) -> Self {
        match include_path {
            IncludePath::System(source_span) => source_span,
            IncludePath::Local(source_span) => source_span,
        }
    }
}

impl<'de> From<&IncludePath<'de>> for SourceSpan<'de> {
    fn from(include_path: &IncludePath<'de>) -> Self {
        match include_path {
            IncludePath::System(source_span) => *source_span,
            IncludePath::Local(source_span) => *source_span,
        }
    }
}

impl<'de> SourceCodeSpan<'de> for IncludePath<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        Some(self.into())
    }
}

/// A `#include` preprocessor directive: `#include <iostream>` or `#include "myfile.h"`.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemInclude<'de> {
    /// Source location of the entire directive.
    pub span: SourceSpan<'de>,
    /// The included path (system `<...>` or local `"..."`).
    pub path: IncludePath<'de>,
}

source_code_span_impl!(ItemInclude, Some, span, path);

/// A preprocessor directive (e.g. `#define`, `#ifdef`, `#pragma`).
#[derive(Debug, Clone, PartialEq)]
pub struct ItemMacro<'de> {
    /// Source location of the entire directive.
    pub span: SourceSpan<'de>,
    /// Raw tokens making up the directive body.
    pub tokens: Vec<Token<'de>>,
}

source_code_span_impl!(ItemMacro, Some, span, tokens);

/// Tokens not interpreted by the parser, analogous to `syn::Item::Verbatim`.
///
/// Used as a fallback when the parser encounters a construct it cannot fully parse.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ItemVerbatim<'de> {
    /// The raw, unparsed tokens.
    pub tokens: LinkedList<Token<'de>>,
}

source_code_span_impl!(ItemVerbatim, tokens);

/// A constructor declaration/definition.
///
/// Example: `explicit Foo(int x) noexcept : m_x(x) { }`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemConstructor<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// `true` if declared `explicit` (prevents implicit conversions).
    pub explicit_token: bool,
    /// `true` if declared `constexpr`.
    pub constexpr_token: bool,
    /// The class name (must match the enclosing class).
    pub ident: Ident<'de>,
    /// Constructor parameters.
    pub inputs: Punctuated<'de, FnArg<'de>>,
    /// `true` if declared `noexcept`.
    pub noexcept_token: bool,
    /// Member initializer list: `: m_x(x), m_y(y)`.
    pub member_init_list: Vec<MemberInit<'de>>,
    /// Constructor body. `None` for declarations.
    pub block: Option<Block<'de>>,
    /// `true` if `= default`.
    pub defaulted: bool,
    /// `true` if `= delete`.
    pub deleted: bool,
}

impl<'de> ItemConstructor<'de> {
    /// Checks if this constructor is a default constructor (no parameters).
    pub fn is_default_constructor(&self) -> bool {
        if let Some((fn_arg, _)) = self.inputs.inner.first()
            && fn_arg.default_value.is_none()
        {
            false
        } else {
            !self.deleted
        }
    }
}

source_code_span_impl!(
    ItemConstructor,
    attrs,
    ident,
    inputs,
    member_init_list,
    and_then,
    block
);

/// A destructor declaration/definition.
///
/// Example: `virtual ~Widget() noexcept = default;`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemDestructor<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// `true` if declared `virtual` (required for polymorphic base classes).
    pub virtual_token: bool,
    /// The class name (the `~ClassName` part, stored without the `~`).
    pub ident: Ident<'de>,
    /// `true` if declared `noexcept`.
    pub noexcept_token: bool,
    /// Destructor body. `None` for declarations.
    pub block: Option<Block<'de>>,
    /// `true` if `= default`.
    pub defaulted: bool,
    /// `true` if `= delete`.
    pub deleted: bool,
    /// `true` if `= 0` (pure virtual destructor).
    pub pure_virtual: bool,
}

source_code_span_impl!(ItemDestructor, attrs, ident, and_then, block);

/// A friend declaration, granting another class or function access to private members.
///
/// Example: `friend class OtherClass;` or `friend void helper(Foo&);`
#[derive(Debug, Clone, PartialEq)]
pub struct ItemFriend<'de> {
    /// C++20 attributes.
    pub attrs: Vec<Attribute<'de>>,
    /// The befriended declaration (function or class).
    pub item: Box<Item<'de>>,
}

source_code_span_impl!(ItemFriend, attrs, item);

//source_code_span_impl!(ItemFriend, attrs, item);

/// Template generics on a class/struct/function, analogous to `syn::Generics`.
///
/// Example: the `<typename T, int N>` in `template<typename T, int N> class Array`.
#[derive(Debug, Clone, PartialEq)]
pub struct Generics<'de> {
    /// The template parameters, comma-separated.
    pub params: Punctuated<'de, TemplateParam<'de>>,
}

source_code_span_impl!(Generics, params);
