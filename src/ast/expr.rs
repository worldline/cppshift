//! Expression AST nodes for C++20
//!
//! Analogous to `syn::Expr`.

use std::str::FromStr;

use crate::ast::ty::{FundamentalKind, Type};
use crate::{SourceCodeSpan, SourceSpan};

use super::item::{Ident, Path};
use super::punct::Punctuated;

/// The kind of a literal value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LitKind {
    Integer,
    Float,
    String,
    Char,
}

impl LitKind {
    /// Check if the fundamental kind matches the literal kind
    pub fn match_fundamental(&self, kind: FundamentalKind) -> bool {
        match self {
            LitKind::Integer => matches!(
                kind,
                FundamentalKind::Short
                    | FundamentalKind::Int
                    | FundamentalKind::Long
                    | FundamentalKind::LongLong
                    | FundamentalKind::SignedChar
                    | FundamentalKind::UnsignedChar
                    | FundamentalKind::UnsignedShort
                    | FundamentalKind::UnsignedInt
                    | FundamentalKind::UnsignedLong
                    | FundamentalKind::UnsignedLongLong
            ),
            LitKind::Float => matches!(
                kind,
                FundamentalKind::Float | FundamentalKind::Double | FundamentalKind::LongDouble
            ),
            LitKind::Char => matches!(
                kind,
                FundamentalKind::Char
                    | FundamentalKind::Wchar
                    | FundamentalKind::Char8
                    | FundamentalKind::Char16
                    | FundamentalKind::Char32
            ),
            _ => false,
        }
    }

    /// Check if the literal kind matches the type
    pub fn match_type(&self, ty: &Type) -> bool {
        match ty {
            Type::Fundamental(fund) => self.match_fundamental(fund.kind),
            Type::Qualified(qualified) => self.match_type(&qualified.ty),
            _ => false,
        }
    }
}

/// Unary operator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    // Prefix
    Plus,
    Negate,
    LogicalNot,
    BitwiseNot,
    Deref,
    AddressOf,
    PreIncrement,
    PreDecrement,
    // Postfix
    PostIncrement,
    PostDecrement,
}

/// Binary operator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOp {
    // Arithmetic
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    // Shift
    ShiftLeft,
    ShiftRight,
    // Comparison
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    ThreeWay,
    Equal,
    NotEqual,
    // Bitwise
    BitAnd,
    BitXor,
    BitOr,
    // Logical
    LogicalAnd,
    LogicalOr,
    // Assignment
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
    ShiftLeftAssign,
    ShiftRightAssign,
    BitAndAssign,
    BitXorAssign,
    BitOrAssign,
    // Member access
    Dot,
    Arrow,
    DotStar,
    ArrowStar,
    // Comma
    Comma,
}

/// C++ cast kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CastKind {
    Static,
    Dynamic,
    Const,
    Reinterpret,
}

/// A C++ expression, analogous to `syn::Expr`.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr<'de> {
    /// Literal value: `42`, `3.14`, `"hello"`, `'a'`
    Lit(ExprLit<'de>),
    /// Simple identifier: `foo`
    Ident(ExprIdent<'de>),
    /// Qualified path: `std::cout`, `::global`
    Path(ExprPath<'de>),
    /// Parenthesized expression: `(expr)`
    Paren(ExprParen<'de>),
    /// Boolean literal: `true` / `false`
    Bool(ExprBool<'de>),
    /// `nullptr`
    Nullptr(ExprNullptr<'de>),
    /// `this`
    This(ExprThis<'de>),
    /// Unary expression: `-x`, `!flag`, `*ptr`, `++i`, `i++`
    Unary(ExprUnary<'de>),
    /// Binary expression: `a + b`, `x = 1`
    Binary(ExprBinary<'de>),
    /// Ternary conditional: `a ? b : c`
    Conditional(ExprConditional<'de>),
    /// Function call: `foo(a, b)`
    Call(ExprCall<'de>),
    /// Method call: `obj.method(a, b)`
    MethodCall(ExprMethodCall<'de>),
    /// Array subscript: `arr[i]`
    Index(ExprIndex<'de>),
    /// Member access: `obj.field`
    Field(ExprField<'de>),
    /// C++ named cast: `static_cast<int>(x)`
    Cast(ExprCast<'de>),
    /// C-style cast: `(int)x`
    CStyleCast(ExprCStyleCast<'de>),
    /// `sizeof(expr)` or `sizeof expr`
    Sizeof(ExprSizeof<'de>),
    /// `alignof(type)`
    Alignof(ExprAlignof<'de>),
    /// `new` expression: `new int(42)`
    New(ExprNew<'de>),
    /// `delete` expression: `delete ptr`
    Delete(ExprDelete<'de>),
    /// `throw` expression: `throw runtime_error("...")`
    Throw(ExprThrow<'de>),
    /// Lambda expression: `[captures](params) { body }`
    Lambda(ExprLambda<'de>),
    /// `typeid(expr)` or `typeid(type)`
    Typeid(ExprTypeid<'de>),
    /// Braced initializer list: `{1, 2, 3}`
    InitList(ExprInitList<'de>),
}

impl<'de> SourceCodeSpan<'de> for Expr<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match self {
            Expr::Lit(ExprLit { span, .. })
            | Expr::Bool(ExprBool { span, .. })
            | Expr::Nullptr(ExprNullptr { span, .. })
            | Expr::This(ExprThis { span, .. }) => Some(*span),
            Expr::Ident(ExprIdent { ident }) => Some(ident.span),
            Expr::Paren(expr_paren) => expr_paren.expr.span(),
            Expr::Unary(expr_unary) => expr_unary.span(),
            Expr::Binary(expr_binary) => expr_binary.span(),
            Expr::Conditional(expr_conditional) => expr_conditional.span(),
            Expr::Call(expr_call) => expr_call.span(),
            Expr::Sizeof(expr_sizeof) => expr_sizeof.operand.span(),
            Expr::Alignof(expr_alignof) => expr_alignof.ty.span(),
            Expr::Typeid(expr_typeid) => expr_typeid.operand.span(),
            _ => None,
        }
    }
}

/// Literal expression: numbers, strings, chars.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExprLit<'de> {
    pub span: SourceSpan<'de>,
    pub kind: LitKind,
}

impl<'de> ExprLit<'de> {
    /// Parse the literal value from the source span as the specified type.
    pub fn parse<F>(&self) -> Result<F, F::Err>
    where
        F: FromStr,
    {
        self.span.src().parse::<F>()
    }
}

/// Simple identifier expression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExprIdent<'de> {
    pub ident: Ident<'de>,
}

/// Qualified path expression: `std::cout`, `::global::func`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprPath<'de> {
    pub path: Path<'de>,
}

/// Parenthesized expression: `(expr)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprParen<'de> {
    pub expr: Box<Expr<'de>>,
}

/// Boolean literal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExprBool<'de> {
    pub span: SourceSpan<'de>,
    pub value: bool,
}

/// `nullptr` expression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExprNullptr<'de> {
    pub span: SourceSpan<'de>,
}

/// `this` expression.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExprThis<'de> {
    pub span: SourceSpan<'de>,
}

/// Unary expression (prefix or postfix).
#[derive(Debug, Clone, PartialEq)]
pub struct ExprUnary<'de> {
    pub op: UnaryOp,
    pub operand: Box<Expr<'de>>,
}

impl<'de> SourceCodeSpan<'de> for ExprUnary<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        self.operand.span()
    }
}

/// Binary expression: `lhs op rhs`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprBinary<'de> {
    pub lhs: Box<Expr<'de>>,
    pub op: BinaryOp,
    pub rhs: Box<Expr<'de>>,
}

impl<'de> SourceCodeSpan<'de> for ExprBinary<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        self.lhs.span().map(|l| {
            if let Some(r) = self.rhs.span() {
                l.extend(r)
            } else {
                l
            }
        })
    }
}

/// Ternary conditional: `condition ? then_expr : else_expr`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprConditional<'de> {
    pub condition: Box<Expr<'de>>,
    pub then_expr: Box<Expr<'de>>,
    pub else_expr: Box<Expr<'de>>,
}

impl<'de> SourceCodeSpan<'de> for ExprConditional<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        self.condition.span().map(|c| {
            if let Some(e) = self.else_expr.span() {
                c.extend(e)
            } else if let Some(t) = self.then_expr.span() {
                c.extend(t)
            } else {
                c
            }
        })
    }
}

/// Function call: `callee(args...)`.
///
/// Analogous to `syn::ExprCall`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprCall<'de> {
    pub func: Box<Expr<'de>>,
    pub args: Punctuated<'de, Expr<'de>>,
}

impl<'de> SourceCodeSpan<'de> for ExprCall<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        if let Some(func_span) = self.func.span() {
            if let Some(args_span) = self.args.span() {
                Some(func_span.extend(args_span))
            } else {
                Some(func_span)
            }
        } else {
            self.args.span()
        }
    }
}

/// Method call: `receiver.method(args...)`.
///
/// Analogous to `syn::ExprMethodCall`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprMethodCall<'de> {
    pub receiver: Box<Expr<'de>>,
    pub arrow: bool,
    pub method: Ident<'de>,
    pub args: Punctuated<'de, Expr<'de>>,
}

/// Array subscript: `object[index]`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprIndex<'de> {
    pub object: Box<Expr<'de>>,
    pub index: Box<Expr<'de>>,
}

/// Member access: `object.field` or `ptr->field`.
///
/// Analogous to `syn::ExprField`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprField<'de> {
    pub object: Box<Expr<'de>>,
    pub arrow: bool,
    pub member: Ident<'de>,
}

/// C++ named cast: `static_cast<T>(expr)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprCast<'de> {
    pub cast_kind: CastKind,
    pub ty: Box<super::ty::Type<'de>>,
    pub expr: Box<Expr<'de>>,
}

/// C-style cast: `(type)expr`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprCStyleCast<'de> {
    pub ty: Box<super::ty::Type<'de>>,
    pub expr: Box<Expr<'de>>,
}

/// `sizeof` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprSizeof<'de> {
    pub operand: Box<Expr<'de>>,
}

/// `alignof` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprAlignof<'de> {
    pub ty: Box<super::ty::Type<'de>>,
}

/// `new` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprNew<'de> {
    pub global: bool,
    pub placement: Option<Punctuated<'de, Expr<'de>>>,
    pub ty: Box<super::ty::Type<'de>>,
    pub initializer: Option<NewInitializer<'de>>,
}

/// Initializer for a `new` expression.
#[derive(Debug, Clone, PartialEq)]
pub enum NewInitializer<'de> {
    /// `new T(args)`
    Parens(Punctuated<'de, Expr<'de>>),
    /// `new T{args}`
    Braces(Vec<Expr<'de>>),
}

/// `delete` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprDelete<'de> {
    pub global: bool,
    pub array: bool,
    pub expr: Box<Expr<'de>>,
}

/// `throw` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprThrow<'de> {
    pub expr: Option<Box<Expr<'de>>>,
}

/// Lambda expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprLambda<'de> {
    pub captures: Vec<LambdaCapture<'de>>,
    pub inputs: Option<Punctuated<'de, super::item::FnArg<'de>>>,
    pub return_type: Option<Box<super::ty::Type<'de>>>,
    pub body: super::stmt::Block<'de>,
}

/// A lambda capture.
#[derive(Debug, Clone, PartialEq)]
pub enum LambdaCapture<'de> {
    /// Default copy capture: `=`
    DefaultCopy,
    /// Default reference capture: `&`
    DefaultRef,
    /// Capture by value: `x`
    ByValue(Ident<'de>),
    /// Capture by reference: `&x`
    ByRef(Ident<'de>),
    /// Capture this: `this`
    This,
    /// Capture *this: `*this`
    StarThis,
    /// Init capture: `x = expr`
    Init(Ident<'de>, Box<Expr<'de>>),
}

/// `typeid` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprTypeid<'de> {
    pub operand: TypeidOperand<'de>,
}

/// Operand of a `typeid` expression.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeidOperand<'de> {
    Type(Box<super::ty::Type<'de>>),
    Expr(Box<Expr<'de>>),
}

impl<'de> SourceCodeSpan<'de> for TypeidOperand<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match &self {
            TypeidOperand::Type(op_type) => op_type.span(),
            TypeidOperand::Expr(op_expr) => op_expr.span(),
        }
    }
}

/// Braced initializer list: `{1, 2, 3}`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprInitList<'de> {
    pub elements: Vec<Expr<'de>>,
}
