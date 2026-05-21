//! Expression AST nodes for C++20
//!
//! Analogous to `syn::Expr`.

use std::str::FromStr;

use crate::ast::ty::{FundamentalKind, TemplateArg, Type};
use crate::{SourceCodeSpan, SourceSpan, source_code_span_impl};

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
            Expr::Path(expr_path) => expr_path.span(),
            Expr::Paren(expr_paren) => expr_paren.expr.span(),
            Expr::Unary(expr_unary) => expr_unary.span(),
            Expr::Binary(expr_binary) => expr_binary.span(),
            Expr::Conditional(expr_conditional) => expr_conditional.span(),
            Expr::Call(expr_call) => expr_call.span(),
            Expr::MethodCall(expr_method_call) => expr_method_call.span(),
            Expr::Index(expr_index) => expr_index.span(),
            Expr::Field(expr_field) => expr_field.span(),
            Expr::Cast(expr_cast) => expr_cast.span(),
            Expr::CStyleCast(expr_cstyle_cast) => expr_cstyle_cast.span(),
            Expr::Sizeof(expr_sizeof) => expr_sizeof.operand.span(),
            Expr::Alignof(expr_alignof) => expr_alignof.ty.span(),
            Expr::New(expr_new) => expr_new.span(),
            Expr::Delete(expr_delete) => expr_delete.expr.span(),
            Expr::Throw(expr_throw) => expr_throw.expr.as_ref().and_then(|expr| expr.span()),
            Expr::Lambda(expr_lambda) => expr_lambda.span(),
            Expr::Typeid(expr_typeid) => expr_typeid.operand.span(),
            Expr::InitList(expr_init_list) => expr_init_list.elements.span(),
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
///
/// May include template arguments: `std::shared_ptr<int>`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprPath<'de> {
    pub path: Path<'de>,
    /// Template arguments, if any: `<int, double>` in `std::pair<int, double>`.
    pub args: Option<Vec<TemplateArg<'de>>>,
}

source_code_span_impl!(ExprPath, path, and_then, args);

/// Parenthesized expression: `(expr)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprParen<'de> {
    pub expr: Box<Expr<'de>>,
}

source_code_span_impl!(ExprParen, expr);

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

source_code_span_impl!(ExprUnary, operand);

/// Binary expression: `lhs op rhs`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprBinary<'de> {
    pub lhs: Box<Expr<'de>>,
    pub op: BinaryOp,
    pub rhs: Box<Expr<'de>>,
}

source_code_span_impl!(ExprBinary, lhs, rhs);

/// Ternary conditional: `condition ? then_expr : else_expr`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprConditional<'de> {
    pub condition: Box<Expr<'de>>,
    pub then_expr: Box<Expr<'de>>,
    pub else_expr: Box<Expr<'de>>,
}

source_code_span_impl!(ExprConditional, condition, then_expr, else_expr);

/// Function call: `callee(args...)`.
///
/// Analogous to `syn::ExprCall`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprCall<'de> {
    pub func: Box<Expr<'de>>,
    pub args: Punctuated<'de, Expr<'de>>,
}

source_code_span_impl!(ExprCall, func, args);

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

source_code_span_impl!(ExprMethodCall, receiver, method, args);

/// Array subscript: `object[index]`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprIndex<'de> {
    pub object: Box<Expr<'de>>,
    pub index: Box<Expr<'de>>,
}

source_code_span_impl!(ExprIndex, object, index);

/// Member access: `object.field` or `ptr->field`.
///
/// Analogous to `syn::ExprField`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprField<'de> {
    pub object: Box<Expr<'de>>,
    pub arrow: bool,
    pub member: Ident<'de>,
}

source_code_span_impl!(ExprField, object, member);

/// C++ named cast: `static_cast<T>(expr)`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprCast<'de> {
    pub cast_kind: CastKind,
    pub ty: Box<super::ty::Type<'de>>,
    pub expr: Box<Expr<'de>>,
}

source_code_span_impl!(ExprCast, ty, expr);

/// C-style cast: `(type)expr`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprCStyleCast<'de> {
    pub ty: Box<super::ty::Type<'de>>,
    pub expr: Box<Expr<'de>>,
}

source_code_span_impl!(ExprCStyleCast, ty, expr);

/// `sizeof` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprSizeof<'de> {
    pub operand: Box<Expr<'de>>,
}

source_code_span_impl!(ExprSizeof, operand);

/// `alignof` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprAlignof<'de> {
    pub ty: Box<super::ty::Type<'de>>,
}

source_code_span_impl!(ExprAlignof, ty);

/// `new` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprNew<'de> {
    pub global: bool,
    pub placement: Option<Punctuated<'de, Expr<'de>>>,
    pub ty: Box<super::ty::Type<'de>>,
    pub initializer: Option<NewInitializer<'de>>,
}

source_code_span_impl!(ExprNew, and_then, placement, ty, and_then, initializer);

/// Initializer for a `new` expression.
#[derive(Debug, Clone, PartialEq)]
pub enum NewInitializer<'de> {
    /// `new T(args)`
    Parens(Punctuated<'de, Expr<'de>>),
    /// `new T{args}`
    Braces(Vec<Expr<'de>>),
}

impl<'de> SourceCodeSpan<'de> for NewInitializer<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match self {
            NewInitializer::Parens(punctuated) => punctuated.span(),
            NewInitializer::Braces(exprs) => exprs.span(),
        }
    }
}

/// `delete` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprDelete<'de> {
    pub global: bool,
    pub array: bool,
    pub expr: Box<Expr<'de>>,
}

source_code_span_impl!(ExprDelete, expr);

/// `throw` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprThrow<'de> {
    pub expr: Option<Box<Expr<'de>>>,
}

source_code_span_impl!(ExprThrow, and_then, expr);

/// Lambda expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprLambda<'de> {
    pub captures: Vec<LambdaCapture<'de>>,
    pub inputs: Option<Punctuated<'de, super::item::FnArg<'de>>>,
    pub return_type: Option<Box<super::ty::Type<'de>>>,
    pub body: super::stmt::Block<'de>,
}

source_code_span_impl!(
    ExprLambda,
    captures,
    and_then,
    inputs,
    and_then,
    return_type,
    body
);

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

impl<'de> SourceCodeSpan<'de> for LambdaCapture<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match &self {
            LambdaCapture::ByValue(ident) | LambdaCapture::ByRef(ident) => Some(ident.span),
            LambdaCapture::Init(ident, expr) => {
                if let Some(expr_span) = expr.span() {
                    Some(ident.span.extend(expr_span))
                } else {
                    Some(ident.span)
                }
            }
            _ => None,
        }
    }
}

/// `typeid` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprTypeid<'de> {
    pub operand: TypeidOperand<'de>,
}

source_code_span_impl!(ExprTypeid, operand);

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

source_code_span_impl!(ExprInitList, elements);

impl<'de> Expr<'de> {
    /// Attempt to evaluate this expression as a compile-time integer constant.
    ///
    /// Returns `None` for expressions that cannot be statically evaluated
    /// (function calls, identifiers, casts, etc.).
    pub fn const_eval_integer(&self) -> Option<i128> {
        match self {
            Expr::Lit(lit) if lit.kind == LitKind::Integer => lit.parse::<i128>().ok(),
            Expr::Unary(unary) => {
                let val = unary.operand.const_eval_integer()?;
                match unary.op {
                    UnaryOp::Negate => Some(-val),
                    UnaryOp::Plus => Some(val),
                    UnaryOp::BitwiseNot => Some(!val),
                    _ => None,
                }
            }
            Expr::Paren(paren) => paren.expr.const_eval_integer(),
            Expr::Binary(binary) => {
                let lhs = binary.lhs.const_eval_integer()?;
                let rhs = binary.rhs.const_eval_integer()?;
                match binary.op {
                    BinaryOp::Add => lhs.checked_add(rhs),
                    BinaryOp::Sub => lhs.checked_sub(rhs),
                    BinaryOp::Mul => lhs.checked_mul(rhs),
                    BinaryOp::Div => lhs.checked_div(rhs),
                    BinaryOp::Mod => lhs.checked_rem(rhs),
                    BinaryOp::ShiftLeft => u32::try_from(rhs).ok().and_then(|r| lhs.checked_shl(r)),
                    BinaryOp::ShiftRight => {
                        u32::try_from(rhs).ok().and_then(|r| lhs.checked_shr(r))
                    }
                    BinaryOp::BitAnd => Some(lhs & rhs),
                    BinaryOp::BitOr => Some(lhs | rhs),
                    BinaryOp::BitXor => Some(lhs ^ rhs),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
