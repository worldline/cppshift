//! Statement AST nodes for C++20
//!
//! Analogous to `syn::Stmt`.

use crate::{SourceCodeSpan, SourceSpan};

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

impl<'de> SourceCodeSpan<'de> for Block<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        let mut span: Option<SourceSpan<'de>> = None;
        for stmt in &self.stmts {
            if let Some(stmt_span) = stmt.span() {
                span = Some(if let Some(existing_span) = span {
                    existing_span.extend(stmt_span)
                } else {
                    stmt_span
                });
            }
        }

        span
    }
}

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
            Stmt::Local(local) => local.span(),
            Stmt::Expr(stmt_expr) => stmt_expr.expr.span(),
            Stmt::Return(ret) => ret.expr.as_ref().and_then(|e| e.span()),
            Stmt::Break(b) => Some(b.span),
            Stmt::Continue(c) => Some(c.span),
            Stmt::Goto(g) => Some(g.label.span),
            Stmt::Label(l) => Some(l.label.span),
            Stmt::Case(c) => c.value.span(),
            Stmt::Default(d) => Some(d.span),
            Stmt::Block(b) => b.span(),
            _ => None,
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

impl<'de> SourceCodeSpan<'de> for StmtLocal<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        Some(if let Some(ty_span) = self.ty.span() {
            if let Some(i) = self.init.as_ref().and_then(|i| i.span()) {
                ty_span.extend(i)
            } else {
                ty_span.extend(self.ident.span)
            }
        } else {
            self.ident.span
        })
    }
}

/// An expression followed by a semicolon.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtExpr<'de> {
    pub expr: Expr<'de>,
}

/// `return expr;` or `return;`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtReturn<'de> {
    pub expr: Option<Expr<'de>>,
}

/// `break;`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtBreak<'de> {
    pub span: crate::SourceSpan<'de>,
}

/// `continue;`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtContinue<'de> {
    pub span: crate::SourceSpan<'de>,
}

/// `goto label;`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtGoto<'de> {
    pub label: Ident<'de>,
}

/// `label:`
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtLabel<'de> {
    pub label: Ident<'de>,
}

/// If / else if / else.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtIf<'de> {
    pub init: Option<Box<Stmt<'de>>>,
    pub condition: Expr<'de>,
    pub then_body: Box<Stmt<'de>>,
    pub else_body: Option<Box<Stmt<'de>>>,
}

/// `while (cond) body`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtWhile<'de> {
    pub condition: Expr<'de>,
    pub body: Box<Stmt<'de>>,
}

/// `do body while (cond);`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtDoWhile<'de> {
    pub body: Box<Stmt<'de>>,
    pub condition: Expr<'de>,
}

/// `for (init; cond; incr) body`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtFor<'de> {
    pub init: Option<Box<Stmt<'de>>>,
    pub condition: Option<Expr<'de>>,
    pub increment: Option<Expr<'de>>,
    pub body: Box<Stmt<'de>>,
}

/// `for (decl : range) body`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtForRange<'de> {
    pub ty: Type<'de>,
    pub ident: Ident<'de>,
    pub range: Expr<'de>,
    pub body: Box<Stmt<'de>>,
}

/// `switch (expr) { body }`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtSwitch<'de> {
    pub expr: Expr<'de>,
    pub body: Block<'de>,
}

/// `case value:`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtCase<'de> {
    pub value: Expr<'de>,
}

/// `default:`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StmtDefault<'de> {
    pub span: crate::SourceSpan<'de>,
}

/// `try { } catch (param) { }`.
#[derive(Debug, Clone, PartialEq)]
pub struct StmtTryCatch<'de> {
    pub try_body: Block<'de>,
    pub catches: Vec<CatchClause<'de>>,
}

/// A catch clause.
#[derive(Debug, Clone, PartialEq)]
pub struct CatchClause<'de> {
    pub param: CatchParam<'de>,
    pub body: Block<'de>,
}

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
