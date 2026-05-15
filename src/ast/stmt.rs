//! Statement AST nodes for C++20
//!
//! Analogous to `syn::Stmt`.

use crate::{SourceCodeSpan, SourceSpan, source_code_span_impl};

use super::expr::Expr;
use super::item::{Ident, Item};
use super::ty::Type;

/// A block (compound statement): `{ stmts... }`.
///
/// Analogous to `syn::Block`.
#[derive(Debug, Clone, PartialEq)]
pub struct Block<'de> {
    pub stmts: Vec<Stmt<'de>>,
}

source_code_span_impl!(Block, stmts);

/// A statement, analogous to `syn::Stmt`.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt<'de> {
    /// A local declaration (variable, type alias, etc.)
    Local(StmtLocal<'de>),
    /// A top-level item used as a statement (e.g., local struct definition)
    Item(Item<'de>),
    /// An expression statement: `expr;`
    Expr(StmtExpr<'de>),
    /// Return: `return expr;`
    Return(StmtReturn<'de>),
    /// Break: `break;`
    Break(StmtBreak<'de>),
    /// Continue: `continue;`
    Continue(StmtContinue<'de>),
    /// Goto: `goto label;`
    Goto(StmtGoto<'de>),
    /// Label: `label:`
    Label(StmtLabel<'de>),
    /// If / else if / else
    If(StmtIf<'de>),
    /// While loop: `while (cond) body`
    While(StmtWhile<'de>),
    /// Do-while loop: `do body while (cond);`
    DoWhile(StmtDoWhile<'de>),
    /// For loop: `for (init; cond; incr) body`
    For(StmtFor<'de>),
    /// Range-based for: `for (decl : range) body`
    ForRange(StmtForRange<'de>),
    /// Switch: `switch (expr) { cases }`
    Switch(StmtSwitch<'de>),
    /// Case label: `case value:`
    Case(StmtCase<'de>),
    /// Default label: `default:`
    Default(StmtDefault<'de>),
    /// Try/catch: `try { } catch (...) { }`
    TryCatch(StmtTryCatch<'de>),
    /// Compound statement (block): `{ stmts... }`
    Block(Block<'de>),
    /// Empty statement: `;`
    Empty,
}

impl<'de> SourceCodeSpan<'de> for Stmt<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        match self {
            Stmt::Local(stmt_local) => stmt_local.span(),
            Stmt::Item(item) => item.span(),
            Stmt::Expr(stmt_expr) => stmt_expr.span(),
            Stmt::Return(stmt_return) => stmt_return.span(),
            Stmt::Break(stmt_break) => stmt_break.span(),
            Stmt::Continue(stmt_continue) => stmt_continue.span(),
            Stmt::Goto(stmt_goto) => stmt_goto.span(),
            Stmt::Label(stmt_label) => stmt_label.span(),
            Stmt::If(stmt_if) => stmt_if.span(),
            Stmt::While(stmt_while) => stmt_while.span(),
            Stmt::DoWhile(stmt_do_while) => stmt_do_while.span(),
            Stmt::For(stmt_for) => stmt_for.span(),
            Stmt::ForRange(stmt_for_range) => stmt_for_range.span(),
            Stmt::Switch(stmt_switch) => stmt_switch.span(),
            Stmt::Case(stmt_case) => stmt_case.span(),
            Stmt::Default(stmt_default) => stmt_default.span(),
            Stmt::TryCatch(stmt_try_catch) => stmt_try_catch.span(),
            Stmt::Block(block) => block.span(),
            Stmt::Empty => None,
        }
    }
}

/// A local variable declaration: `int x = 42;`.
///
/// Analogous to `syn::Local`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtLocal<'de> {
    pub ty: Type<'de>,
    pub ident: Ident<'de>,
    pub init: Option<Expr<'de>>,
}

source_code_span_impl!(StmtLocal, ty, ident, and_then, init);

/// An expression followed by a semicolon.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtExpr<'de> {
    pub expr: Expr<'de>,
}

source_code_span_impl!(StmtExpr, expr);

/// `return expr;` or `return;`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtReturn<'de> {
    pub expr: Option<Expr<'de>>,
}

source_code_span_impl!(StmtReturn, and_then, expr);

/// `break;`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtBreak<'de> {
    pub span: crate::SourceSpan<'de>,
}

source_code_span_impl!(StmtBreak, Some, span);

/// `continue;`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtContinue<'de> {
    pub span: crate::SourceSpan<'de>,
}

source_code_span_impl!(StmtContinue, Some, span);

/// `goto label;`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtGoto<'de> {
    pub label: Ident<'de>,
}

source_code_span_impl!(StmtGoto, label);

/// `label:`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtLabel<'de> {
    pub label: Ident<'de>,
}

source_code_span_impl!(StmtLabel, label);

/// If / else if / else.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtIf<'de> {
    pub init: Option<Box<Stmt<'de>>>,
    pub condition: Expr<'de>,
    pub then_body: Box<Stmt<'de>>,
    pub else_body: Option<Box<Stmt<'de>>>,
}

source_code_span_impl!(
    StmtIf, and_then, init, condition, then_body, and_then, else_body
);

/// `while (cond) body`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtWhile<'de> {
    pub condition: Expr<'de>,
    pub body: Box<Stmt<'de>>,
}

source_code_span_impl!(StmtWhile, condition, body);

/// `do body while (cond);`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtDoWhile<'de> {
    pub body: Box<Stmt<'de>>,
    pub condition: Expr<'de>,
}

source_code_span_impl!(StmtDoWhile, body, condition);

/// `for (init; cond; incr) body`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtFor<'de> {
    pub init: Option<Box<Stmt<'de>>>,
    pub condition: Option<Expr<'de>>,
    pub increment: Option<Expr<'de>>,
    pub body: Box<Stmt<'de>>,
}

source_code_span_impl!(
    StmtFor, and_then, init, and_then, condition, and_then, increment, body
);

/// `for (decl : range) body`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtForRange<'de> {
    pub ty: Type<'de>,
    pub ident: Ident<'de>,
    pub range: Expr<'de>,
    pub body: Box<Stmt<'de>>,
}

source_code_span_impl!(StmtForRange, ty, ident, range, body);

/// `switch (expr) { body }`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtSwitch<'de> {
    pub expr: Expr<'de>,
    pub body: Block<'de>,
}

source_code_span_impl!(StmtSwitch, expr, body);

/// `case value:`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtCase<'de> {
    pub value: Expr<'de>,
}

source_code_span_impl!(StmtCase, value);

/// `default:`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtDefault<'de> {
    pub span: crate::SourceSpan<'de>,
}

source_code_span_impl!(StmtDefault, Some, span);

/// `try { } catch (param) { }`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtTryCatch<'de> {
    pub try_body: Block<'de>,
    pub catches: Vec<CatchClause<'de>>,
}

source_code_span_impl!(StmtTryCatch, try_body, catches);

/// A catch clause.
#[derive(Debug, Clone, PartialEq)]
pub struct CatchClause<'de> {
    pub param: CatchParam<'de>,
    pub body: Block<'de>,
}
source_code_span_impl!(CatchClause, param, body);

/// A catch parameter.
#[derive(Debug, Clone, PartialEq)]
pub enum CatchParam<'de> {
    /// `catch (Type name)` or `catch (Type)`
    Typed {
        ty: Type<'de>,
        ident: Option<Ident<'de>>,
    },
    /// `catch (...)`
    Ellipsis,
}

impl<'de> SourceCodeSpan<'de> for CatchParam<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        if let CatchParam::Typed { ty, ident } = &self {
            if let Some(ty_span) = ty.span() {
                if let Some(ident_span) = ident.map(|i| i.span) {
                    Some(ty_span.extend(ident_span))
                } else {
                    Some(ty_span)
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}
