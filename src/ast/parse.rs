//! Internal ad-hoc parser for C++20
//!
//! Not part of the public API. Used exclusively by [`super::parse_file`].

use std::collections::LinkedList;

use crate::SourceSpan;
use crate::lex::{Lexer, Token, TokenKind};

use super::File;
use super::error::AstError;
use super::expr::*;
use super::item::*;
use super::punct::Punctuated;
use super::stmt::*;
use super::ty::*;

// ---------------------------------------------------------------------------
// Parser — thin wrapper around a cloneable Lexer
// ---------------------------------------------------------------------------

pub(super) struct Parser<'de> {
    src: &'de str,
    lexer: Lexer<'de>,
    /// The most recently consumed token (for span tracking)
    last_token: Option<Token<'de>>,
    /// When we split a `>>` (ShiftRight) inside nested template args, the second
    /// `>` is logically still pending. This holds the token so that peek/bump
    /// can return it before advancing the underlying lexer.
    pending_right_chevron: Option<Token<'de>>,
}

/// Advance a lexer clone, skipping comments, returning the next token.
fn next_non_comment<'de>(lexer: &mut Lexer<'de>) -> Option<Token<'de>> {
    loop {
        match lexer.next()? {
            Ok(tok) if tok.kind() == TokenKind::Comment => continue,
            Ok(tok) => return Some(tok),
            Err(_) => return None,
        }
    }
}

impl<'de> Parser<'de> {
    pub fn new(src: &'de str) -> Result<Self, AstError> {
        Ok(Parser {
            src,
            lexer: Lexer::new(src),
            last_token: None,
            pending_right_chevron: None,
        })
    }

    fn peek(&self) -> Option<Token<'de>> {
        if let Some(tok) = self.pending_right_chevron {
            return Some(tok);
        }
        let mut clone = self.lexer.clone();
        next_non_comment(&mut clone)
    }

    fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind())
    }

    fn peek_nth(&self, n: usize) -> Option<Token<'de>> {
        let mut clone = self.lexer.clone();
        if self.pending_right_chevron.is_some() && n == 0 {
            return self.pending_right_chevron;
        }

        let start = if self.pending_right_chevron.is_some() {
            1
        } else {
            0
        };
        for _ in start..n {
            next_non_comment(&mut clone)?;
        }
        next_non_comment(&mut clone)
    }

    fn peek_nth_kind(&self, n: usize) -> Option<TokenKind> {
        self.peek_nth(n).map(|t| t.kind())
    }

    fn bump(&mut self) -> Result<Token<'de>, AstError> {
        if let Some(tok) = self.pending_right_chevron.take() {
            self.last_token = Some(tok);
            return Ok(tok);
        }
        loop {
            match self.lexer.next() {
                Some(Ok(tok)) if tok.kind() == TokenKind::Comment => continue,
                Some(Ok(tok)) => {
                    self.last_token = Some(tok);
                    return Ok(tok);
                }
                Some(Err(e)) => return Err(e.into()),
                None => return Err(self.eof_error("token")),
            }
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token<'de>, AstError> {
        match self.peek() {
            Some(token) if token.kind() == kind => self.bump(),
            Some(token) => Err(AstError::UnexpectedToken {
                expected: format!("{kind:?}"),
                found: token.kind(),
                src: self.src.to_string(),
                err_span: token.src_span().into(),
            }),
            None => Err(self.eof_error(&format!("{kind:?}"))),
        }
    }

    fn eat(&mut self, kind: TokenKind) -> Option<Token<'de>> {
        if self.peek_kind() == Some(kind) {
            self.bump().ok()
        } else {
            None
        }
    }

    fn is_empty(&self) -> bool {
        self.peek().is_none()
    }

    /// Clone the lexer for backtracking.
    fn checkpoint(&self) -> (Lexer<'de>, Option<Token<'de>>) {
        (self.lexer.clone(), self.pending_right_chevron)
    }

    /// Restore the lexer from a previously saved clone.
    fn restore(&mut self, saved: &(Lexer<'de>, Option<Token<'de>>)) {
        self.lexer = saved.0.clone();
        self.pending_right_chevron = saved.1;
    }

    /// Compute a span from `start` (captured via `peek()` before parsing)
    /// through the most recently consumed token.
    fn span_since(&self, start: Option<Token<'de>>) -> SourceSpan<'de> {
        let start_span = match start {
            Some(tok) => tok.src_span(),
            None => return SourceSpan::new(self.src, self.src.len(), 0),
        };
        let start_range: core::ops::Range<usize> = start_span.into();
        let end_span = match self.last_token {
            Some(tok) => tok.src_span(),
            None => return start_span,
        };
        let end_range: core::ops::Range<usize> = end_span.into();
        SourceSpan::new(
            self.src,
            start_range.start,
            end_range.end - start_range.start,
        )
    }

    /// Get the span of the most recently consumed token.
    fn last_span(&self) -> SourceSpan<'de> {
        self.last_token
            .map(|t| t.src_span())
            .unwrap_or_else(|| SourceSpan::new(self.src, self.src.len(), 0))
    }

    fn eof_error(&self, expected: &str) -> AstError {
        AstError::UnexpectedEof {
            expected: expected.to_string(),
            src: self.src.to_string(),
            err_span: miette::SourceSpan::new(self.src.len().into(), 0),
        }
    }

    fn error_at_current(&self, message: &str) -> AstError {
        match self.peek() {
            Some(tok) => AstError::Custom {
                message: message.to_string(),
                src: self.src.to_string(),
                err_span: tok.src_span().into(),
            },
            None => self.eof_error(message),
        }
    }
}

// ---------------------------------------------------------------------------
// Top-level: parse_file
// ---------------------------------------------------------------------------

pub(super) fn parse_file<'de>(src: &'de str) -> Result<File<'de>, AstError> {
    let mut parser = Parser::new(src)?;
    let mut items = Vec::new();

    while !parser.is_empty() {
        items.push(parse_item(&mut parser)?);
    }

    Ok(File {
        attrs: Vec::new(),
        items,
    })
}

// ---------------------------------------------------------------------------
// Attributes: [[...]]
// ---------------------------------------------------------------------------

fn parse_attributes<'de>(p: &mut Parser<'de>) -> Result<Vec<Attribute<'de>>, AstError> {
    let mut attrs = Vec::new();
    while p.peek_kind() == Some(TokenKind::DoubleLeftBracket) {
        attrs.push(parse_attribute(p)?);
    }
    Ok(attrs)
}

fn parse_attribute<'de>(p: &mut Parser<'de>) -> Result<Attribute<'de>, AstError> {
    let start = p.peek();
    p.expect(TokenKind::DoubleLeftBracket)?;

    // Parse attribute path (e.g., nodiscard, gnu::always_inline)
    let path = parse_path(p)?;

    // Collect any argument tokens until ]]
    let mut args = Vec::new();
    if p.peek_kind() == Some(TokenKind::LeftParenthese) {
        p.bump()?;
        let mut depth = 1u32;
        while depth > 0 && !p.is_empty() {
            match p.peek_kind() {
                Some(TokenKind::LeftParenthese) => {
                    depth += 1;
                    args.push(p.bump()?);
                }
                Some(TokenKind::RightParenthese) => {
                    depth -= 1;
                    if depth > 0 {
                        args.push(p.bump()?);
                    } else {
                        p.bump()?;
                    }
                }
                _ => {
                    args.push(p.bump()?);
                }
            }
        }
    }

    p.expect(TokenKind::DoubleRightBracket)?;
    let span = p.span_since(start);
    Ok(Attribute { span, path, args })
}

// ---------------------------------------------------------------------------
// Item parsing
// ---------------------------------------------------------------------------

fn parse_item<'de>(p: &mut Parser<'de>) -> Result<Item<'de>, AstError> {
    // Skip preprocessor directives (lines starting with #)
    if p.peek_kind() == Some(TokenKind::NumberSign) {
        if p.peek_nth(1)
            .is_some_and(|t| t.src_span().src() == "include")
        {
            return parse_item_include(p).map(Item::Include);
        }
        return parse_item_macro(p).map(Item::Macro);
    }

    // Empty declaration (stray semicolons, e.g., after namespace or class body)
    if p.peek_kind() == Some(TokenKind::Semicolon) {
        p.bump()?;
        return Ok(Item::Verbatim(ItemVerbatim::default()));
    }

    // Parse leading attributes [[...]]
    let attrs = parse_attributes(p)?;

    // Collect leading specifiers to determine what kind of item this is
    let item = match p.peek_kind() {
        Some(TokenKind::KeywordNamespace) => Some(parse_item_namespace(p).map(Item::Namespace)?),
        Some(TokenKind::KeywordInline)
            if p.peek_nth_kind(1) == Some(TokenKind::KeywordNamespace) =>
        {
            Some(parse_item_namespace(p).map(Item::Namespace)?)
        }
        Some(TokenKind::KeywordStaticAssert) => {
            Some(parse_item_static_assert(p).map(Item::StaticAssert)?)
        }
        Some(TokenKind::KeywordUsing) => Some(parse_item_using(p)?),
        Some(TokenKind::KeywordTypedef) => Some(parse_item_typedef(p).map(Item::Typedef)?),
        Some(TokenKind::KeywordEnum) => Some(parse_item_enum(p).map(Item::Enum)?),
        Some(TokenKind::KeywordStruct) => Some(parse_item_struct(p).map(Item::Struct)?),
        Some(TokenKind::KeywordClass) => Some(parse_item_class(p).map(Item::Class)?),
        Some(TokenKind::KeywordUnion) => Some(parse_item_union(p).map(Item::Union)?),
        Some(TokenKind::KeywordTemplate) => Some(parse_item_template(p).map(Item::Template)?),
        Some(TokenKind::KeywordExtern) if p.peek_nth_kind(1) == Some(TokenKind::String) => {
            Some(parse_item_foreign_mod(p).map(Item::ForeignMod)?)
        }
        // asm declaration: skip content as verbatim
        Some(TokenKind::KeywordAsm) => {
            p.bump()?;
            p.eat(TokenKind::KeywordVolatile);
            p.expect(TokenKind::LeftParenthese)?;
            let mut depth = 1u32;
            while depth > 0 && !p.is_empty() {
                match p.peek_kind() {
                    Some(TokenKind::LeftParenthese) => {
                        depth += 1;
                        p.bump()?;
                    }
                    Some(TokenKind::RightParenthese) => {
                        depth -= 1;
                        p.bump()?;
                    }
                    _ => {
                        p.bump()?;
                    }
                }
            }
            p.expect(TokenKind::Semicolon)?;
            Some(Item::Verbatim(ItemVerbatim::default()))
        }
        _ => None,
    };

    if let Some(item) = item {
        if !attrs.is_empty() {
            return Ok(set_item_attrs(item, attrs));
        }
        return Ok(item);
    }

    // Try parsing as a function or variable declaration
    // This handles: [specifiers] type ident (...) { } or [specifiers] type ident ;
    let item = parse_item_fn_or_var(p)?;

    // Apply leading attributes to the item if it doesn't already have them
    if !attrs.is_empty() {
        return Ok(set_item_attrs(item, attrs));
    }
    Ok(item)
}

/// Set attributes on an item (used to propagate leading [[...]] attributes).
fn set_item_attrs<'de>(mut item: Item<'de>, attrs: Vec<Attribute<'de>>) -> Item<'de> {
    match &mut item {
        Item::Fn(f) => f.attrs = attrs,
        Item::Struct(s) => s.attrs = attrs,
        Item::Class(c) => c.attrs = attrs,
        Item::Enum(e) => e.attrs = attrs,
        Item::Union(u) => u.attrs = attrs,
        Item::Namespace(n) => n.attrs = attrs,
        Item::Use(ItemUse::Declaration { attrs: a, .. }) => *a = attrs,
        Item::Use(ItemUse::Directive { attrs: a, .. }) => *a = attrs,
        Item::Use(ItemUse::Alias { attrs: a, .. }) => *a = attrs,
        Item::Type(t) => t.attrs = attrs,
        Item::Typedef(t) => t.attrs = attrs,
        Item::Const(c) => c.attrs = attrs,
        Item::Static(s) => s.attrs = attrs,
        Item::Var(v) => v.attrs = attrs,
        Item::ForeignMod(f) => f.attrs = attrs,
        Item::Template(t) => t.attrs = attrs,
        Item::StaticAssert(_) | Item::Include(_) | Item::Macro(_) | Item::Verbatim(_) => {}
    }
    item
}

// ---------------------------------------------------------------------------
// Preprocessor macro: # ... (until end of logical line)
// ---------------------------------------------------------------------------

fn parse_item_include<'de>(p: &mut Parser<'de>) -> Result<ItemInclude<'de>, AstError> {
    let start = p.peek();
    p.expect(TokenKind::NumberSign)?;
    p.bump()?; // consume the "include" ident

    let path = if p.peek_kind() == Some(TokenKind::LeftChevron) {
        // System include: <...> — capture tokens inside angle brackets, excluding delimiters
        p.expect(TokenKind::LeftChevron)?;
        let path_start = p.peek();
        while !p.is_empty() && p.peek_kind() != Some(TokenKind::RightChevron) {
            p.bump()?;
        }
        let path_span = p.span_since(path_start);
        p.expect(TokenKind::RightChevron)?;
        IncludePath::System(path_span)
    } else {
        // Local include: "..." — strip surrounding quotes
        let str_tok = p.bump()?;
        let str_range: core::ops::Range<usize> = str_tok.src_span().into();
        IncludePath::Local(SourceSpan::new(
            p.src,
            str_range.start + 1,
            str_range.len() - 2,
        ))
    };

    let span = p.span_since(start);
    Ok(ItemInclude { span, path })
}

fn parse_item_macro<'de>(p: &mut Parser<'de>) -> Result<ItemMacro<'de>, AstError> {
    let start = p.peek();
    let hash_tok = p.expect(TokenKind::NumberSign)?;
    let mut tokens = Vec::new();

    // Find the line number of the # token
    let hash_offset: usize = {
        let r: core::ops::Range<usize> = hash_tok.src_span().into();
        r.start
    };
    let hash_line = p.src[..hash_offset].matches('\n').count();

    // Collect tokens on the same line (or continuation lines ending with \)
    let mut current_line = hash_line;
    while !p.is_empty() {
        let next = p.peek().unwrap();
        let next_offset: usize = {
            let r: core::ops::Range<usize> = next.src_span().into();
            r.start
        };
        let next_line = p.src[..next_offset].matches('\n').count();

        // If the next token is on a new line and the previous line didn't end with \,
        // stop collecting (unless it's a multi-line macro with \ continuation)
        if next_line > current_line {
            // Check if ALL intermediate lines between current_line and next_line
            // ended with \ (line continuation)
            let mut has_continuation = true;
            let before = &p.src[..next_offset];
            // Find the content of lines between current_line and next_line
            for line in before
                .lines()
                .skip(current_line)
                .take(next_line - current_line)
            {
                if !line.trim_end().ends_with('\\') {
                    has_continuation = false;
                    break;
                }
            }
            if !has_continuation {
                break;
            }
        }
        current_line = next_line;
        tokens.push(p.bump()?);
    }

    let span = p.span_since(start);
    Ok(ItemMacro { span, tokens })
}

// ---------------------------------------------------------------------------
// Namespace
// ---------------------------------------------------------------------------

fn parse_item_namespace<'de>(p: &mut Parser<'de>) -> Result<ItemNamespace<'de>, AstError> {
    let inline_token = p.eat(TokenKind::KeywordInline).is_some();
    p.expect(TokenKind::KeywordNamespace)?;

    // Collect namespace segments for nested syntax: namespace a::b::c { }
    let mut segments = Vec::new();
    if p.peek_kind() == Some(TokenKind::Ident) {
        segments.push(parse_ident(p)?);
        while p.eat(TokenKind::DoubleColon).is_some() {
            segments.push(parse_ident(p)?);
        }
    }

    p.expect(TokenKind::LeftBrace)?;
    let mut content = Vec::new();
    while p.peek_kind() != Some(TokenKind::RightBrace) && !p.is_empty() {
        content.push(parse_item(p)?);
    }
    p.expect(TokenKind::RightBrace)?;

    // Build nested namespaces from inside out
    if segments.is_empty() {
        return Ok(ItemNamespace {
            attrs: Vec::new(),
            inline_token,
            ident: None,
            content,
        });
    }

    let mut result = ItemNamespace {
        attrs: Vec::new(),
        inline_token: false,
        ident: segments.last().copied(),
        content,
    };
    for seg in segments.iter().rev().skip(1) {
        result = ItemNamespace {
            attrs: Vec::new(),
            inline_token: false,
            ident: Some(*seg),
            content: vec![Item::Namespace(result)],
        };
    }
    result.inline_token = inline_token;

    Ok(result)
}

// ---------------------------------------------------------------------------
// static_assert
// ---------------------------------------------------------------------------

fn parse_item_static_assert<'de>(p: &mut Parser<'de>) -> Result<ItemStaticAssert<'de>, AstError> {
    p.expect(TokenKind::KeywordStaticAssert)?;
    p.expect(TokenKind::LeftParenthese)?;
    let expr = parse_expr(p)?;
    let message = if p.eat(TokenKind::Comma).is_some() {
        Some(parse_expr(p)?)
    } else {
        None
    };
    p.expect(TokenKind::RightParenthese)?;
    p.expect(TokenKind::Semicolon)?;
    Ok(ItemStaticAssert { expr, message })
}

// ---------------------------------------------------------------------------
// Using (declaration, directive, alias)
// ---------------------------------------------------------------------------

fn parse_item_using<'de>(p: &mut Parser<'de>) -> Result<Item<'de>, AstError> {
    p.expect(TokenKind::KeywordUsing)?;

    // using namespace path;
    if p.eat(TokenKind::KeywordNamespace).is_some() {
        let namespace = parse_path(p)?;
        p.expect(TokenKind::Semicolon)?;
        return Ok(Item::Use(ItemUse::Directive {
            attrs: Vec::new(),
            namespace,
        }));
    }

    // using ident = type;  (type alias)
    if p.peek_kind() == Some(TokenKind::Ident) && p.peek_nth_kind(1) == Some(TokenKind::Equal) {
        let ident = parse_ident(p)?;
        p.expect(TokenKind::Equal)?;
        let ty = parse_type(p)?;
        p.expect(TokenKind::Semicolon)?;
        return Ok(Item::Type(ItemType {
            attrs: Vec::new(),
            ident,
            generics: None,
            ty,
        }));
    }

    // using path;  (using declaration)
    let name = parse_path(p)?;
    p.expect(TokenKind::Semicolon)?;
    Ok(Item::Use(ItemUse::Declaration {
        attrs: Vec::new(),
        name,
    }))
}

// ---------------------------------------------------------------------------
// Typedef
// ---------------------------------------------------------------------------

fn parse_item_typedef<'de>(p: &mut Parser<'de>) -> Result<ItemTypedef<'de>, AstError> {
    p.expect(TokenKind::KeywordTypedef)?;
    let mut ty = parse_type(p)?;
    let ident = parse_ident(p)?;
    // Handle C-style array typedefs: `typedef char type24[3];`
    while p.peek_kind() == Some(TokenKind::LeftBracket) {
        p.bump()?;
        let size = if p.peek_kind() != Some(TokenKind::RightBracket) {
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect(TokenKind::RightBracket)?;
        ty = Type::Array(TypeArray {
            element: Box::new(ty),
            size,
        });
    }
    p.expect(TokenKind::Semicolon)?;
    Ok(ItemTypedef {
        attrs: Vec::new(),
        ty,
        ident,
    })
}

// ---------------------------------------------------------------------------
// Enum
// ---------------------------------------------------------------------------

fn parse_item_enum<'de>(p: &mut Parser<'de>) -> Result<ItemEnum<'de>, AstError> {
    p.expect(TokenKind::KeywordEnum)?;
    let scoped =
        p.eat(TokenKind::KeywordClass).is_some() || p.eat(TokenKind::KeywordStruct).is_some();

    let ident = if p.peek_kind() == Some(TokenKind::Ident) {
        Some(parse_ident(p)?)
    } else {
        None
    };

    let underlying_type = if p.eat(TokenKind::Colon).is_some() {
        Some(parse_type(p)?)
    } else {
        None
    };

    let mut variants = Punctuated::new();
    if p.eat(TokenKind::LeftBrace).is_some() {
        while p.peek_kind() != Some(TokenKind::RightBrace) && !p.is_empty() {
            let v_ident = parse_ident(p)?;
            let discriminant = if p.eat(TokenKind::Equal).is_some() {
                Some(parse_expr_no_comma(p)?)
            } else {
                None
            };
            let variant = Variant {
                attrs: Vec::new(),
                ident: v_ident,
                discriminant,
            };
            if let Some(comma) = p.eat(TokenKind::Comma) {
                variants.push_pair(variant, comma);
            } else {
                variants.push_value(variant);
                break;
            }
        }
        p.expect(TokenKind::RightBrace)?;
    }

    // A trailing declarator may follow the enum body, e.g.
    // `enum { A, B } mField;` declares an unnamed enum type and a member of
    // that type in one statement. The declarator (and any initializer) is
    // discarded — only the enum type is retained. Skip to the terminating `;`,
    // balancing brackets from array declarators or brace/paren initializers.
    if p.peek_kind() != Some(TokenKind::Semicolon) {
        let mut depth = 0usize;
        while let Some(kind) = p.peek_kind() {
            match kind {
                TokenKind::LeftBrace | TokenKind::LeftParenthese | TokenKind::LeftBracket => {
                    depth += 1;
                }
                TokenKind::RightBrace | TokenKind::RightParenthese | TokenKind::RightBracket => {
                    depth = depth.saturating_sub(1);
                }
                TokenKind::Semicolon if depth == 0 => break,
                _ => {}
            }
            p.bump()?;
        }
    }
    p.expect(TokenKind::Semicolon)?;

    Ok(ItemEnum {
        attrs: Vec::new(),
        ident,
        scoped,
        underlying_type,
        variants,
    })
}

// ---------------------------------------------------------------------------
// Struct
// ---------------------------------------------------------------------------

fn parse_item_struct<'de>(p: &mut Parser<'de>) -> Result<ItemStruct<'de>, AstError> {
    p.expect(TokenKind::KeywordStruct)?;

    let ident = if p.peek_kind() == Some(TokenKind::Ident) {
        Some(parse_ident(p)?)
    } else {
        None
    };

    // `struct Foo final : ...` — the `final` specifier follows the name.
    p.eat(TokenKind::KeywordFinal);

    // Forward declaration: struct Foo;
    if p.eat(TokenKind::Semicolon).is_some() {
        return Ok(ItemStruct {
            attrs: Vec::new(),
            ident,
            generics: None,
            bases: Vec::new(),
            fields: Fields::Unit,
        });
    }

    let bases = if p.eat(TokenKind::Colon).is_some() {
        parse_base_specifiers(p)?
    } else {
        Vec::new()
    };

    let class_name = ident.map(|i| i.sym);
    let fields = parse_fields_named(p, class_name)?;
    p.expect(TokenKind::Semicolon)?;

    Ok(ItemStruct {
        attrs: Vec::new(),
        ident,
        generics: None,
        bases,
        fields: Fields::Named(fields),
    })
}

// ---------------------------------------------------------------------------
// Class
// ---------------------------------------------------------------------------

fn parse_item_class<'de>(p: &mut Parser<'de>) -> Result<ItemClass<'de>, AstError> {
    p.expect(TokenKind::KeywordClass)?;

    let ident = if p.peek_kind() == Some(TokenKind::Ident) {
        Some(parse_ident(p)?)
    } else {
        None
    };

    // `class Foo final : ...` — the `final` specifier follows the name.
    p.eat(TokenKind::KeywordFinal);

    // Forward declaration: class Foo;
    if p.eat(TokenKind::Semicolon).is_some() {
        return Ok(ItemClass {
            attrs: Vec::new(),
            ident,
            generics: None,
            bases: Vec::new(),
            fields: Fields::Unit,
        });
    }

    let bases = if p.eat(TokenKind::Colon).is_some() {
        parse_base_specifiers(p)?
    } else {
        Vec::new()
    };

    let class_name = ident.map(|i| i.sym);
    let fields = parse_fields_named(p, class_name)?;
    p.expect(TokenKind::Semicolon)?;

    Ok(ItemClass {
        attrs: Vec::new(),
        ident,
        generics: None,
        bases,
        fields: Fields::Named(fields),
    })
}

// ---------------------------------------------------------------------------
// Union
// ---------------------------------------------------------------------------

fn parse_item_union<'de>(p: &mut Parser<'de>) -> Result<ItemUnion<'de>, AstError> {
    p.expect(TokenKind::KeywordUnion)?;
    let ident = if p.peek_kind() == Some(TokenKind::Ident) {
        Some(parse_ident(p)?)
    } else {
        None
    };
    let fields = parse_fields_named(p, None)?;
    p.expect(TokenKind::Semicolon)?;
    Ok(ItemUnion {
        attrs: Vec::new(),
        ident,
        fields,
    })
}

// ---------------------------------------------------------------------------
// Template
// ---------------------------------------------------------------------------

fn parse_item_template<'de>(p: &mut Parser<'de>) -> Result<ItemTemplate<'de>, AstError> {
    p.expect(TokenKind::KeywordTemplate)?;
    p.expect(TokenKind::LeftChevron)?;

    let mut params = Punctuated::new();
    while p.peek_kind() != Some(TokenKind::RightChevron) && !p.is_empty() {
        let param = parse_template_param(p)?;
        if let Some(comma) = p.eat(TokenKind::Comma) {
            params.push_pair(param, comma);
        } else {
            params.push_value(param);
            break;
        }
    }
    p.expect(TokenKind::RightChevron)?;

    let item = parse_item(p)?;

    Ok(ItemTemplate {
        attrs: Vec::new(),
        params,
        item: Box::new(item),
    })
}

fn parse_template_param<'de>(p: &mut Parser<'de>) -> Result<TemplateParam<'de>, AstError> {
    // typename T / class T / typename... Args
    if p.peek_kind() == Some(TokenKind::KeywordTypename)
        || p.peek_kind() == Some(TokenKind::KeywordClass)
    {
        p.bump()?;
        if p.eat(TokenKind::Ellipsis).is_some() {
            let ident = parse_ident(p)?;
            return Ok(TemplateParam::Pack { ident });
        }
        let ident = if p.peek_kind() == Some(TokenKind::Ident) {
            Some(parse_ident(p)?)
        } else {
            None
        };
        let default = if p.eat(TokenKind::Equal).is_some() {
            Some(parse_type(p)?)
        } else {
            None
        };
        return Ok(TemplateParam::Type { ident, default });
    }

    // Non-type template parameter: int N = 0
    let ty = parse_type(p)?;
    let ident = if p.peek_kind() == Some(TokenKind::Ident) {
        Some(parse_ident(p)?)
    } else {
        None
    };
    let default = if p.eat(TokenKind::Equal).is_some() {
        Some(parse_expr(p)?)
    } else {
        None
    };
    Ok(TemplateParam::NonType { ty, ident, default })
}

// ---------------------------------------------------------------------------
// extern "C" { ... }
// ---------------------------------------------------------------------------

fn parse_item_foreign_mod<'de>(p: &mut Parser<'de>) -> Result<ItemForeignMod<'de>, AstError> {
    p.expect(TokenKind::KeywordExtern)?;
    let abi_tok = p.expect(TokenKind::String)?;
    // Strip quotes from abi string
    let abi_raw = abi_tok.src();
    let abi = &abi_raw[1..abi_raw.len() - 1];

    // extern "C" single-declaration (no braces)
    if p.peek_kind() != Some(TokenKind::LeftBrace) {
        let _item = parse_item(p)?;
        return Ok(ItemForeignMod {
            attrs: Vec::new(),
            abi,
            items: vec![ForeignItem::Verbatim(ItemVerbatim::default())],
        });
    }

    p.expect(TokenKind::LeftBrace)?;
    let mut items = Vec::new();
    while p.peek_kind() != Some(TokenKind::RightBrace) && !p.is_empty() {
        let inner_item = parse_item(p)?;
        match inner_item {
            Item::Fn(f) => items.push(ForeignItem::Fn(f)),
            Item::Static(s) => items.push(ForeignItem::Static(s)),
            _ => items.push(ForeignItem::Verbatim(ItemVerbatim::default())),
        }
    }
    p.expect(TokenKind::RightBrace)?;

    Ok(ItemForeignMod {
        attrs: Vec::new(),
        abi,
        items,
    })
}

// ---------------------------------------------------------------------------
// Function or variable declaration (the ambiguous case)
// ---------------------------------------------------------------------------

/// Check if const/volatile is followed by another specifier (e.g., `const static int`).
/// This distinguishes `const static int x` (const as leading specifier) from `const int x`
/// (const as type qualifier).
fn is_specifier_after_cv(p: &Parser<'_>) -> bool {
    matches!(
        p.peek_nth_kind(1),
        Some(
            TokenKind::KeywordStatic
                | TokenKind::KeywordInline
                | TokenKind::KeywordExtern
                | TokenKind::KeywordVirtual
                | TokenKind::KeywordConstexpr
                | TokenKind::KeywordConsteval
                | TokenKind::KeywordConstinit
                | TokenKind::KeywordThreadLocal
                | TokenKind::KeywordMutable
                | TokenKind::KeywordExplicit
                | TokenKind::KeywordRegister
        )
    )
}

fn parse_item_fn_or_var<'de>(p: &mut Parser<'de>) -> Result<Item<'de>, AstError> {
    // Parse leading specifiers
    let mut constexpr_token = false;
    let mut consteval_token = false;
    let mut inline_token = false;
    let mut virtual_token = false;
    let mut static_token = false;
    let mut explicit_token = false;
    let mut _extern_token = false;
    let mut leading_const = false;
    let mut leading_volatile = false;

    loop {
        match p.peek_kind() {
            Some(TokenKind::KeywordConstexpr) => {
                p.bump()?;
                constexpr_token = true;
            }
            Some(TokenKind::KeywordConsteval) => {
                p.bump()?;
                consteval_token = true;
            }
            Some(TokenKind::KeywordInline) => {
                p.bump()?;
                inline_token = true;
            }
            Some(TokenKind::KeywordVirtual) => {
                p.bump()?;
                virtual_token = true;
            }
            Some(TokenKind::KeywordStatic) => {
                p.bump()?;
                static_token = true;
            }
            Some(TokenKind::KeywordExplicit) => {
                p.bump()?;
                explicit_token = true;
            }
            Some(TokenKind::KeywordExtern) => {
                p.bump()?;
                _extern_token = true;
            }
            Some(TokenKind::KeywordConstinit) => {
                p.bump()?;
                // constinit is like constexpr for storage
            }
            Some(TokenKind::KeywordThreadLocal) => {
                p.bump()?;
            }
            Some(TokenKind::KeywordRegister) => {
                p.bump()?;
            }
            Some(TokenKind::KeywordMutable) => {
                p.bump()?;
            }
            Some(TokenKind::KeywordFriend) => {
                // friend declaration — skip for now, parse inner item
                p.bump()?;
            }
            // Handle const/volatile appearing before other specifiers (e.g., const static int)
            Some(TokenKind::KeywordConst) if is_specifier_after_cv(p) => {
                p.bump()?;
                leading_const = true;
            }
            Some(TokenKind::KeywordVolatile) if is_specifier_after_cv(p) => {
                p.bump()?;
                leading_volatile = true;
            }
            _ => break,
        }
    }

    // Skip macro annotations before return type (e.g., GTEST_NO_INLINE_ GTEST_NO_TAIL_CALL_)
    // These are identifiers (often ALL_CAPS) followed by another identifier or type keyword
    while p.peek_kind() == Some(TokenKind::Ident) {
        // Check if this identifier is followed by something that looks like a type
        // (another identifier, a type keyword, or ::/const/volatile)
        let next = p.peek_nth_kind(1);
        let looks_like_macro_attr = matches!(
            next,
            Some(
                TokenKind::Ident
                    | TokenKind::DoubleColon
                    | TokenKind::KeywordConst
                    | TokenKind::KeywordVolatile
                    | TokenKind::KeywordVoid
                    | TokenKind::KeywordBool
                    | TokenKind::KeywordChar
                    | TokenKind::KeywordInt
                    | TokenKind::KeywordFloat
                    | TokenKind::KeywordDouble
                    | TokenKind::KeywordLong
                    | TokenKind::KeywordShort
                    | TokenKind::KeywordSigned
                    | TokenKind::KeywordUnsigned
                    | TokenKind::KeywordAuto
            )
        );
        if !looks_like_macro_attr {
            break;
        }
        // Check if the identifier after this one could also be a macro attr or is a real type
        // Heuristic: if current ident contains only uppercase + underscores, it's likely a macro
        let src = p.peek().unwrap().src();
        // Macro identifiers typically contain underscores and are multi-character ALL_CAPS
        let is_macro_like = src.len() > 1
            && src.contains('_')
            && src
                .chars()
                .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit());
        if !is_macro_like {
            break;
        }
        p.bump()?;
        // Also consume optional (args)
        if p.peek_kind() == Some(TokenKind::LeftParenthese) {
            p.bump()?;
            let mut depth = 1u32;
            while depth > 0 && !p.is_empty() {
                match p.peek_kind() {
                    Some(TokenKind::LeftParenthese) => {
                        depth += 1;
                        p.bump()?;
                    }
                    Some(TokenKind::RightParenthese) => {
                        depth -= 1;
                        p.bump()?;
                    }
                    _ => {
                        p.bump()?;
                    }
                }
            }
        }
    }

    // Parse return type
    let mut return_type = parse_type(p)?;

    // Re-apply const/volatile that were consumed as leading specifiers
    if leading_const || leading_volatile {
        return_type = Type::Qualified(TypeQualified {
            cv: CvQualifiers {
                const_token: leading_const,
                volatile_token: leading_volatile,
            },
            ty: Box::new(return_type),
        });
    }

    // Function pointer declaration: return_type (*name)(params)
    if p.peek_kind() == Some(TokenKind::LeftParenthese)
        && p.peek_nth_kind(1) == Some(TokenKind::Star)
    {
        p.bump()?; // (
        p.bump()?; // *
        let fp_ident = if p.peek_kind() == Some(TokenKind::Ident) {
            parse_ident(p)?
        } else {
            // Anonymous function pointer — create dummy ident
            return Err(p.error_at_current("expected function pointer name"));
        };
        p.expect(TokenKind::RightParenthese)?;
        let fn_params = parse_fn_params(p)?;
        // Build FnPtr type
        let fn_ptr_type = Type::FnPtr(TypeFnPtr {
            return_type: Box::new(return_type),
            params: {
                let mut params = Punctuated::new();
                for (arg, punct) in fn_params.pairs() {
                    if let Some(p) = punct {
                        params.push_pair(arg.ty.clone(), *p);
                    } else {
                        params.push_value(arg.ty.clone());
                    }
                }
                params
            },
        });
        let expr = if p.eat(TokenKind::Equal).is_some() {
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect(TokenKind::Semicolon)?;
        return Ok(Item::Static(ItemStatic {
            attrs: Vec::new(),
            ty: fn_ptr_type,
            class_path: None,
            ident: fp_ident,
            expr,
        }));
    }

    // Qualified destructor: ClassName::~ClassName()
    if p.peek_kind() == Some(TokenKind::DoubleColon) && p.peek_nth_kind(1) == Some(TokenKind::Compl)
    {
        let class_path = if let Type::Path(tp) = &return_type {
            Some(tp.path.clone())
        } else {
            None
        };
        p.bump()?; // ::
        p.bump()?; // ~
        let dtor_ident = parse_ident(p)?;
        let inputs = parse_fn_params(p)?;
        let noexcept_token = p.eat(TokenKind::KeywordNoexcept).is_some();
        // = default or = delete
        if p.eat(TokenKind::Equal).is_some() {
            p.bump()?; // default or delete or 0
        }
        // Skip macro annotations (e.g., GTEST_LOCK_EXCLUDED_(mutex_))
        while p.peek_kind() == Some(TokenKind::Ident) {
            p.bump()?;
            if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                p.bump()?;
                let mut depth = 1u32;
                while depth > 0 && !p.is_empty() {
                    match p.peek_kind() {
                        Some(TokenKind::LeftParenthese) => {
                            depth += 1;
                            p.bump()?;
                        }
                        Some(TokenKind::RightParenthese) => {
                            depth -= 1;
                            p.bump()?;
                        }
                        _ => {
                            p.bump()?;
                        }
                    }
                }
            }
        }
        let block = if p.peek_kind() == Some(TokenKind::LeftBrace) {
            Some(parse_block(p)?)
        } else {
            p.expect(TokenKind::Semicolon)?;
            None
        };
        return Ok(Item::Fn(ItemFn {
            attrs: Vec::new(),
            vis: Visibility::Inherited,
            sig: Signature {
                constexpr_token,
                consteval_token,
                inline_token,
                virtual_token,
                static_token,
                explicit_token,
                return_type: Type::Fundamental(TypeFundamental {
                    span: dtor_ident.span,
                    kind: FundamentalKind::Void,
                }),
                class_path,
                ident: dtor_ident,
                inputs,
                variadic: false,
                const_token: false,
                noexcept_token,
                override_token: false,
                final_token: false,
                pure_virtual: false,
                defaulted: false,
                deleted: false,
                destructor_token: true,
                member_init_list: Vec::new(),
            },
            block,
        }));
    }

    // If next token is ( and the "type" is a single unqualified identifier,
    // this might be a macro invocation like GTEST_DEFINE_bool_(...);
    if p.peek_kind() == Some(TokenKind::LeftParenthese) {
        let is_macro = match &return_type {
            Type::Path(tp) => tp.path.segments.len() == 1 && !tp.path.leading_colon,
            _ => false,
        };
        if !is_macro {
            // Could be a qualified function: Type::Path was parsed as
            // A::B::method where method is actually the function name
            // Try to extract the last segment as the function name
            if let Type::Path(tp) = &return_type
                && tp.path.segments.len() >= 2
            {
                let fn_name = tp.path.segments.last().unwrap().ident;
                // Build class_path from all segments except the last
                let class_path = {
                    let mut segments = tp.path.segments.clone();
                    segments.pop();
                    Some(Path {
                        leading_colon: tp.path.leading_colon,
                        segments,
                    })
                };
                let inputs = parse_fn_params(p)?;
                // Trailing qualifiers
                let mut const_token = false;
                let mut noexcept_token = false;
                loop {
                    match p.peek_kind() {
                        Some(TokenKind::KeywordConst) => {
                            p.bump()?;
                            const_token = true;
                        }
                        Some(TokenKind::KeywordNoexcept) => {
                            p.bump()?;
                            noexcept_token = true;
                            if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                                p.bump()?;
                                let _expr = parse_expr(p)?;
                                p.expect(TokenKind::RightParenthese)?;
                            }
                        }
                        Some(TokenKind::KeywordOverride | TokenKind::KeywordFinal) => {
                            p.bump()?;
                        }
                        _ => break,
                    }
                }
                // Member initializer list for constructors only
                let mut member_init_list = Vec::new();
                let is_constructor = class_path.as_ref().is_some_and(|cp| {
                    cp.segments
                        .last()
                        .is_some_and(|seg| seg.ident.sym == fn_name.sym)
                });
                if is_constructor && p.eat(TokenKind::Colon).is_some() {
                    loop {
                        skip_macro_annotations(p)?;
                        let member = parse_path(p)?;
                        p.expect(TokenKind::LeftParenthese)?;
                        let mut args = Punctuated::new();
                        while p.peek_kind() != Some(TokenKind::RightParenthese) && !p.is_empty() {
                            let arg = parse_expr_no_comma(p)?;
                            if let Some(comma) = p.eat(TokenKind::Comma) {
                                args.push_pair(arg, comma);
                            } else {
                                args.push_value(arg);
                                break;
                            }
                        }
                        p.expect(TokenKind::RightParenthese)?;
                        member_init_list.push(MemberInit { member, args });
                        if p.eat(TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                // Trailing return type
                if p.peek_kind() == Some(TokenKind::PointerMember) {
                    p.bump()?;
                    let _rt = parse_type(p)?;
                }
                let block = if p.peek_kind() == Some(TokenKind::LeftBrace) {
                    Some(parse_block(p)?)
                } else {
                    p.expect(TokenKind::Semicolon)?;
                    None
                };
                return Ok(Item::Fn(ItemFn {
                    attrs: Vec::new(),
                    vis: Visibility::Inherited,
                    sig: Signature {
                        constexpr_token,
                        consteval_token,
                        inline_token,
                        virtual_token,
                        static_token,
                        explicit_token,
                        return_type: Type::Fundamental(TypeFundamental {
                            span: fn_name.span,
                            kind: FundamentalKind::Void,
                        }),
                        class_path,
                        ident: fn_name,
                        inputs,
                        variadic: false,
                        const_token,
                        noexcept_token,
                        override_token: false,
                        final_token: false,
                        pure_virtual: false,
                        defaulted: false,
                        deleted: false,
                        destructor_token: false,
                        member_init_list,
                    },
                    block,
                }));
            }
            return Err(p.error_at_current("expected identifier or function pointer"));
        }
        // Treat as macro invocation: consume everything until matching ) and ;
        let macro_ident = match &return_type {
            Type::Path(tp) => tp.path.segments[0].ident,
            _ => unreachable!(),
        };
        let mut tokens = LinkedList::from([Token::new(macro_ident.span, TokenKind::Ident)]);
        tokens.push_back(p.bump()?); // (
        let mut depth = 1u32;
        while depth > 0 && !p.is_empty() {
            match p.peek_kind() {
                Some(TokenKind::LeftParenthese) => {
                    depth += 1;
                    tokens.push_back(p.bump()?);
                }
                Some(TokenKind::RightParenthese) => {
                    depth -= 1;
                    tokens.push_back(p.bump()?);
                }
                _ => {
                    tokens.push_back(p.bump()?);
                }
            }
        }
        // Optional trailing block { ... } (e.g., MACRO_NAME(suite, name) { body })
        if p.peek_kind() == Some(TokenKind::LeftBrace) {
            tokens.push_back(p.bump()?);
            let mut brace_depth = 1u32;
            while brace_depth > 0 && !p.is_empty() {
                match p.peek_kind() {
                    Some(TokenKind::LeftBrace) => {
                        brace_depth += 1;
                        tokens.push_back(p.bump()?);
                    }
                    Some(TokenKind::RightBrace) => {
                        brace_depth -= 1;
                        tokens.push_back(p.bump()?);
                    }
                    _ => {
                        tokens.push_back(p.bump()?);
                    }
                }
            }
        } else if let Some(semi) = p.eat(TokenKind::Semicolon) {
            tokens.push_back(semi);
        }
        return Ok(Item::Verbatim(ItemVerbatim { tokens }));
    }

    // Parse name — could be a qualified path (e.g., Foo::bar, A::B::method)
    // or an operator overload (operator+), or a destructor (~Foo handled elsewhere)
    let (class_path, ident) = if p.peek_kind() == Some(TokenKind::KeywordOperator) {
        // operator overload: use span from 'operator' keyword through operator token
        let op_tok = p.bump()?;
        // Parse the operator symbol(s)
        match p.peek_kind() {
            Some(TokenKind::LeftParenthese) => {
                // operator()
                p.bump()?;
                p.expect(TokenKind::RightParenthese)?;
            }
            Some(TokenKind::LeftBracket) => {
                // operator[]
                p.bump()?;
                p.expect(TokenKind::RightBracket)?;
            }
            Some(TokenKind::KeywordNew) | Some(TokenKind::KeywordDelete) => {
                p.bump()?;
                // operator new[] / operator delete[]
                if p.peek_kind() == Some(TokenKind::LeftBracket) {
                    p.bump()?;
                    p.expect(TokenKind::RightBracket)?;
                }
            }
            _ => {
                // Single-token operators: +, -, *, etc.
                p.bump()?;
            }
        }
        let end_span = p.last_span();
        let start_r: core::ops::Range<usize> = op_tok.src_span().into();
        let end_r: core::ops::Range<usize> = end_span.into();
        let span = SourceSpan::new(p.src, start_r.start, end_r.end - start_r.start);
        (
            None,
            Ident {
                sym: &p.src[start_r.start..end_r.end],
                span,
            },
        )
    } else {
        // Regular ident, possibly qualified: Foo::bar
        let first = parse_ident(p)?;
        if p.peek_kind() == Some(TokenKind::DoubleColon)
            && matches!(
                p.peek_nth_kind(1),
                Some(TokenKind::Ident | TokenKind::Compl | TokenKind::KeywordOperator)
            )
        {
            // Qualified name: consume all :: segments, tracking qualifier
            let mut qualifier_segments = vec![PathSegment { ident: first }];
            let mut last = first;
            while p.eat(TokenKind::DoubleColon).is_some() {
                if p.peek_kind() == Some(TokenKind::Compl) {
                    // Qualified destructor: Foo::~Foo
                    p.bump()?;
                    let dtor_ident = parse_ident(p)?;
                    last = Ident {
                        sym: &p.src[{
                            let r: core::ops::Range<usize> = last.span.into();
                            r.start
                        }..{
                            let r: core::ops::Range<usize> = dtor_ident.span.into();
                            r.end
                        }],
                        span: SourceSpan::new(
                            p.src,
                            {
                                let r: core::ops::Range<usize> = last.span.into();
                                r.start
                            },
                            {
                                let r1: core::ops::Range<usize> = last.span.into();
                                let r2: core::ops::Range<usize> = dtor_ident.span.into();
                                r2.end - r1.start
                            },
                        ),
                    };
                    break;
                } else if p.peek_kind() == Some(TokenKind::KeywordOperator) {
                    // Qualified operator: Foo::operator+
                    let op_start = p.bump()?; // consume 'operator'
                    match p.peek_kind() {
                        Some(TokenKind::LeftParenthese) => {
                            p.bump()?;
                            p.expect(TokenKind::RightParenthese)?;
                        }
                        Some(TokenKind::LeftBracket) => {
                            p.bump()?;
                            p.expect(TokenKind::RightBracket)?;
                        }
                        Some(TokenKind::KeywordNew | TokenKind::KeywordDelete) => {
                            p.bump()?;
                            if p.peek_kind() == Some(TokenKind::LeftBracket) {
                                p.bump()?;
                                p.expect(TokenKind::RightBracket)?;
                            }
                        }
                        _ => {
                            p.bump()?;
                        } // single-token operator
                    }
                    let end_span = p.last_span();
                    let start_r: core::ops::Range<usize> = op_start.src_span().into();
                    let end_r: core::ops::Range<usize> = end_span.into();
                    last = Ident {
                        sym: &p.src[start_r.start..end_r.end],
                        span: SourceSpan::new(p.src, start_r.start, end_r.end - start_r.start),
                    };
                    break;
                } else if p.peek_kind() == Some(TokenKind::Ident) {
                    last = parse_ident(p)?;
                    qualifier_segments.push(PathSegment { ident: last });
                } else {
                    break;
                }
            }
            // The last segment in qualifier_segments is actually the function name, remove it
            qualifier_segments.pop();
            let class_path = if qualifier_segments.is_empty() {
                None
            } else {
                Some(Path {
                    leading_colon: false,
                    segments: qualifier_segments,
                })
            };
            (class_path, last)
        } else {
            (None, first)
        }
    };

    // Function: ident followed by (
    if p.peek_kind() == Some(TokenKind::LeftParenthese) {
        let inputs = parse_fn_params(p)?;

        // Trailing qualifiers
        let mut const_token = false;
        let mut noexcept_token = false;
        let mut override_token = false;
        let mut final_token = false;
        let mut pure_virtual = false;
        let mut defaulted = false;
        let mut deleted = false;

        loop {
            match p.peek_kind() {
                Some(TokenKind::KeywordConst) => {
                    p.bump()?;
                    const_token = true;
                }
                Some(TokenKind::KeywordNoexcept) => {
                    p.bump()?;
                    noexcept_token = true;
                    // Optional noexcept(expr)
                    if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                        p.bump()?;
                        let _expr = parse_expr(p)?;
                        p.expect(TokenKind::RightParenthese)?;
                    }
                }
                Some(TokenKind::KeywordOverride) => {
                    p.bump()?;
                    override_token = true;
                }
                Some(TokenKind::KeywordFinal) => {
                    p.bump()?;
                    final_token = true;
                }
                _ => break,
            }
        }

        // Trailing return type: -> Type
        if p.peek_kind() == Some(TokenKind::PointerMember) {
            p.bump()?;
            return_type = parse_type(p)?;
        }

        // = 0, = default, = delete
        if p.eat(TokenKind::Equal).is_some() {
            match p.peek_kind() {
                Some(TokenKind::Number) => {
                    p.bump()?; // consume 0
                    pure_virtual = true;
                }
                Some(TokenKind::KeywordDefault) => {
                    p.bump()?;
                    defaulted = true;
                }
                Some(TokenKind::KeywordDelete) => {
                    p.bump()?;
                    deleted = true;
                }
                _ => {}
            }
        }

        // Member initializer list for constructors only: : member(arg), ...
        let mut member_init_list = Vec::new();
        let is_constructor = class_path.as_ref().is_some_and(|cp| {
            cp.segments
                .last()
                .is_some_and(|seg| seg.ident.sym == ident.sym)
        });
        if is_constructor && p.eat(TokenKind::Colon).is_some() {
            loop {
                skip_macro_annotations(p)?;
                let member = parse_path(p)?;
                p.expect(TokenKind::LeftParenthese)?;
                let mut args = Punctuated::new();
                while p.peek_kind() != Some(TokenKind::RightParenthese) && !p.is_empty() {
                    let arg = parse_expr_no_comma(p)?;
                    if let Some(comma) = p.eat(TokenKind::Comma) {
                        args.push_pair(arg, comma);
                    } else {
                        args.push_value(arg);
                        break;
                    }
                }
                p.expect(TokenKind::RightParenthese)?;
                member_init_list.push(MemberInit { member, args });
                if p.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
        }

        let sig = Signature {
            constexpr_token,
            consteval_token,
            inline_token,
            virtual_token,
            static_token,
            explicit_token,
            return_type,
            class_path,
            ident,
            inputs,
            variadic: false,
            const_token,
            noexcept_token,
            override_token,
            final_token,
            pure_virtual,
            defaulted,
            deleted,
            destructor_token: false,
            member_init_list,
        };

        // Skip macro annotations between signature and body (e.g., GTEST_LOCK_EXCLUDED_(mutex_))
        while p.peek_kind() == Some(TokenKind::Ident) {
            p.bump()?;
            if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                p.bump()?;
                let mut depth = 1u32;
                while depth > 0 && !p.is_empty() {
                    match p.peek_kind() {
                        Some(TokenKind::LeftParenthese) => {
                            depth += 1;
                            p.bump()?;
                        }
                        Some(TokenKind::RightParenthese) => {
                            depth -= 1;
                            p.bump()?;
                        }
                        _ => {
                            p.bump()?;
                        }
                    }
                }
            }
        }

        // Body or semicolon
        let block = if p.peek_kind() == Some(TokenKind::LeftBrace) {
            Some(parse_block(p)?)
        } else {
            p.expect(TokenKind::Semicolon)?;
            None
        };

        return Ok(Item::Fn(ItemFn {
            attrs: Vec::new(),
            vis: Visibility::Inherited,
            sig,
            block,
        }));
    }

    // Array declarator: type ident[N];
    let mut return_type_for_var = return_type.clone();
    while p.peek_kind() == Some(TokenKind::LeftBracket) {
        p.bump()?;
        let size = if p.peek_kind() != Some(TokenKind::RightBracket) {
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect(TokenKind::RightBracket)?;
        return_type_for_var = Type::Array(TypeArray {
            element: Box::new(return_type_for_var),
            size,
        });
    }
    let return_type = return_type_for_var;

    // Variable declaration: type ident [= expr | {init}] [, ident2 [= expr2]]* ;
    let expr = if p.eat(TokenKind::Equal).is_some() {
        Some(parse_expr_no_comma(p)?)
    } else if p.peek_kind() == Some(TokenKind::LeftBrace) {
        // Brace initialization: type ident{expr};
        p.bump()?;
        let init = if p.peek_kind() != Some(TokenKind::RightBrace) {
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect(TokenKind::RightBrace)?;
        init
    } else {
        None
    };
    // Skip additional declarators in comma-separated list
    while p.eat(TokenKind::Comma).is_some() {
        if p.peek_kind() == Some(TokenKind::Ident) {
            p.bump()?;
        }
        // Array dims
        while p.peek_kind() == Some(TokenKind::LeftBracket) {
            p.bump()?;
            if p.peek_kind() != Some(TokenKind::RightBracket) {
                let _size = parse_expr(p)?;
            }
            p.expect(TokenKind::RightBracket)?;
        }
        if p.eat(TokenKind::Equal).is_some() {
            let _init = parse_expr_no_comma(p)?;
        }
    }
    p.expect(TokenKind::Semicolon)?;

    if static_token {
        Ok(Item::Static(ItemStatic {
            attrs: Vec::new(),
            ty: return_type,
            class_path,
            ident,
            expr,
        }))
    } else if constexpr_token || is_const_type(&return_type) {
        Ok(Item::Const(ItemConst {
            attrs: Vec::new(),
            constexpr_token,
            ty: return_type,
            class_path,
            ident,
            expr: expr.unwrap_or(Expr::Lit(ExprLit {
                span: ident.span,
                kind: LitKind::Integer,
            })),
        }))
    } else {
        // Regular variable declaration (not static, not const)
        Ok(Item::Var(ItemVar {
            attrs: Vec::new(),
            ty: return_type,
            ident,
            expr,
        }))
    }
}

fn is_const_type(ty: &Type) -> bool {
    match ty {
        Type::Qualified(q) => q.cv.const_token,
        Type::Ptr(p) => is_const_type(&p.pointee),
        Type::Reference(r) => is_const_type(&r.referent),
        Type::Array(a) => is_const_type(&a.element),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Function parameters
// ---------------------------------------------------------------------------

fn parse_fn_params<'de>(p: &mut Parser<'de>) -> Result<Punctuated<'de, FnArg<'de>>, AstError> {
    p.expect(TokenKind::LeftParenthese)?;
    let mut params = Punctuated::new();

    // void parameter list
    if p.peek_kind() == Some(TokenKind::KeywordVoid)
        && p.peek_nth_kind(1) == Some(TokenKind::RightParenthese)
    {
        p.bump()?; // consume void
        p.expect(TokenKind::RightParenthese)?;
        return Ok(params);
    }

    while p.peek_kind() != Some(TokenKind::RightParenthese) && !p.is_empty() {
        // Check for variadic ...
        if p.peek_kind() == Some(TokenKind::Ellipsis) {
            p.bump()?;
            break;
        }

        let start = p.peek();
        let cp = p.checkpoint();
        match parse_fn_param(p) {
            Ok((arg, has_comma)) => {
                if let Some(comma) = has_comma {
                    params.push_pair(arg, comma);
                } else {
                    params.push_value(arg);
                    break;
                }
            }
            Err(_) => {
                // Recovery: skip to next comma or close paren (for complex types like
                // pointer-to-member functions: int (Class::*method)() const)
                p.restore(&cp);
                let mut depth = 0u32;
                loop {
                    match p.peek_kind() {
                        None => break,
                        Some(TokenKind::LeftParenthese) => {
                            depth += 1;
                            p.bump()?;
                        }
                        Some(TokenKind::RightParenthese) if depth > 0 => {
                            depth -= 1;
                            p.bump()?;
                        }
                        Some(TokenKind::RightParenthese) => break,
                        Some(TokenKind::Comma) if depth == 0 => {
                            let comma = p.bump()?;
                            let span = p.span_since(start);
                            let arg = FnArg {
                                attrs: Vec::new(),
                                ty: Type::Path(TypePath {
                                    path: Path {
                                        leading_colon: false,
                                        segments: vec![PathSegment {
                                            ident: Ident {
                                                sym: "/*complex*/",
                                                span,
                                            },
                                        }],
                                    },
                                }),
                                ident: None,
                                default_value: None,
                            };
                            params.push_pair(arg, comma);
                            break;
                        }
                        _ => {
                            p.bump()?;
                        }
                    }
                }
                if p.peek_kind() == Some(TokenKind::RightParenthese) && depth == 0 {
                    // End of params with recovery
                    let span = p.span_since(start);
                    let span_range: core::ops::Range<usize> = span.into();
                    if !span_range.is_empty() {
                        params.push_value(FnArg {
                            attrs: Vec::new(),
                            ty: Type::Path(TypePath {
                                path: Path {
                                    leading_colon: false,
                                    segments: vec![PathSegment {
                                        ident: Ident {
                                            sym: "/*complex*/",
                                            span,
                                        },
                                    }],
                                },
                            }),
                            ident: None,
                            default_value: None,
                        });
                    }
                    break;
                }
            }
        }
    }
    p.expect(TokenKind::RightParenthese)?;
    Ok(params)
}

fn parse_fn_param<'de>(p: &mut Parser<'de>) -> Result<(FnArg<'de>, Option<Token<'de>>), AstError> {
    let ty = parse_type(p)?;

    // Handle array declarator after ident: int arr[]
    let ident = if p.peek_kind() == Some(TokenKind::Ident) {
        Some(parse_ident(p)?)
    } else {
        None
    };

    // Handle array dimensions after parameter name: int arr[10]
    let ty = if p.peek_kind() == Some(TokenKind::LeftBracket) {
        let mut arr_ty = ty;
        while p.peek_kind() == Some(TokenKind::LeftBracket) {
            p.bump()?;
            let size = if p.peek_kind() != Some(TokenKind::RightBracket) {
                Some(parse_expr(p)?)
            } else {
                None
            };
            p.expect(TokenKind::RightBracket)?;
            arr_ty = Type::Array(TypeArray {
                element: Box::new(arr_ty),
                size,
            });
        }
        arr_ty
    } else {
        ty
    };

    // If next token is ( and we just got a bare type, it's likely a complex type
    // like pointer-to-member-function: int (Class::*name)(args)
    if p.peek_kind() == Some(TokenKind::LeftParenthese) && ident.is_none() {
        return Err(p.error_at_current("complex parameter type"));
    }

    let default_value = if p.eat(TokenKind::Equal).is_some() {
        Some(parse_expr_no_comma(p)?)
    } else {
        None
    };
    let arg = FnArg {
        attrs: Vec::new(),
        ty,
        ident,
        default_value,
    };
    let comma = p.eat(TokenKind::Comma);
    Ok((arg, comma))
}

// ---------------------------------------------------------------------------
// Base specifiers: public Base, virtual protected Interface
// ---------------------------------------------------------------------------

fn parse_base_specifiers<'de>(p: &mut Parser<'de>) -> Result<Vec<BaseSpecifier<'de>>, AstError> {
    let mut bases = Vec::new();
    loop {
        let mut access = Visibility::Inherited;
        let mut virtual_token = false;

        // Parse access and virtual in any order
        loop {
            match p.peek_kind() {
                Some(TokenKind::KeywordPublic) => {
                    p.bump()?;
                    access = Visibility::Public;
                }
                Some(TokenKind::KeywordProtected) => {
                    p.bump()?;
                    access = Visibility::Protected;
                }
                Some(TokenKind::KeywordPrivate) => {
                    p.bump()?;
                    access = Visibility::Private;
                }
                Some(TokenKind::KeywordVirtual) => {
                    p.bump()?;
                    virtual_token = true;
                }
                _ => break,
            }
        }

        let path = parse_path(p)?;
        // Skip template arguments on base class (e.g., Base<T, U>)
        if p.peek_kind() == Some(TokenKind::LeftChevron) {
            let cp = p.checkpoint();
            if parse_angle_bracketed_args(p).is_err() {
                p.restore(&cp);
            }
        }
        bases.push(BaseSpecifier {
            access,
            virtual_token,
            path,
        });

        if p.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    Ok(bases)
}

// ---------------------------------------------------------------------------
// Fields (class/struct/union body)
// ---------------------------------------------------------------------------

fn parse_fields_named<'de>(
    p: &mut Parser<'de>,
    class_name: Option<&str>,
) -> Result<FieldsNamed<'de>, AstError> {
    p.expect(TokenKind::LeftBrace)?;
    let mut members = Vec::new();
    let mut current_vis = Visibility::Inherited;

    while p.peek_kind() != Some(TokenKind::RightBrace) && !p.is_empty() {
        // Skip attributes inside class body
        let _attrs = parse_attributes(p)?;

        // Destructor: ~ClassName()
        if p.peek_kind() == Some(TokenKind::Compl)
            || (p.peek_kind() == Some(TokenKind::KeywordVirtual)
                && p.peek_nth_kind(1) == Some(TokenKind::Compl))
        {
            let virtual_token = p.eat(TokenKind::KeywordVirtual).is_some();
            p.expect(TokenKind::Compl)?;
            let ident = parse_ident(p)?;
            p.expect(TokenKind::LeftParenthese)?;
            p.expect(TokenKind::RightParenthese)?;
            // Trailing qualifiers: noexcept, override, final
            let mut noexcept_token = false;
            loop {
                match p.peek_kind() {
                    Some(TokenKind::KeywordNoexcept) => {
                        p.bump()?;
                        noexcept_token = true;
                        if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                            p.bump()?;
                            let _e = parse_expr(p)?;
                            p.expect(TokenKind::RightParenthese)?;
                        }
                    }
                    Some(TokenKind::KeywordOverride | TokenKind::KeywordFinal) => {
                        p.bump()?;
                    }
                    _ => break,
                }
            }
            let mut defaulted = false;
            let mut deleted = false;
            let mut pure_virtual = false;
            if p.eat(TokenKind::Equal).is_some() {
                match p.peek_kind() {
                    Some(TokenKind::Number) => {
                        p.bump()?;
                        pure_virtual = true;
                    }
                    Some(TokenKind::KeywordDefault) => {
                        p.bump()?;
                        defaulted = true;
                    }
                    Some(TokenKind::KeywordDelete) => {
                        p.bump()?;
                        deleted = true;
                    }
                    _ => {}
                }
            }
            let block = if p.peek_kind() == Some(TokenKind::LeftBrace) {
                Some(parse_block(p)?)
            } else {
                p.expect(TokenKind::Semicolon)?;
                None
            };
            members.push(Member::Destructor(ItemDestructor {
                attrs: Vec::new(),
                virtual_token,
                ident,
                noexcept_token,
                block,
                defaulted,
                deleted,
                pure_virtual,
            }));
            continue;
        }

        // Constructor: [explicit] [constexpr] ClassName(params)
        if let Some(name) = class_name {
            let mut look = 0;
            let mut explicit_token = false;
            let mut constexpr_token = false;
            // Skip leading specifiers for constructors
            loop {
                match p.peek_nth_kind(look) {
                    Some(TokenKind::KeywordExplicit) => {
                        look += 1;
                        explicit_token = true;
                    }
                    Some(TokenKind::KeywordConstexpr) => {
                        look += 1;
                        constexpr_token = true;
                    }
                    Some(TokenKind::KeywordInline) => {
                        look += 1;
                    }
                    _ => break,
                }
            }
            if p.peek_nth_kind(look) == Some(TokenKind::Ident)
                && p.peek_nth(look).unwrap().src() == name
                && p.peek_nth_kind(look + 1) == Some(TokenKind::LeftParenthese)
            {
                // Consume the specifiers
                for _ in 0..look {
                    p.bump()?;
                }
                let ident = parse_ident(p)?;
                let inputs = parse_fn_params(p)?;
                let noexcept_token = p.eat(TokenKind::KeywordNoexcept).is_some();
                let mut member_init_list = Vec::new();
                let mut defaulted = false;
                let mut deleted = false;

                if p.eat(TokenKind::Colon).is_some() {
                    loop {
                        skip_macro_annotations(p)?;
                        let member = parse_path(p)?;
                        p.expect(TokenKind::LeftParenthese)?;
                        let mut args = Punctuated::new();
                        while p.peek_kind() != Some(TokenKind::RightParenthese) && !p.is_empty() {
                            let arg = parse_expr_no_comma(p)?;
                            if let Some(comma) = p.eat(TokenKind::Comma) {
                                args.push_pair(arg, comma);
                            } else {
                                args.push_value(arg);
                                break;
                            }
                        }
                        p.expect(TokenKind::RightParenthese)?;
                        member_init_list.push(MemberInit { member, args });
                        if p.eat(TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }

                if p.eat(TokenKind::Equal).is_some() {
                    match p.peek_kind() {
                        Some(TokenKind::KeywordDefault) => {
                            p.bump()?;
                            defaulted = true;
                        }
                        Some(TokenKind::KeywordDelete) => {
                            p.bump()?;
                            deleted = true;
                        }
                        _ => {}
                    }
                }

                let block = if p.peek_kind() == Some(TokenKind::LeftBrace) {
                    Some(parse_block(p)?)
                } else {
                    p.expect(TokenKind::Semicolon)?;
                    None
                };

                members.push(Member::Constructor(ItemConstructor {
                    attrs: Vec::new(),
                    explicit_token,
                    constexpr_token,
                    ident,
                    inputs,
                    noexcept_token,
                    member_init_list,
                    block,
                    defaulted,
                    deleted,
                }));
                continue;
            }
        }

        // Access specifier
        match p.peek_kind() {
            Some(TokenKind::KeywordPublic) => {
                p.bump()?;
                p.expect(TokenKind::Colon)?;
                current_vis = Visibility::Public;
                members.push(Member::AccessSpecifier(Visibility::Public));
                continue;
            }
            Some(TokenKind::KeywordProtected) => {
                p.bump()?;
                p.expect(TokenKind::Colon)?;
                current_vis = Visibility::Protected;
                members.push(Member::AccessSpecifier(Visibility::Protected));
                continue;
            }
            Some(TokenKind::KeywordPrivate) => {
                p.bump()?;
                p.expect(TokenKind::Colon)?;
                current_vis = Visibility::Private;
                members.push(Member::AccessSpecifier(Visibility::Private));
                continue;
            }
            Some(TokenKind::KeywordStaticAssert) => {
                let sa = parse_item_static_assert(p)?;
                members.push(Member::StaticAssert(sa));
                continue;
            }
            Some(TokenKind::KeywordUsing) => {
                let item = parse_item_using(p)?;
                match item {
                    Item::Use(u) => members.push(Member::Using(u)),
                    Item::Type(_) => {
                        members.push(Member::Item(Box::new(item)));
                    }
                    _ => {}
                }
                continue;
            }
            Some(TokenKind::KeywordFriend) => {
                p.bump()?;
                // Friend declarations are not emitted, and can carry constructs
                // the item parser doesn't handle (e.g. a templated befriended
                // class `friend class cFoo<A, B>;`). Skip to the terminating
                // `;`, balancing any `{}`/`()` from an inline friend definition.
                let mut brace_depth = 0usize;
                let mut paren_depth = 0usize;
                while let Some(kind) = p.peek_kind() {
                    match kind {
                        TokenKind::LeftBrace => brace_depth += 1,
                        TokenKind::RightBrace => brace_depth = brace_depth.saturating_sub(1),
                        TokenKind::LeftParenthese => paren_depth += 1,
                        TokenKind::RightParenthese => paren_depth = paren_depth.saturating_sub(1),
                        TokenKind::Semicolon if brace_depth == 0 && paren_depth == 0 => {
                            p.bump()?;
                            break;
                        }
                        _ => {}
                    }
                    p.bump()?;
                }
                continue;
            }
            _ => {}
        }

        // Try to parse as a member function or field
        let item = parse_item(p)?;
        match item {
            Item::Fn(f) => members.push(Member::Method(f)),
            Item::Static(s) => {
                members.push(Member::Field(Field {
                    attrs: Vec::new(),
                    vis: current_vis,
                    static_token: true,
                    ty: s.ty,
                    ident: Some(s.ident),
                    default_value: s.expr,
                }));
            }
            Item::Const(c) => {
                members.push(Member::Field(Field {
                    attrs: Vec::new(),
                    vis: current_vis,
                    static_token: false,
                    ty: c.ty,
                    ident: Some(c.ident),
                    default_value: Some(c.expr),
                }));
            }
            Item::Var(v) => {
                members.push(Member::Field(Field {
                    attrs: Vec::new(),
                    vis: current_vis,
                    static_token: false,
                    ty: v.ty,
                    ident: Some(v.ident),
                    default_value: v.expr,
                }));
            }
            other => members.push(Member::Item(Box::new(other))),
        }
    }
    p.expect(TokenKind::RightBrace)?;
    Ok(FieldsNamed { members })
}

// ---------------------------------------------------------------------------
// Block
// ---------------------------------------------------------------------------

fn parse_block<'de>(p: &mut Parser<'de>) -> Result<Block<'de>, AstError> {
    p.expect(TokenKind::LeftBrace)?;
    let mut stmts = Vec::new();
    while p.peek_kind() != Some(TokenKind::RightBrace) && !p.is_empty() {
        stmts.push(parse_stmt(p)?);
    }
    p.expect(TokenKind::RightBrace)?;
    Ok(Block { stmts })
}

// ---------------------------------------------------------------------------
// Statements (simplified — enough to parse function bodies)
// ---------------------------------------------------------------------------

fn parse_stmt<'de>(p: &mut Parser<'de>) -> Result<Stmt<'de>, AstError> {
    // Skip preprocessor directives inside function bodies
    if p.peek_kind() == Some(TokenKind::NumberSign) {
        if p.peek_nth(1)
            .is_some_and(|t| t.src_span().src() == "include")
        {
            return parse_item_include(p).map(|i| Stmt::Item(Item::Include(i)));
        }
        let macro_item = parse_item_macro(p)?;
        return Ok(Stmt::Item(Item::Macro(macro_item)));
    }

    // Empty statement
    if p.eat(TokenKind::Semicolon).is_some() {
        return Ok(Stmt::Empty);
    }

    // Compound statement
    if p.peek_kind() == Some(TokenKind::LeftBrace) {
        let block = parse_block(p)?;
        return Ok(Stmt::Block(block));
    }

    // Return
    if p.eat(TokenKind::KeywordReturn).is_some() {
        let expr = if p.peek_kind() != Some(TokenKind::Semicolon) {
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect(TokenKind::Semicolon)?;
        return Ok(Stmt::Return(StmtReturn { expr }));
    }

    // If (with optional constexpr and init-statement)
    if p.eat(TokenKind::KeywordIf).is_some() {
        let _constexpr = p.eat(TokenKind::KeywordConstexpr).is_some();
        p.expect(TokenKind::LeftParenthese)?;

        // Check for init-statement: scan for ; before matching )
        let init = if has_semicolon_before_close_paren(p) {
            let init_stmt = parse_stmt(p)?;
            Some(Box::new(init_stmt))
        } else {
            None
        };

        let condition = parse_expr(p)?;
        p.expect(TokenKind::RightParenthese)?;
        let then_body = Box::new(parse_stmt(p)?);
        let else_body = if p.eat(TokenKind::KeywordElse).is_some() {
            Some(Box::new(parse_stmt(p)?))
        } else {
            None
        };
        return Ok(Stmt::If(StmtIf {
            init,
            condition,
            then_body,
            else_body,
        }));
    }

    // While
    if p.eat(TokenKind::KeywordWhile).is_some() {
        p.expect(TokenKind::LeftParenthese)?;
        let condition = parse_expr(p)?;
        p.expect(TokenKind::RightParenthese)?;
        let body = Box::new(parse_stmt(p)?);
        return Ok(Stmt::While(StmtWhile { condition, body }));
    }

    // Do-while
    if p.eat(TokenKind::KeywordDo).is_some() {
        let body = Box::new(parse_stmt(p)?);
        p.expect(TokenKind::KeywordWhile)?;
        p.expect(TokenKind::LeftParenthese)?;
        let condition = parse_expr(p)?;
        p.expect(TokenKind::RightParenthese)?;
        p.expect(TokenKind::Semicolon)?;
        return Ok(Stmt::DoWhile(StmtDoWhile { body, condition }));
    }

    // For (regular or range-based)
    if p.eat(TokenKind::KeywordFor).is_some() {
        p.expect(TokenKind::LeftParenthese)?;

        // Try range-based for: type ident : expr  OR  type [id, id] : expr
        // Use parse_type_no_array to avoid consuming `[` as an array suffix,
        // which would conflict with structured bindings like `[k, v]`.
        let cp = p.checkpoint();
        if is_type_start(p.peek_kind()) || p.peek_kind() == Some(TokenKind::Ident) {
            if let Ok(ty) = parse_type_no_array(p) {
                // Single identifier binding
                let binding = if p.peek_kind() == Some(TokenKind::Ident) {
                    let ident = parse_ident(p).unwrap();
                    Some(ForRangeBinding::Ident(ident))
                // Structured binding: [id1, id2, ...]
                } else if p.peek_kind() == Some(TokenKind::LeftBracket) {
                    p.eat(TokenKind::LeftBracket);
                    let mut idents = Vec::new();
                    loop {
                        if p.peek_kind() == Some(TokenKind::RightBracket) {
                            break;
                        }
                        if p.peek_kind() == Some(TokenKind::Ident) {
                            idents.push(parse_ident(p).unwrap());
                        } else {
                            break;
                        }
                        if p.eat(TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                    if p.eat(TokenKind::RightBracket).is_some() && !idents.is_empty() {
                        Some(ForRangeBinding::Structured(idents))
                    } else {
                        None
                    }
                } else {
                    None
                };

                if let Some(binding) = binding
                    && p.eat(TokenKind::Colon).is_some()
                {
                    let range = parse_expr(p)?;
                    p.expect(TokenKind::RightParenthese)?;
                    let body = Box::new(parse_stmt(p)?);
                    return Ok(Stmt::ForRange(StmtForRange {
                        ty,
                        binding,
                        range,
                        body,
                    }));
                }
            }
            p.restore(&cp);
        }

        // Regular for: try local variable init
        let init = if p.peek_kind() != Some(TokenKind::Semicolon) {
            let cp2 = p.checkpoint();
            if let Some(local) = try_parse_local(p) {
                // StmtLocal already consumed the semicolon
                Some(Box::new(Stmt::Local(local)))
            } else {
                p.restore(&cp2);
                let expr = parse_expr(p)?;
                Some(Box::new(Stmt::Expr(StmtExpr { expr })))
            }
        } else {
            None
        };
        if !matches!(init.as_deref(), Some(Stmt::Local(_))) {
            p.expect(TokenKind::Semicolon)?;
        }
        let condition = if p.peek_kind() != Some(TokenKind::Semicolon) {
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect(TokenKind::Semicolon)?;
        let increment = if p.peek_kind() != Some(TokenKind::RightParenthese) {
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect(TokenKind::RightParenthese)?;
        let body = Box::new(parse_stmt(p)?);
        return Ok(Stmt::For(StmtFor {
            init,
            condition,
            increment,
            body,
        }));
    }

    // Switch
    if p.eat(TokenKind::KeywordSwitch).is_some() {
        p.expect(TokenKind::LeftParenthese)?;
        let expr = parse_expr(p)?;
        p.expect(TokenKind::RightParenthese)?;
        let body = parse_block(p)?;
        return Ok(Stmt::Switch(StmtSwitch { expr, body }));
    }

    // Case
    if p.eat(TokenKind::KeywordCase).is_some() {
        let value = parse_expr_no_comma(p)?;
        p.expect(TokenKind::Colon)?;
        return Ok(Stmt::Case(StmtCase { value }));
    }

    // Default (with lookahead for : to distinguish from = default)
    if p.peek_kind() == Some(TokenKind::KeywordDefault)
        && p.peek_nth_kind(1) == Some(TokenKind::Colon)
    {
        let tok = p.bump()?;
        p.expect(TokenKind::Colon)?;
        return Ok(Stmt::Default(StmtDefault {
            span: tok.src_span(),
        }));
    }

    // Break
    if p.eat(TokenKind::KeywordBreak).is_some() {
        let span = p.last_span();
        p.expect(TokenKind::Semicolon)?;
        return Ok(Stmt::Break(StmtBreak { span }));
    }

    // Continue
    if p.eat(TokenKind::KeywordContinue).is_some() {
        let span = p.last_span();
        p.expect(TokenKind::Semicolon)?;
        return Ok(Stmt::Continue(StmtContinue { span }));
    }

    // Goto
    if p.eat(TokenKind::KeywordGoto).is_some() {
        let label = parse_ident(p)?;
        p.expect(TokenKind::Semicolon)?;
        return Ok(Stmt::Goto(StmtGoto { label }));
    }

    // Try/catch
    if p.eat(TokenKind::KeywordTry).is_some() {
        let try_body = parse_block(p)?;
        let mut catches = Vec::new();
        while p.eat(TokenKind::KeywordCatch).is_some() {
            p.expect(TokenKind::LeftParenthese)?;
            let param = if p.eat(TokenKind::Ellipsis).is_some() {
                CatchParam::Ellipsis
            } else {
                let ty = parse_type(p)?;
                let ident = if p.peek_kind() == Some(TokenKind::Ident) {
                    Some(parse_ident(p)?)
                } else {
                    None
                };
                CatchParam::Typed { ty, ident }
            };
            p.expect(TokenKind::RightParenthese)?;
            let body = parse_block(p)?;
            catches.push(CatchClause { param, body });
        }
        return Ok(Stmt::TryCatch(StmtTryCatch { try_body, catches }));
    }

    // Label: ident followed by : (but not ::)
    if p.peek_kind() == Some(TokenKind::Ident)
        && p.peek_nth_kind(1) == Some(TokenKind::Colon)
        && p.peek_nth_kind(2) != Some(TokenKind::Colon)
    {
        // Check it's not just an expression like ternary a ? b : c
        // Labels only have a single ident before :
        let label = parse_ident(p)?;
        p.expect(TokenKind::Colon)?;
        return Ok(Stmt::Label(StmtLabel { label }));
    }

    // Try local variable declaration (before expression statement)
    let cp = p.checkpoint();
    if let Some(local) = try_parse_local(p) {
        return Ok(Stmt::Local(local));
    }
    p.restore(&cp);

    // Expression statement
    let cp = p.checkpoint();
    match parse_expr(p) {
        Ok(expr) => {
            if p.peek_kind() == Some(TokenKind::LeftBrace) {
                // Expression followed by block: likely a macro/extension with body
                // Skip the block
                let _block = parse_block(p)?;
                // Check for trailing constructs like __except(...)
                while p.peek_kind() == Some(TokenKind::Ident) {
                    p.bump()?;
                    if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                        p.bump()?;
                        let mut depth = 1u32;
                        while depth > 0 && !p.is_empty() {
                            match p.peek_kind() {
                                Some(TokenKind::LeftParenthese) => {
                                    depth += 1;
                                    p.bump()?;
                                }
                                Some(TokenKind::RightParenthese) => {
                                    depth -= 1;
                                    p.bump()?;
                                }
                                _ => {
                                    p.bump()?;
                                }
                            }
                        }
                    }
                    if p.peek_kind() == Some(TokenKind::LeftBrace) {
                        let _block = parse_block(p)?;
                    } else {
                        break;
                    }
                }
                return Ok(Stmt::Empty);
            }
            p.expect(TokenKind::Semicolon)?;
            Ok(Stmt::Expr(StmtExpr { expr }))
        }
        Err(_) => {
            // Recovery: skip to next semicolon or closing brace
            p.restore(&cp);
            let mut depth = 0u32;
            loop {
                match p.peek_kind() {
                    None => return Err(p.eof_error("statement")),
                    Some(TokenKind::LeftBrace) => {
                        depth += 1;
                        p.bump()?;
                    }
                    Some(TokenKind::RightBrace) if depth > 0 => {
                        depth -= 1;
                        p.bump()?;
                    }
                    Some(TokenKind::RightBrace) => break,
                    Some(TokenKind::Semicolon) if depth == 0 => {
                        p.bump()?;
                        break;
                    }
                    _ => {
                        p.bump()?;
                    }
                }
            }
            Ok(Stmt::Empty)
        }
    }
}

/// Check if there is a semicolon before the matching close parenthesis at depth 1.
fn has_semicolon_before_close_paren(p: &Parser) -> bool {
    let mut depth = 1u32;
    let mut lexer = p.lexer.clone();
    while let Some(tok) = next_non_comment(&mut lexer) {
        match tok.kind() {
            TokenKind::LeftParenthese => depth += 1,
            TokenKind::RightParenthese => {
                depth -= 1;
                if depth == 0 {
                    return false;
                }
            }
            TokenKind::Semicolon if depth == 1 => return true,
            _ => {}
        }
    }
    false
}

/// Try to parse a local variable declaration: type ident [= expr] ;
/// Returns None if this doesn't look like a local declaration.
fn try_parse_local<'de>(p: &mut Parser<'de>) -> Option<StmtLocal<'de>> {
    let cp = p.checkpoint();
    let ty = parse_type(p).ok()?;

    // After type, we need an identifier
    if p.peek_kind() != Some(TokenKind::Ident) {
        p.restore(&cp);
        return None;
    }
    let ident = parse_ident(p).ok()?;

    // Must be followed by = ; { ( [ or , to be a local variable
    let (final_ty, init) = match p.peek_kind() {
        Some(TokenKind::Equal) => {
            p.bump().ok()?;
            (ty, Some(parse_expr_no_comma(p).ok()?))
        }
        Some(TokenKind::Semicolon | TokenKind::Comma) => (ty, None),
        Some(TokenKind::LeftParenthese) => {
            // Constructor-style init: Type var(args)
            p.bump().ok()?;
            let init_expr = parse_expr(p).ok()?;
            p.expect(TokenKind::RightParenthese).ok()?;
            (ty, Some(init_expr))
        }
        Some(TokenKind::LeftBrace) => {
            let init_expr = parse_expr(p).ok()?;
            (ty, Some(init_expr))
        }
        Some(TokenKind::LeftBracket) => {
            // Array declarator: Type var[N] = init;
            let mut arr_ty = ty;
            while p.peek_kind() == Some(TokenKind::LeftBracket) {
                p.bump().ok()?;
                let size = if p.peek_kind() != Some(TokenKind::RightBracket) {
                    Some(parse_expr(p).ok()?)
                } else {
                    None
                };
                p.expect(TokenKind::RightBracket).ok()?;
                arr_ty = Type::Array(TypeArray {
                    element: Box::new(arr_ty),
                    size,
                });
            }
            let arr_init = if p.eat(TokenKind::Equal).is_some() {
                Some(parse_expr_no_comma(p).ok()?)
            } else {
                None
            };
            (arr_ty, arr_init)
        }
        _ => {
            p.restore(&cp);
            return None;
        }
    };

    // Skip additional comma-separated declarators
    while p.eat(TokenKind::Comma).is_some() {
        skip_declarator(p);
    }
    p.expect(TokenKind::Semicolon).ok()?;
    Some(StmtLocal {
        ty: final_ty,
        ident,
        init,
    })
}

/// Skip a single declarator in a comma-separated declaration list
fn skip_declarator(p: &mut Parser) {
    // Skip pointer/reference decorations
    while matches!(
        p.peek_kind(),
        Some(TokenKind::Star | TokenKind::BitAnd | TokenKind::And)
    ) {
        p.bump().ok();
    }
    // Skip identifier
    if p.peek_kind() == Some(TokenKind::Ident) {
        p.bump().ok();
    }
    // Skip array dimensions
    while p.peek_kind() == Some(TokenKind::LeftBracket) {
        p.bump().ok();
        let mut depth = 1u32;
        while depth > 0 {
            match p.peek_kind() {
                Some(TokenKind::LeftBracket) => {
                    depth += 1;
                    p.bump().ok();
                }
                Some(TokenKind::RightBracket) => {
                    depth -= 1;
                    p.bump().ok();
                }
                None => break,
                _ => {
                    p.bump().ok();
                }
            }
        }
    }
    // Skip initializer
    if p.eat(TokenKind::Equal).is_some() {
        let _ = parse_expr_no_comma(p);
    } else if p.peek_kind() == Some(TokenKind::LeftParenthese) {
        p.bump().ok();
        let mut depth = 1u32;
        while depth > 0 {
            match p.peek_kind() {
                Some(TokenKind::LeftParenthese) => {
                    depth += 1;
                    p.bump().ok();
                }
                Some(TokenKind::RightParenthese) => {
                    depth -= 1;
                    p.bump().ok();
                }
                None => break,
                _ => {
                    p.bump().ok();
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Type parsing
// ---------------------------------------------------------------------------

fn is_type_start(kind: Option<TokenKind>) -> bool {
    matches!(
        kind,
        Some(
            TokenKind::KeywordVoid
                | TokenKind::KeywordBool
                | TokenKind::KeywordChar
                | TokenKind::KeywordChar8
                | TokenKind::KeywordChar16
                | TokenKind::KeywordChar32
                | TokenKind::KeywordWchar
                | TokenKind::KeywordFloat
                | TokenKind::KeywordDouble
                | TokenKind::KeywordInt
                | TokenKind::KeywordShort
                | TokenKind::KeywordLong
                | TokenKind::KeywordSigned
                | TokenKind::KeywordUnsigned
                | TokenKind::KeywordAuto
                | TokenKind::KeywordDecltype
                | TokenKind::KeywordConst
                | TokenKind::KeywordVolatile
                | TokenKind::KeywordStruct
                | TokenKind::KeywordClass
                | TokenKind::KeywordEnum
        )
    )
}

fn parse_type<'de>(p: &mut Parser<'de>) -> Result<Type<'de>, AstError> {
    // CV qualifiers
    let mut cv = CvQualifiers::default();
    while let Some(kind) = p.peek_kind() {
        match kind {
            TokenKind::KeywordConst => {
                p.bump()?;
                cv.const_token = true;
            }
            TokenKind::KeywordVolatile => {
                p.bump()?;
                cv.volatile_token = true;
            }
            _ => break,
        }
    }

    // Base type
    let base = parse_base_type(p)?;

    // Wrap with CV qualifiers if needed
    let qualified = if cv.const_token || cv.volatile_token {
        Type::Qualified(TypeQualified {
            cv,
            ty: Box::new(base),
        })
    } else {
        base
    };

    // Pointer/reference suffixes
    parse_type_suffix(p, qualified)
}

/// Like `parse_type` but stops before array brackets, so `[` is not consumed.
/// Used in range-based for to avoid ambiguity with structured bindings.
fn parse_type_no_array<'de>(p: &mut Parser<'de>) -> Result<Type<'de>, AstError> {
    let mut cv = CvQualifiers::default();
    while let Some(kind) = p.peek_kind() {
        match kind {
            TokenKind::KeywordConst => {
                p.bump()?;
                cv.const_token = true;
            }
            TokenKind::KeywordVolatile => {
                p.bump()?;
                cv.volatile_token = true;
            }
            _ => break,
        }
    }
    let base = parse_base_type(p)?;
    let qualified = if cv.const_token || cv.volatile_token {
        Type::Qualified(TypeQualified {
            cv,
            ty: Box::new(base),
        })
    } else {
        base
    };
    parse_type_ptr_ref_suffix(p, qualified)
}

fn parse_base_type<'de>(p: &mut Parser<'de>) -> Result<Type<'de>, AstError> {
    match p.peek_kind() {
        Some(TokenKind::KeywordVoid) => {
            let tok = p.bump()?;
            Ok(Type::Fundamental(TypeFundamental {
                span: tok.src_span(),
                kind: FundamentalKind::Void,
            }))
        }
        Some(TokenKind::KeywordBool) => {
            let tok = p.bump()?;
            Ok(Type::Fundamental(TypeFundamental {
                span: tok.src_span(),
                kind: FundamentalKind::Bool,
            }))
        }
        Some(TokenKind::KeywordChar) => {
            let tok = p.bump()?;
            Ok(Type::Fundamental(TypeFundamental {
                span: tok.src_span(),
                kind: FundamentalKind::Char,
            }))
        }
        Some(TokenKind::KeywordChar8) => {
            let tok = p.bump()?;
            Ok(Type::Fundamental(TypeFundamental {
                span: tok.src_span(),
                kind: FundamentalKind::Char8,
            }))
        }
        Some(TokenKind::KeywordChar16) => {
            let tok = p.bump()?;
            Ok(Type::Fundamental(TypeFundamental {
                span: tok.src_span(),
                kind: FundamentalKind::Char16,
            }))
        }
        Some(TokenKind::KeywordChar32) => {
            let tok = p.bump()?;
            Ok(Type::Fundamental(TypeFundamental {
                span: tok.src_span(),
                kind: FundamentalKind::Char32,
            }))
        }
        Some(TokenKind::KeywordWchar) => {
            let tok = p.bump()?;
            Ok(Type::Fundamental(TypeFundamental {
                span: tok.src_span(),
                kind: FundamentalKind::Wchar,
            }))
        }
        Some(TokenKind::KeywordFloat) => {
            let tok = p.bump()?;
            Ok(Type::Fundamental(TypeFundamental {
                span: tok.src_span(),
                kind: FundamentalKind::Float,
            }))
        }
        Some(TokenKind::KeywordDouble) => {
            let tok = p.bump()?;
            Ok(Type::Fundamental(TypeFundamental {
                span: tok.src_span(),
                kind: FundamentalKind::Double,
            }))
        }
        Some(TokenKind::KeywordAuto) => {
            let tok = p.bump()?;
            Ok(Type::Auto(TypeAuto {
                span: tok.src_span(),
            }))
        }
        Some(TokenKind::KeywordDecltype) => {
            p.bump()?;
            p.expect(TokenKind::LeftParenthese)?;
            let expr = parse_expr(p)?;
            p.expect(TokenKind::RightParenthese)?;
            Ok(Type::Decltype(TypeDecltype { expr }))
        }
        // int, short, long, signed, unsigned — may combine
        Some(
            TokenKind::KeywordInt
            | TokenKind::KeywordShort
            | TokenKind::KeywordLong
            | TokenKind::KeywordSigned
            | TokenKind::KeywordUnsigned,
        ) => parse_integer_type(p),
        // Named type (user-defined or qualified)
        Some(TokenKind::Ident) | Some(TokenKind::DoubleColon) => {
            let path = parse_path(p)?;
            try_parse_template_inst(p, path)
        }
        // struct/class/enum as type specifier: struct Foo
        Some(TokenKind::KeywordStruct | TokenKind::KeywordClass | TokenKind::KeywordEnum) => {
            p.bump()?; // skip struct/class/enum keyword
            let path = parse_path(p)?;
            try_parse_template_inst(p, path)
        }
        _ => Err(p.error_at_current("expected type")),
    }
}

/// After parsing a path, try to parse `<args>` for a template instantiation type.
fn try_parse_template_inst<'de>(
    p: &mut Parser<'de>,
    path: Path<'de>,
) -> Result<Type<'de>, AstError> {
    if p.peek_kind() == Some(TokenKind::LeftChevron) {
        let cp = p.checkpoint();
        if let Ok(args) = parse_angle_bracketed_args(p) {
            // Check if :: follows for further qualification: vector<int>::iterator
            if p.peek_kind() == Some(TokenKind::DoubleColon)
                && p.peek_nth_kind(1) == Some(TokenKind::Ident)
            {
                p.bump()?; // ::
                let suffix_path = parse_path(p)?;
                // Create a path that represents the full qualified type
                // For simplicity, use TypePath with a combined path
                return try_parse_template_inst(p, suffix_path);
            }
            return Ok(Type::TemplateInst(TypeTemplateInst {
                path,
                args: args.args,
            }));
        }
        p.restore(&cp);
    }
    Ok(Type::Path(TypePath { path }))
}

/// Parse `<type_or_expr, type_or_expr, ...>` for template arguments.
fn parse_angle_bracketed_args<'de>(
    p: &mut Parser<'de>,
) -> Result<AngleBracketedArgs<'de>, AstError> {
    p.expect(TokenKind::LeftChevron)?;
    let mut args = Vec::new();
    if p.peek_kind() == Some(TokenKind::RightChevron) {
        p.bump()?;
        return Ok(AngleBracketedArgs { args });
    }
    loop {
        // Try type first, fallback to expression
        let cp = p.checkpoint();
        if let Ok(ty) = parse_type(p) {
            // Check if followed by , or > or >>
            match p.peek_kind() {
                Some(TokenKind::Comma)
                | Some(TokenKind::RightChevron)
                | Some(TokenKind::ShiftRight) => {
                    args.push(TemplateArg::Type(ty));
                }
                _ => {
                    // Not a type arg, try as expression
                    p.restore(&cp);
                    let expr = parse_expr_no_angle(p)?;
                    args.push(TemplateArg::Expr(expr));
                }
            }
        } else {
            p.restore(&cp);
            let expr = parse_expr_no_angle(p)?;
            args.push(TemplateArg::Expr(expr));
        }

        match p.peek_kind() {
            Some(TokenKind::Comma) => {
                p.bump()?;
            }
            Some(TokenKind::RightChevron) => {
                p.bump()?;
                break;
            }
            Some(TokenKind::ShiftRight) => {
                // >> closing two template levels — split into two logical `>`s.
                // Consume the `>>` token, then push a synthetic `>` back so the
                // outer template parser sees it.
                let tok = p.bump()?;
                let range: core::ops::Range<usize> = tok.src_span().into();
                let right_span = SourceSpan::new(tok.src_span().full_source(), range.start + 1, 1);
                p.pending_right_chevron = Some(Token::new(right_span, TokenKind::RightChevron));
                break;
            }
            _ => return Err(p.error_at_current("expected `,` or `>` in template arguments")),
        }
    }
    Ok(AngleBracketedArgs { args })
}

fn parse_integer_type<'de>(p: &mut Parser<'de>) -> Result<Type<'de>, AstError> {
    let start = p.peek();
    let mut _has_signed = false;
    let mut has_unsigned = false;
    let mut has_short = false;
    let mut has_int = false;
    let mut long_count = 0u8;

    loop {
        match p.peek_kind() {
            Some(TokenKind::KeywordSigned) => {
                p.bump()?;
                _has_signed = true;
            }
            Some(TokenKind::KeywordUnsigned) => {
                p.bump()?;
                has_unsigned = true;
            }
            Some(TokenKind::KeywordShort) => {
                p.bump()?;
                has_short = true;
            }
            Some(TokenKind::KeywordLong) => {
                p.bump()?;
                long_count += 1;
            }
            Some(TokenKind::KeywordInt) => {
                p.bump()?;
                has_int = true;
            }
            Some(TokenKind::KeywordChar) if has_unsigned || _has_signed => {
                p.bump()?;
                let kind = if has_unsigned {
                    FundamentalKind::UnsignedChar
                } else {
                    FundamentalKind::SignedChar
                };
                return Ok(Type::Fundamental(TypeFundamental {
                    span: p.span_since(start),
                    kind,
                }));
            }
            Some(TokenKind::KeywordDouble) if long_count > 0 => {
                let _tok = p.bump()?;
                return Ok(Type::Fundamental(TypeFundamental {
                    span: p.span_since(start),
                    kind: FundamentalKind::LongDouble,
                }));
            }
            _ => break,
        }
    }

    let kind = if has_unsigned {
        match (has_short, long_count, has_int) {
            (true, _, _) => FundamentalKind::UnsignedShort,
            (_, 2, _) => FundamentalKind::UnsignedLongLong,
            (_, 1, _) => FundamentalKind::UnsignedLong,
            _ => FundamentalKind::UnsignedInt,
        }
    } else if has_short {
        FundamentalKind::Short
    } else if long_count >= 2 {
        FundamentalKind::LongLong
    } else if long_count == 1 {
        FundamentalKind::Long
    } else {
        FundamentalKind::Int
    };

    let span = p.span_since(start);
    Ok(Type::Fundamental(TypeFundamental { span, kind }))
}

/// Parse pointer and reference suffixes only (no array brackets).
fn parse_type_ptr_ref_suffix<'de>(
    p: &mut Parser<'de>,
    mut ty: Type<'de>,
) -> Result<Type<'de>, AstError> {
    loop {
        match p.peek_kind() {
            Some(TokenKind::Star) => {
                p.bump()?;
                let mut cv = CvQualifiers::default();
                loop {
                    match p.peek_kind() {
                        Some(TokenKind::KeywordConst) => {
                            p.bump()?;
                            cv.const_token = true;
                        }
                        Some(TokenKind::KeywordVolatile) => {
                            p.bump()?;
                            cv.volatile_token = true;
                        }
                        _ => break,
                    }
                }
                ty = Type::Ptr(TypePtr {
                    cv,
                    pointee: Box::new(ty),
                });
            }
            Some(TokenKind::BitAnd) => {
                p.bump()?;
                ty = Type::Reference(TypeReference {
                    cv: CvQualifiers::default(),
                    referent: Box::new(ty),
                });
            }
            Some(TokenKind::And) => {
                p.bump()?;
                ty = Type::RvalueReference(TypeRvalueReference {
                    referent: Box::new(ty),
                });
            }
            _ => break,
        }
    }
    Ok(ty)
}

fn parse_type_suffix<'de>(p: &mut Parser<'de>, ty: Type<'de>) -> Result<Type<'de>, AstError> {
    let mut ty = parse_type_ptr_ref_suffix(p, ty)?;
    while let Some(TokenKind::LeftBracket) = p.peek_kind() {
        // Array type: T[N] or T[]
        p.bump()?;
        let size = if p.peek_kind() != Some(TokenKind::RightBracket) {
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect(TokenKind::RightBracket)?;
        ty = Type::Array(TypeArray {
            element: Box::new(ty),
            size,
        });
    }

    Ok(ty)
}

// ---------------------------------------------------------------------------
// Macro annotation skipping
// ---------------------------------------------------------------------------

/// Skip macro-like annotations and preprocessor directives.
/// Handles ALL_CAPS identifiers optionally followed by `(...)` and `#ifdef`/`#endif` etc.
fn skip_macro_annotations<'de>(p: &mut Parser<'de>) -> Result<(), AstError> {
    loop {
        match p.peek_kind() {
            Some(TokenKind::NumberSign) => {
                parse_item_macro(p)?;
            }
            Some(TokenKind::Ident) => {
                let src = p.peek().unwrap().src();
                let is_macro_like = src.len() > 1
                    && src.contains('_')
                    && src
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit());
                if !is_macro_like {
                    break;
                }
                p.bump()?;
                if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                    p.bump()?;
                    let mut depth = 1u32;
                    while depth > 0 && !p.is_empty() {
                        match p.peek_kind() {
                            Some(TokenKind::LeftParenthese) => {
                                depth += 1;
                                p.bump()?;
                            }
                            Some(TokenKind::RightParenthese) => {
                                depth -= 1;
                                p.bump()?;
                            }
                            _ => {
                                p.bump()?;
                            }
                        }
                    }
                }
            }
            _ => break,
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Path: ident (:: ident)*  or  :: ident (:: ident)*
// ---------------------------------------------------------------------------

fn parse_path<'de>(p: &mut Parser<'de>) -> Result<Path<'de>, AstError> {
    let leading_colon = p.eat(TokenKind::DoubleColon).is_some();

    let first = parse_ident(p)?;
    let mut segments = vec![PathSegment { ident: first }];

    loop {
        if p.peek_kind() != Some(TokenKind::DoubleColon) {
            break;
        }
        // Check if what follows :: is an identifier
        if p.peek_nth_kind(1) != Some(TokenKind::Ident) {
            break;
        }
        p.bump()?; // consume ::
        let seg = parse_ident(p)?;
        segments.push(PathSegment { ident: seg });
    }

    Ok(Path {
        leading_colon,
        segments,
    })
}

// ---------------------------------------------------------------------------
// Ident
// ---------------------------------------------------------------------------

fn parse_ident<'de>(p: &mut Parser<'de>) -> Result<Ident<'de>, AstError> {
    let tok = p.expect(TokenKind::Ident)?;
    Ok(Ident {
        sym: tok.src(),
        span: tok.src_span(),
    })
}

// ---------------------------------------------------------------------------
// Expression parsing (Pratt parser)
// ---------------------------------------------------------------------------

fn parse_expr<'de>(p: &mut Parser<'de>) -> Result<Expr<'de>, AstError> {
    parse_expr_precedence(p, 0, false)
}

fn parse_expr_no_comma<'de>(p: &mut Parser<'de>) -> Result<Expr<'de>, AstError> {
    parse_expr_precedence(p, 1, false)
}

/// Parse an expression that stops at `>` and `>>` (used inside template argument lists).
fn parse_expr_no_angle<'de>(p: &mut Parser<'de>) -> Result<Expr<'de>, AstError> {
    parse_expr_precedence(p, 1, true)
}

fn parse_expr_precedence<'de>(
    p: &mut Parser<'de>,
    min_prec: u8,
    stop_at_angle: bool,
) -> Result<Expr<'de>, AstError> {
    let mut lhs = parse_expr_prefix(p)?;

    loop {
        // Postfix operators
        match p.peek_kind() {
            // Preprocessor directive inside expression (e.g., #if in string concatenation)
            Some(TokenKind::NumberSign) => {
                let _macro = parse_item_macro(p)?;
                continue;
            }
            // String concatenation with macro: MACRO "string" or expr "string"
            Some(TokenKind::String) => {
                // Treat adjacent string as concatenation (macro expanding to string)
                let tok = p.bump()?;
                let mut span = tok.src_span();
                // Consume additional strings and macros
                loop {
                    if p.peek_kind() == Some(TokenKind::String)
                        || (p.peek_kind() == Some(TokenKind::Ident)
                            && p.peek_nth_kind(1) == Some(TokenKind::String))
                    {
                        let next = p.bump()?;
                        let next_range: core::ops::Range<usize> = next.src_span().into();
                        let start_range: core::ops::Range<usize> = span.into();
                        span = SourceSpan::new(
                            p.src,
                            start_range.start,
                            next_range.end - start_range.start,
                        );
                    } else {
                        break;
                    }
                }
                lhs = Expr::Lit(ExprLit {
                    span,
                    kind: LitKind::String,
                });
                continue;
            }
            Some(TokenKind::LeftParenthese) => {
                p.bump()?;
                let mut args = Punctuated::new();
                while p.peek_kind() != Some(TokenKind::RightParenthese) && !p.is_empty() {
                    let arg = parse_expr_no_comma(p)?;
                    if let Some(comma) = p.eat(TokenKind::Comma) {
                        args.push_pair(arg, comma);
                    } else {
                        args.push_value(arg);
                        break;
                    }
                }
                p.expect(TokenKind::RightParenthese)?;
                lhs = Expr::Call(ExprCall {
                    func: Box::new(lhs),
                    args,
                });
                continue;
            }
            Some(TokenKind::LeftBracket) => {
                p.bump()?;
                let index = parse_expr(p)?;
                p.expect(TokenKind::RightBracket)?;
                lhs = Expr::Index(ExprIndex {
                    object: Box::new(lhs),
                    index: Box::new(index),
                });
                continue;
            }
            Some(TokenKind::Increment) => {
                p.bump()?;
                lhs = Expr::Unary(ExprUnary {
                    op: UnaryOp::PostIncrement,
                    operand: Box::new(lhs),
                });
                continue;
            }
            Some(TokenKind::Decrement) => {
                p.bump()?;
                lhs = Expr::Unary(ExprUnary {
                    op: UnaryOp::PostDecrement,
                    operand: Box::new(lhs),
                });
                continue;
            }
            Some(TokenKind::Dot) => {
                p.bump()?;
                let member = parse_ident(p)?;
                // Explicit template arguments on a method call, e.g.
                // `obj.method<T>(...)`. The turbofish is discarded (method calls
                // don't retain template args); only recognized when it parses as
                // angle-bracketed args and is immediately followed by `(`, so a
                // genuine comparison like `a.b < c` is left untouched.
                if p.peek_kind() == Some(TokenKind::LeftChevron) {
                    let cp = p.checkpoint();
                    match parse_angle_bracketed_args(p) {
                        Ok(_) if p.peek_kind() == Some(TokenKind::LeftParenthese) => {}
                        _ => p.restore(&cp),
                    }
                }
                if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                    p.bump()?;
                    let mut args = Punctuated::new();
                    while p.peek_kind() != Some(TokenKind::RightParenthese) && !p.is_empty() {
                        let arg = parse_expr_no_comma(p)?;
                        if let Some(comma) = p.eat(TokenKind::Comma) {
                            args.push_pair(arg, comma);
                        } else {
                            args.push_value(arg);
                            break;
                        }
                    }
                    p.expect(TokenKind::RightParenthese)?;
                    lhs = Expr::MethodCall(ExprMethodCall {
                        receiver: Box::new(lhs),
                        arrow: false,
                        method: member,
                        args,
                    });
                } else {
                    lhs = Expr::Field(ExprField {
                        object: Box::new(lhs),
                        arrow: false,
                        member,
                    });
                }
                continue;
            }
            Some(TokenKind::PointerMember) => {
                p.bump()?;
                let member = parse_ident(p)?;
                // Explicit template arguments on a method call, e.g.
                // `ptr->method<T>(...)`. The turbofish is discarded (method calls
                // don't retain template args); only recognized when it parses as
                // angle-bracketed args and is immediately followed by `(`, so a
                // genuine comparison like `a->b < c` is left untouched.
                if p.peek_kind() == Some(TokenKind::LeftChevron) {
                    let cp = p.checkpoint();
                    match parse_angle_bracketed_args(p) {
                        Ok(_) if p.peek_kind() == Some(TokenKind::LeftParenthese) => {}
                        _ => p.restore(&cp),
                    }
                }
                if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                    p.bump()?;
                    let mut args = Punctuated::new();
                    while p.peek_kind() != Some(TokenKind::RightParenthese) && !p.is_empty() {
                        let arg = parse_expr_no_comma(p)?;
                        if let Some(comma) = p.eat(TokenKind::Comma) {
                            args.push_pair(arg, comma);
                        } else {
                            args.push_value(arg);
                            break;
                        }
                    }
                    p.expect(TokenKind::RightParenthese)?;
                    lhs = Expr::MethodCall(ExprMethodCall {
                        receiver: Box::new(lhs),
                        arrow: true,
                        method: member,
                        args,
                    });
                } else {
                    lhs = Expr::Field(ExprField {
                        object: Box::new(lhs),
                        arrow: true,
                        member,
                    });
                }
                continue;
            }
            // Scope resolution as member access: expr::member (for template instantiation paths)
            Some(TokenKind::DoubleColon) if p.peek_nth_kind(1) == Some(TokenKind::Ident) => {
                p.bump()?;
                let member = parse_ident(p)?;
                lhs = Expr::Field(ExprField {
                    object: Box::new(lhs),
                    arrow: false,
                    member,
                });
                continue;
            }
            _ => {}
        }

        // Ternary
        if min_prec <= 1 && p.peek_kind() == Some(TokenKind::Ternary) {
            p.bump()?;
            let then_expr = parse_expr_precedence(p, 0, stop_at_angle)?;
            p.expect(TokenKind::Colon)?;
            let else_expr = parse_expr_precedence(p, 1, stop_at_angle)?;
            lhs = Expr::Conditional(ExprConditional {
                condition: Box::new(lhs),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            });
            continue;
        }

        // Binary operators
        let Some(op) = peek_binary_op(p) else {
            break;
        };
        // In template argument context, > and >> close the template, not compare
        if stop_at_angle
            && matches!(
                op,
                BinaryOp::Greater
                    | BinaryOp::GreaterEqual
                    | BinaryOp::ShiftRight
                    | BinaryOp::ShiftRightAssign
            )
        {
            break;
        }
        let (prec, right_assoc) = binary_op_precedence(op);
        if prec < min_prec {
            break;
        }
        p.bump()?;
        let next_min = if right_assoc { prec } else { prec + 1 };
        let rhs = parse_expr_precedence(p, next_min, stop_at_angle)?;
        lhs = Expr::Binary(ExprBinary {
            lhs: Box::new(lhs),
            op,
            rhs: Box::new(rhs),
        });
    }

    Ok(lhs)
}

fn parse_expr_prefix<'de>(p: &mut Parser<'de>) -> Result<Expr<'de>, AstError> {
    match p.peek_kind() {
        Some(TokenKind::Minus) => {
            p.bump()?;
            let operand = parse_expr_prefix(p)?;
            Ok(Expr::Unary(ExprUnary {
                op: UnaryOp::Negate,
                operand: Box::new(operand),
            }))
        }
        Some(TokenKind::Plus) => {
            p.bump()?;
            let operand = parse_expr_prefix(p)?;
            Ok(Expr::Unary(ExprUnary {
                op: UnaryOp::Plus,
                operand: Box::new(operand),
            }))
        }
        Some(TokenKind::Not) => {
            p.bump()?;
            let operand = parse_expr_prefix(p)?;
            Ok(Expr::Unary(ExprUnary {
                op: UnaryOp::LogicalNot,
                operand: Box::new(operand),
            }))
        }
        Some(TokenKind::Compl) => {
            p.bump()?;
            let operand = parse_expr_prefix(p)?;
            Ok(Expr::Unary(ExprUnary {
                op: UnaryOp::BitwiseNot,
                operand: Box::new(operand),
            }))
        }
        Some(TokenKind::Star) => {
            p.bump()?;
            let operand = parse_expr_prefix(p)?;
            Ok(Expr::Unary(ExprUnary {
                op: UnaryOp::Deref,
                operand: Box::new(operand),
            }))
        }
        Some(TokenKind::BitAnd) => {
            p.bump()?;
            let operand = parse_expr_prefix(p)?;
            Ok(Expr::Unary(ExprUnary {
                op: UnaryOp::AddressOf,
                operand: Box::new(operand),
            }))
        }
        Some(TokenKind::Increment) => {
            p.bump()?;
            let operand = parse_expr_prefix(p)?;
            Ok(Expr::Unary(ExprUnary {
                op: UnaryOp::PreIncrement,
                operand: Box::new(operand),
            }))
        }
        Some(TokenKind::Decrement) => {
            p.bump()?;
            let operand = parse_expr_prefix(p)?;
            Ok(Expr::Unary(ExprUnary {
                op: UnaryOp::PreDecrement,
                operand: Box::new(operand),
            }))
        }
        // throw
        Some(TokenKind::KeywordThrow) => {
            p.bump()?;
            let expr = if p.peek_kind() != Some(TokenKind::Semicolon)
                && p.peek_kind() != Some(TokenKind::RightParenthese)
            {
                Some(Box::new(parse_expr_no_comma(p)?))
            } else {
                None
            };
            Ok(Expr::Throw(ExprThrow { expr }))
        }
        // new
        Some(TokenKind::KeywordNew) => parse_new_expr(p, false),
        // delete
        Some(TokenKind::KeywordDelete) => parse_delete_expr(p, false),
        // ::new or ::delete
        Some(TokenKind::DoubleColon)
            if matches!(
                p.peek_nth_kind(1),
                Some(TokenKind::KeywordNew | TokenKind::KeywordDelete)
            ) =>
        {
            p.bump()?; // consume ::
            match p.peek_kind() {
                Some(TokenKind::KeywordNew) => parse_new_expr(p, true),
                Some(TokenKind::KeywordDelete) => parse_delete_expr(p, true),
                _ => unreachable!(),
            }
        }
        _ => parse_expr_primary(p),
    }
}

fn parse_new_expr<'de>(p: &mut Parser<'de>, global: bool) -> Result<Expr<'de>, AstError> {
    p.expect(TokenKind::KeywordNew)?;

    // Optional placement: new (expr) Type
    let placement = if p.peek_kind() == Some(TokenKind::LeftParenthese) {
        let cp = p.checkpoint();
        p.bump()?;
        // If what follows looks like a type, this is not placement
        if is_type_start(p.peek_kind()) {
            p.restore(&cp);
            None
        } else {
            let mut args = Punctuated::new();
            while p.peek_kind() != Some(TokenKind::RightParenthese) && !p.is_empty() {
                let arg = parse_expr_no_comma(p)?;
                if let Some(comma) = p.eat(TokenKind::Comma) {
                    args.push_pair(arg, comma);
                } else {
                    args.push_value(arg);
                    break;
                }
            }
            p.expect(TokenKind::RightParenthese)?;
            Some(args)
        }
    } else {
        None
    };

    let ty = parse_type(p)?;

    // Array size: new int[10]
    let ty = if p.peek_kind() == Some(TokenKind::LeftBracket) {
        p.bump()?;
        let size = if p.peek_kind() != Some(TokenKind::RightBracket) {
            Some(parse_expr(p)?)
        } else {
            None
        };
        p.expect(TokenKind::RightBracket)?;
        Type::Array(TypeArray {
            element: Box::new(ty),
            size,
        })
    } else {
        ty
    };

    let initializer = match p.peek_kind() {
        Some(TokenKind::LeftParenthese) => {
            p.bump()?;
            let mut args = Punctuated::new();
            while p.peek_kind() != Some(TokenKind::RightParenthese) && !p.is_empty() {
                let arg = parse_expr_no_comma(p)?;
                if let Some(comma) = p.eat(TokenKind::Comma) {
                    args.push_pair(arg, comma);
                } else {
                    args.push_value(arg);
                    break;
                }
            }
            p.expect(TokenKind::RightParenthese)?;
            Some(NewInitializer::Parens(args))
        }
        Some(TokenKind::LeftBrace) => {
            p.bump()?;
            let mut elems = Vec::new();
            while p.peek_kind() != Some(TokenKind::RightBrace) && !p.is_empty() {
                elems.push(parse_expr_no_comma(p)?);
                if p.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
            p.expect(TokenKind::RightBrace)?;
            Some(NewInitializer::Braces(elems))
        }
        _ => None,
    };

    Ok(Expr::New(ExprNew {
        global,
        placement,
        ty: Box::new(ty),
        initializer,
    }))
}

fn parse_delete_expr<'de>(p: &mut Parser<'de>, global: bool) -> Result<Expr<'de>, AstError> {
    p.expect(TokenKind::KeywordDelete)?;
    let array = if p.eat(TokenKind::LeftBracket).is_some() {
        p.expect(TokenKind::RightBracket)?;
        true
    } else {
        false
    };
    let expr = parse_expr_prefix(p)?;
    Ok(Expr::Delete(ExprDelete {
        global,
        array,
        expr: Box::new(expr),
    }))
}

fn parse_expr_primary<'de>(p: &mut Parser<'de>) -> Result<Expr<'de>, AstError> {
    match p.peek_kind() {
        Some(TokenKind::Number) => {
            let tok = p.bump()?;
            let src = tok.src();
            let kind = if src.contains('.') || src.contains('e') || src.contains('E') {
                LitKind::Float
            } else {
                LitKind::Integer
            };
            let mut span = tok.src_span();
            // User-defined literal suffix: 0_potato, 1.0_sec, etc.
            if let Some(suffix) = p.peek()
                && suffix.kind() == TokenKind::Ident
                && suffix.src().starts_with('_')
            {
                let s = p.bump()?;
                let s_range: core::ops::Range<usize> = s.src_span().into();
                let start_range: core::ops::Range<usize> = span.into();
                span = SourceSpan::new(p.src, start_range.start, s_range.end - start_range.start);
            }
            Ok(Expr::Lit(ExprLit { span, kind }))
        }
        Some(TokenKind::String) => {
            let tok = p.bump()?;
            // String concatenation: "a" "b" "c" or "a" MACRO "b"
            let mut span = tok.src_span();
            loop {
                if p.peek_kind() == Some(TokenKind::String) {
                    let next = p.bump()?;
                    let next_range: core::ops::Range<usize> = next.src_span().into();
                    let start_range: core::ops::Range<usize> = span.into();
                    span = SourceSpan::new(
                        p.src,
                        start_range.start,
                        next_range.end - start_range.start,
                    );
                } else if p.peek_kind() == Some(TokenKind::Ident)
                    && matches!(
                        p.peek_nth_kind(1),
                        Some(TokenKind::String | TokenKind::Ident)
                    )
                {
                    // Macro between strings: "a" MACRO_NAME "b"
                    let next = p.bump()?;
                    let next_range: core::ops::Range<usize> = next.src_span().into();
                    let start_range: core::ops::Range<usize> = span.into();
                    span = SourceSpan::new(
                        p.src,
                        start_range.start,
                        next_range.end - start_range.start,
                    );
                } else {
                    break;
                }
            }
            Ok(Expr::Lit(ExprLit {
                span,
                kind: LitKind::String,
            }))
        }
        Some(TokenKind::Char) => {
            let tok = p.bump()?;
            Ok(Expr::Lit(ExprLit {
                span: tok.src_span(),
                kind: LitKind::Char,
            }))
        }
        Some(TokenKind::KeywordTrue) => {
            let tok = p.bump()?;
            Ok(Expr::Bool(ExprBool {
                span: tok.src_span(),
                value: true,
            }))
        }
        Some(TokenKind::KeywordFalse) => {
            let tok = p.bump()?;
            Ok(Expr::Bool(ExprBool {
                span: tok.src_span(),
                value: false,
            }))
        }
        Some(TokenKind::KeywordNullptr) => {
            let tok = p.bump()?;
            Ok(Expr::Nullptr(ExprNullptr {
                span: tok.src_span(),
            }))
        }
        Some(TokenKind::KeywordThis) => {
            let tok = p.bump()?;
            Ok(Expr::This(ExprThis {
                span: tok.src_span(),
            }))
        }
        // sizeof
        Some(TokenKind::KeywordSizeof) => {
            p.bump()?;
            let operand = if p.peek_kind() == Some(TokenKind::LeftParenthese) {
                p.bump()?;
                let start_sizeof = p.peek();
                let cp = p.checkpoint();
                if is_type_start(p.peek_kind()) {
                    if let Ok(_ty) = parse_type(p)
                        && p.eat(TokenKind::RightParenthese).is_some()
                    {
                        return Ok(Expr::Sizeof(ExprSizeof {
                            operand: Box::new(Expr::Lit(ExprLit {
                                span: p.span_since(start_sizeof),
                                kind: LitKind::Integer,
                            })),
                        }));
                    }
                    p.restore(&cp);
                }
                let inner = parse_expr(p)?;
                p.expect(TokenKind::RightParenthese)?;
                inner
            } else {
                parse_expr_prefix(p)?
            };
            Ok(Expr::Sizeof(ExprSizeof {
                operand: Box::new(operand),
            }))
        }
        // alignof
        Some(TokenKind::KeywordAlignof) => {
            p.bump()?;
            p.expect(TokenKind::LeftParenthese)?;
            let ty = parse_type(p)?;
            p.expect(TokenKind::RightParenthese)?;
            Ok(Expr::Alignof(ExprAlignof { ty: Box::new(ty) }))
        }
        // typeid
        Some(TokenKind::KeywordTypeid) => {
            p.bump()?;
            p.expect(TokenKind::LeftParenthese)?;
            let cp = p.checkpoint();
            if is_type_start(p.peek_kind()) {
                if let Ok(ty) = parse_type(p)
                    && p.peek_kind() == Some(TokenKind::RightParenthese)
                {
                    p.expect(TokenKind::RightParenthese)?;
                    return Ok(Expr::Typeid(ExprTypeid {
                        operand: TypeidOperand::Type(Box::new(ty)),
                    }));
                }
                p.restore(&cp);
            }
            let expr = parse_expr(p)?;
            p.expect(TokenKind::RightParenthese)?;
            Ok(Expr::Typeid(ExprTypeid {
                operand: TypeidOperand::Expr(Box::new(expr)),
            }))
        }
        // Named casts: static_cast, dynamic_cast, const_cast, reinterpret_cast
        Some(
            TokenKind::KeywordStaticCast
            | TokenKind::KeywordDynamicCast
            | TokenKind::KeywordConstCast
            | TokenKind::KeywordReinterpretCast,
        ) => {
            let cast_kind = match p.peek_kind().unwrap() {
                TokenKind::KeywordStaticCast => CastKind::Static,
                TokenKind::KeywordDynamicCast => CastKind::Dynamic,
                TokenKind::KeywordConstCast => CastKind::Const,
                TokenKind::KeywordReinterpretCast => CastKind::Reinterpret,
                _ => unreachable!(),
            };
            p.bump()?;
            p.expect(TokenKind::LeftChevron)?;
            let ty = parse_type(p)?;
            p.expect(TokenKind::RightChevron)?;
            p.expect(TokenKind::LeftParenthese)?;
            let expr = parse_expr(p)?;
            p.expect(TokenKind::RightParenthese)?;
            Ok(Expr::Cast(ExprCast {
                cast_kind,
                ty: Box::new(ty),
                expr: Box::new(expr),
            }))
        }
        // Parenthesized expression or C-style cast
        Some(TokenKind::LeftParenthese) => {
            let cp = p.checkpoint();
            p.bump()?;
            // Try C-style cast: (type)expr
            if is_type_start(p.peek_kind())
                || matches!(
                    p.peek_kind(),
                    Some(TokenKind::Ident | TokenKind::DoubleColon)
                )
            {
                let _cp2 = p.checkpoint();
                if let Ok(ty) = parse_type(p)
                    && p.eat(TokenKind::RightParenthese).is_some()
                {
                    // Check if what follows can be a unary expression
                    if is_cast_follower(p.peek_kind()) {
                        let expr = parse_expr_prefix(p)?;
                        return Ok(Expr::CStyleCast(ExprCStyleCast {
                            ty: Box::new(ty),
                            expr: Box::new(expr),
                        }));
                    }
                }
                p.restore(&cp);
                p.bump()?; // re-consume (
            }
            let inner = parse_expr(p)?;
            p.expect(TokenKind::RightParenthese)?;
            Ok(Expr::Paren(ExprParen {
                expr: Box::new(inner),
            }))
        }
        // Lambda: [captures](params) { body }
        Some(TokenKind::LeftBracket) => {
            // Could be a lambda [capture]... or just a subscript (but subscript is postfix)
            parse_lambda(p)
        }
        // Braced init list: {1, 2, 3}
        Some(TokenKind::LeftBrace) => {
            p.bump()?;
            let mut elements = Vec::new();
            while p.peek_kind() != Some(TokenKind::RightBrace) && !p.is_empty() {
                elements.push(parse_expr_no_comma(p)?);
                if p.eat(TokenKind::Comma).is_none() {
                    break;
                }
            }
            p.expect(TokenKind::RightBrace)?;
            Ok(Expr::InitList(ExprInitList { elements }))
        }
        // Qualified path or identifier, possibly with template args
        Some(TokenKind::DoubleColon) | Some(TokenKind::Ident) => {
            // Handle prefixed string/char literals: L"...", L'..', u8"...", u"...", U"..."
            if p.peek_kind() == Some(TokenKind::Ident) {
                let src = p.peek().unwrap().src();
                if matches!(
                    src,
                    "L" | "u" | "U" | "u8" | "LR" | "uR" | "UR" | "u8R" | "R"
                ) && matches!(
                    p.peek_nth_kind(1),
                    Some(TokenKind::String | TokenKind::Char)
                ) {
                    p.bump()?; // consume prefix
                    let tok = p.bump()?;
                    let kind = if tok.kind() == TokenKind::String {
                        LitKind::String
                    } else {
                        LitKind::Char
                    };
                    // Handle string concatenation for prefixed strings too
                    let mut span = tok.src_span();
                    while p.peek_kind() == Some(TokenKind::String)
                        || (p.peek_kind() == Some(TokenKind::Ident)
                            && matches!(p.peek().unwrap().src(), "L" | "u" | "U" | "u8")
                            && p.peek_nth_kind(1) == Some(TokenKind::String))
                    {
                        if p.peek_kind() == Some(TokenKind::Ident) {
                            p.bump()?; // prefix
                        }
                        let next = p.bump()?;
                        let next_range: core::ops::Range<usize> = next.src_span().into();
                        let start_range: core::ops::Range<usize> = span.into();
                        span = SourceSpan::new(
                            p.src,
                            start_range.start,
                            next_range.end - start_range.start,
                        );
                    }
                    return Ok(Expr::Lit(ExprLit { span, kind }));
                }
            }
            let path = parse_path(p)?;
            // Try template arguments: ident<types>(...)
            if p.peek_kind() == Some(TokenKind::LeftChevron) {
                let cp = p.checkpoint();
                if let Ok(args) = parse_angle_bracketed_args(p) {
                    // If followed by ( or ;, it's a template instantiation
                    if matches!(
                        p.peek_kind(),
                        Some(
                            TokenKind::LeftParenthese
                                | TokenKind::Semicolon
                                | TokenKind::RightParenthese
                                | TokenKind::Comma
                                | TokenKind::DoubleColon
                                | TokenKind::Dot
                                | TokenKind::PointerMember
                                | TokenKind::LeftBrace
                                | TokenKind::RightBrace
                                | TokenKind::LeftBracket
                        )
                    ) {
                        return Ok(Expr::Path(ExprPath {
                            path,
                            args: Some(args.args),
                        }));
                    }
                }
                p.restore(&cp);
            }
            if path.segments.len() == 1 && !path.leading_colon {
                Ok(Expr::Ident(ExprIdent {
                    ident: path.segments[0].ident,
                }))
            } else {
                Ok(Expr::Path(ExprPath { path, args: None }))
            }
        }
        Some(kind) => Err(AstError::UnexpectedToken {
            expected: "expression".to_string(),
            found: kind,
            src: p.src.to_string(),
            err_span: p.peek().unwrap().src_span().into(),
        }),
        None => Err(p.eof_error("expression")),
    }
}

/// Check if what follows a closing `)` could be the start of a unary expression (for C-style cast disambiguation).
fn is_cast_follower(kind: Option<TokenKind>) -> bool {
    matches!(
        kind,
        Some(
            TokenKind::Ident
                | TokenKind::Number
                | TokenKind::String
                | TokenKind::Char
                | TokenKind::LeftParenthese
                | TokenKind::KeywordTrue
                | TokenKind::KeywordFalse
                | TokenKind::KeywordNullptr
                | TokenKind::KeywordThis
                | TokenKind::KeywordSizeof
                | TokenKind::KeywordNew
                | TokenKind::KeywordDelete
                | TokenKind::Minus
                | TokenKind::Plus
                | TokenKind::Not
                | TokenKind::Compl
                | TokenKind::Star
                | TokenKind::BitAnd
                | TokenKind::Increment
                | TokenKind::Decrement
                | TokenKind::DoubleColon
        )
    )
}

fn parse_lambda<'de>(p: &mut Parser<'de>) -> Result<Expr<'de>, AstError> {
    p.expect(TokenKind::LeftBracket)?;
    let mut captures = Vec::new();
    while p.peek_kind() != Some(TokenKind::RightBracket) && !p.is_empty() {
        let cap = parse_lambda_capture(p)?;
        captures.push(cap);
        if p.eat(TokenKind::Comma).is_none() {
            break;
        }
    }
    p.expect(TokenKind::RightBracket)?;

    // Optional parameter list
    let inputs = if p.peek_kind() == Some(TokenKind::LeftParenthese) {
        Some(parse_fn_params(p)?)
    } else {
        None
    };

    // Optional mutable
    p.eat(TokenKind::KeywordMutable);

    // Optional noexcept
    p.eat(TokenKind::KeywordNoexcept);

    // Optional trailing return type
    let return_type = if p.peek_kind() == Some(TokenKind::PointerMember) {
        p.bump()?;
        Some(Box::new(parse_type(p)?))
    } else {
        None
    };

    let body = parse_block(p)?;
    Ok(Expr::Lambda(ExprLambda {
        captures,
        inputs,
        return_type,
        body,
    }))
}

fn parse_lambda_capture<'de>(p: &mut Parser<'de>) -> Result<LambdaCapture<'de>, AstError> {
    match p.peek_kind() {
        // Default copy capture: =
        Some(TokenKind::Equal) => {
            p.bump()?;
            Ok(LambdaCapture::DefaultCopy)
        }
        // Default reference or by-ref capture
        Some(TokenKind::BitAnd) => {
            p.bump()?;
            if p.peek_kind() == Some(TokenKind::RightBracket)
                || p.peek_kind() == Some(TokenKind::Comma)
            {
                Ok(LambdaCapture::DefaultRef)
            } else {
                let ident = parse_ident(p)?;
                Ok(LambdaCapture::ByRef(ident))
            }
        }
        // *this
        Some(TokenKind::Star) if p.peek_nth_kind(1) == Some(TokenKind::KeywordThis) => {
            p.bump()?;
            p.bump()?;
            Ok(LambdaCapture::StarThis)
        }
        // this
        Some(TokenKind::KeywordThis) => {
            p.bump()?;
            Ok(LambdaCapture::This)
        }
        // by-value or init capture
        Some(TokenKind::Ident) => {
            let ident = parse_ident(p)?;
            if p.eat(TokenKind::Equal).is_some() {
                let expr = parse_expr_no_comma(p)?;
                Ok(LambdaCapture::Init(ident, Box::new(expr)))
            } else {
                Ok(LambdaCapture::ByValue(ident))
            }
        }
        _ => Err(p.error_at_current("expected lambda capture")),
    }
}

fn peek_binary_op(p: &Parser) -> Option<BinaryOp> {
    match p.peek_kind()? {
        TokenKind::Plus => Some(BinaryOp::Add),
        TokenKind::Minus => Some(BinaryOp::Sub),
        TokenKind::Star => Some(BinaryOp::Mul),
        TokenKind::Div => Some(BinaryOp::Div),
        TokenKind::Modulo => Some(BinaryOp::Mod),
        TokenKind::ShiftLeft => Some(BinaryOp::ShiftLeft),
        TokenKind::ShiftRight => Some(BinaryOp::ShiftRight),
        TokenKind::LeftChevron => Some(BinaryOp::Less),
        TokenKind::LessOrEqualTo => Some(BinaryOp::LessEqual),
        TokenKind::RightChevron => Some(BinaryOp::Greater),
        TokenKind::GreaterOrEqualTo => Some(BinaryOp::GreaterEqual),
        TokenKind::Compare => Some(BinaryOp::ThreeWay),
        TokenKind::EqualTo => Some(BinaryOp::Equal),
        TokenKind::NotEqualTo => Some(BinaryOp::NotEqual),
        TokenKind::BitAnd => Some(BinaryOp::BitAnd),
        TokenKind::Xor => Some(BinaryOp::BitXor),
        TokenKind::BitOr => Some(BinaryOp::BitOr),
        TokenKind::And => Some(BinaryOp::LogicalAnd),
        TokenKind::Or => Some(BinaryOp::LogicalOr),
        TokenKind::Equal => Some(BinaryOp::Assign),
        TokenKind::CompoundAdd => Some(BinaryOp::AddAssign),
        TokenKind::CompoundSub => Some(BinaryOp::SubAssign),
        TokenKind::CompoundMult => Some(BinaryOp::MulAssign),
        TokenKind::CompoundDiv => Some(BinaryOp::DivAssign),
        TokenKind::CompoundModulo => Some(BinaryOp::ModAssign),
        TokenKind::CompoundShiftLeft => Some(BinaryOp::ShiftLeftAssign),
        TokenKind::CompoundShiftRight => Some(BinaryOp::ShiftRightAssign),
        TokenKind::CompoundAnd | TokenKind::AndEq => Some(BinaryOp::BitAndAssign),
        TokenKind::CompoundXor | TokenKind::XorEq => Some(BinaryOp::BitXorAssign),
        TokenKind::CompoundOr | TokenKind::OrEq => Some(BinaryOp::BitOrAssign),
        TokenKind::Comma => Some(BinaryOp::Comma),
        TokenKind::PointerObjMember => Some(BinaryOp::DotStar),
        TokenKind::PointerObjAccess => Some(BinaryOp::ArrowStar),
        _ => None,
    }
}

fn binary_op_precedence(op: BinaryOp) -> (u8, bool) {
    match op {
        BinaryOp::Dot | BinaryOp::Arrow => (15, false),
        BinaryOp::DotStar | BinaryOp::ArrowStar => (13, false),
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => (12, false),
        BinaryOp::Add | BinaryOp::Sub => (11, false),
        BinaryOp::ShiftLeft | BinaryOp::ShiftRight => (10, false),
        BinaryOp::ThreeWay => (9, false),
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
            (8, false)
        }
        BinaryOp::Equal | BinaryOp::NotEqual => (7, false),
        BinaryOp::BitAnd => (6, false),
        BinaryOp::BitXor => (5, false),
        BinaryOp::BitOr => (4, false),
        BinaryOp::LogicalAnd => (3, false),
        BinaryOp::LogicalOr => (2, false),
        BinaryOp::Assign
        | BinaryOp::AddAssign
        | BinaryOp::SubAssign
        | BinaryOp::MulAssign
        | BinaryOp::DivAssign
        | BinaryOp::ModAssign
        | BinaryOp::ShiftLeftAssign
        | BinaryOp::ShiftRightAssign
        | BinaryOp::BitAndAssign
        | BinaryOp::BitXorAssign
        | BinaryOp::BitOrAssign => (1, true),
        BinaryOp::Comma => (0, false),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> File<'_> {
        parse_file(src).unwrap()
    }

    #[test]
    fn parse_empty_file() {
        let file = parse("");
        assert!(file.items.is_empty());
    }

    #[test]
    fn parse_namespace() {
        let file = parse("namespace foo { }");
        assert_eq!(file.items.len(), 1);
        match &file.items[0] {
            Item::Namespace(ns) => {
                assert_eq!(ns.ident.unwrap().sym, "foo");
                assert!(!ns.inline_token);
                assert!(ns.content.is_empty());
            }
            other => panic!("expected Namespace, got {other:?}"),
        }
    }

    #[test]
    fn parse_inline_namespace() {
        let file = parse("inline namespace v2 { }");
        match &file.items[0] {
            Item::Namespace(ns) => {
                assert!(ns.inline_token);
                assert_eq!(ns.ident.unwrap().sym, "v2");
            }
            other => panic!("expected Namespace, got {other:?}"),
        }
    }

    #[test]
    fn parse_nested_namespace() {
        let file = parse("namespace outer { namespace inner { } }");
        match &file.items[0] {
            Item::Namespace(ns) => {
                assert_eq!(ns.ident.unwrap().sym, "outer");
                assert_eq!(ns.content.len(), 1);
                assert!(matches!(&ns.content[0], Item::Namespace(_)));
            }
            other => panic!("expected Namespace, got {other:?}"),
        }
    }

    #[test]
    fn parse_static_assert_test() {
        let file = parse("static_assert(sizeof(int) == 4, \"int must be 4 bytes\");");
        assert_eq!(file.items.len(), 1);
        assert!(matches!(&file.items[0], Item::StaticAssert(_)));
    }

    #[test]
    fn parse_using_directive() {
        let file = parse("using namespace std;");
        match &file.items[0] {
            Item::Use(ItemUse::Directive { namespace, .. }) => {
                assert_eq!(namespace.segments[0].ident.sym, "std");
            }
            other => panic!("expected Use::Directive, got {other:?}"),
        }
    }

    #[test]
    fn parse_using_declaration() {
        let file = parse("using std::cout;");
        match &file.items[0] {
            Item::Use(ItemUse::Declaration { name, .. }) => {
                assert_eq!(name.segments.len(), 2);
            }
            other => panic!("expected Use::Declaration, got {other:?}"),
        }
    }

    #[test]
    fn parse_using_alias() {
        let file = parse("using size_t = unsigned long;");
        match &file.items[0] {
            Item::Type(it) => {
                assert_eq!(it.ident.sym, "size_t");
            }
            other => panic!("expected Type, got {other:?}"),
        }
    }

    #[test]
    fn parse_typedef_test() {
        let file = parse("typedef unsigned long size_t;");
        match &file.items[0] {
            Item::Typedef(td) => {
                assert_eq!(td.ident.sym, "size_t");
            }
            other => panic!("expected Typedef, got {other:?}"),
        }
    }

    #[test]
    fn parse_typedef_array() {
        let file = parse("typedef char type24[3];");
        match &file.items[0] {
            Item::Typedef(td) => {
                assert_eq!(td.ident.sym, "type24");
                match &td.ty {
                    Type::Array(arr) => {
                        assert!(matches!(arr.element.as_ref(), Type::Fundamental(_)));
                        assert!(arr.size.is_some());
                    }
                    other => panic!("expected Array type, got {other:?}"),
                }
            }
            other => panic!("expected Typedef, got {other:?}"),
        }
    }

    #[test]
    fn parse_typedef_array_2d() {
        let file = parse("typedef int matrix[3][4];");
        match &file.items[0] {
            Item::Typedef(td) => {
                assert_eq!(td.ident.sym, "matrix");
                match &td.ty {
                    Type::Array(outer) => {
                        assert!(matches!(outer.element.as_ref(), Type::Array(_)));
                    }
                    other => panic!("expected Array type, got {other:?}"),
                }
            }
            other => panic!("expected Typedef, got {other:?}"),
        }
    }

    #[test]
    fn parse_enum_test() {
        let file = parse("enum Color { Red, Green, Blue };");
        match &file.items[0] {
            Item::Enum(e) => {
                assert_eq!(e.ident.unwrap().sym, "Color");
                assert!(!e.scoped);
                assert_eq!(e.variants.len(), 3);
            }
            other => panic!("expected Enum, got {other:?}"),
        }
    }

    #[test]
    fn parse_enum_class_test() {
        let file = parse("enum class Color : int { Red = 0, Green, Blue };");
        match &file.items[0] {
            Item::Enum(e) => {
                assert!(e.scoped);
                assert!(e.underlying_type.is_some());
                assert_eq!(e.variants.len(), 3);
            }
            other => panic!("expected Enum, got {other:?}"),
        }
    }

    #[test]
    fn parse_enum_with_trailing_declarator() {
        // Anonymous enum declaring a member in one statement (C++ allows this).
        let file = parse("enum { FC_NONE, FC_ALL } mFuncCallHistoryType;");
        match &file.items[0] {
            Item::Enum(e) => {
                assert!(e.ident.is_none());
                assert_eq!(e.variants.len(), 2);
            }
            other => panic!("expected Enum, got {other:?}"),
        }
    }

    #[test]
    fn parse_method_call_with_template_args() {
        // Explicit template arguments on a method call (turbofish); the args are
        // discarded but the call must parse as a method call.
        let file = parse("void f() { pTxn->searchDecoW<cTDPci>(); }");
        match &file.items[0] {
            Item::Fn(_) => {}
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_arrow_comparison_still_works() {
        // A genuine comparison after `->` must not be mistaken for a turbofish.
        let file = parse("void f() { bool b = a->x < c; }");
        match &file.items[0] {
            Item::Fn(_) => {}
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_struct_forward() {
        let file = parse("struct Foo;");
        match &file.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.ident.unwrap().sym, "Foo");
                assert!(matches!(s.fields, Fields::Unit));
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn parse_struct_with_fields() {
        let file = parse("struct Point { int x; int y; };");
        match &file.items[0] {
            Item::Struct(s) => {
                assert_eq!(s.ident.unwrap().sym, "Point");
                assert!(matches!(s.fields, Fields::Named(_)));
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn parse_class_with_access() {
        let file = parse("class Foo { public: int x; private: int y; };");
        match &file.items[0] {
            Item::Class(c) => {
                assert_eq!(c.ident.unwrap().sym, "Foo");
            }
            other => panic!("expected Class, got {other:?}"),
        }
    }

    #[test]
    fn parse_class_inheritance() {
        let file = parse("class Derived : public Base { };");
        match &file.items[0] {
            Item::Class(c) => {
                assert_eq!(c.bases.len(), 1);
                assert_eq!(c.bases[0].access, Visibility::Public);
            }
            other => panic!("expected Class, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_decl() {
        let file = parse("int foo();");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.sig.ident.sym, "foo");
                assert!(f.block.is_none());
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_definition() {
        let file = parse("int main() { return 0; }");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.sig.ident.sym, "main");
                assert!(f.block.is_some());
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_with_params() {
        let file = parse("int add(int a, int b);");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.sig.inputs.len(), 2);
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_function_specifiers() {
        let file = parse("inline constexpr int square(int x) { return x * x; }");
        match &file.items[0] {
            Item::Fn(f) => {
                assert!(f.sig.inline_token);
                assert!(f.sig.constexpr_token);
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_virtual_function() {
        let file = parse("virtual void draw() = 0;");
        match &file.items[0] {
            Item::Fn(f) => {
                assert!(f.sig.virtual_token);
                assert!(f.sig.pure_virtual);
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_const_variable() {
        let file = parse("const int MAX = 100;");
        match &file.items[0] {
            Item::Const(c) => {
                assert_eq!(c.ident.sym, "MAX");
            }
            other => panic!("expected Const, got {other:?}"),
        }
    }

    #[test]
    fn parse_constexpr_variable() {
        let file = parse("constexpr int SIZE = 42;");
        match &file.items[0] {
            Item::Const(c) => {
                assert!(c.constexpr_token);
                assert_eq!(c.ident.sym, "SIZE");
            }
            other => panic!("expected Const, got {other:?}"),
        }
    }

    #[test]
    fn parse_template_function() {
        let file = parse("template<typename T> T max(T a, T b);");
        match &file.items[0] {
            Item::Template(t) => {
                assert_eq!(t.params.len(), 1);
                assert!(matches!(&*t.item, Item::Fn(_)));
            }
            other => panic!("expected Template, got {other:?}"),
        }
    }

    #[test]
    fn parse_template_class() {
        let file = parse("template<typename T> class Vec { };");
        match &file.items[0] {
            Item::Template(t) => {
                assert!(matches!(&*t.item, Item::Class(_)));
            }
            other => panic!("expected Template, got {other:?}"),
        }
    }

    #[test]
    fn parse_extern_c_block() {
        let file = parse("extern \"C\" { int foo(); }");
        match &file.items[0] {
            Item::ForeignMod(fm) => {
                assert_eq!(fm.abi, "C");
                assert_eq!(fm.items.len(), 1);
            }
            other => panic!("expected ForeignMod, got {other:?}"),
        }
    }

    #[test]
    fn parse_union_test() {
        let file = parse("union Data { int i; float f; };");
        match &file.items[0] {
            Item::Union(u) => {
                assert_eq!(u.ident.unwrap().sym, "Data");
            }
            other => panic!("expected Union, got {other:?}"),
        }
    }

    #[test]
    fn parse_multiple_items() {
        let file = parse("namespace ns { int foo(); void bar(); } enum E { A, B };");
        assert_eq!(file.items.len(), 2);
        assert!(matches!(&file.items[0], Item::Namespace(_)));
        assert!(matches!(&file.items[1], Item::Enum(_)));
    }

    // -- Étape 0 tests --

    #[test]
    fn parse_nested_block() {
        let file = parse("void f() { { } }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                assert_eq!(block.stmts.len(), 1);
                assert!(matches!(&block.stmts[0], Stmt::Block(_)));
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_attribute_nodiscard() {
        let file = parse("[[nodiscard]] int compute();");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.attrs.len(), 1);
                assert_eq!(f.attrs[0].path.segments[0].ident.sym, "nodiscard");
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_attribute_with_args() {
        let file = parse("[[deprecated(\"use bar\")]] void foo();");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.attrs.len(), 1);
                assert_eq!(f.attrs[0].path.segments[0].ident.sym, "deprecated");
                assert!(!f.attrs[0].args.is_empty());
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_multiple_attributes() {
        let file = parse("[[nodiscard]] [[deprecated]] int f();");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.attrs.len(), 2);
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    // -- Étape 1 tests: statements --

    #[test]
    fn parse_local_variable() {
        let file = parse("void f() { int x = 42; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                assert!(matches!(&block.stmts[0], Stmt::Local(_)));
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_local_auto() {
        let file = parse("void f() { auto x = 5; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                assert!(matches!(&block.stmts[0], Stmt::Local(_)));
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_local_no_init() {
        let file = parse("void f() { int x; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Local(local) => {
                        assert_eq!(local.ident.sym, "x");
                        assert!(local.init.is_none());
                    }
                    other => panic!("expected Local, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_do_while() {
        let file = parse("void f() { do { x++; } while (x < 10); }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                assert!(matches!(&block.stmts[0], Stmt::DoWhile(_)));
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_switch() {
        let file =
            parse("void f() { switch (x) { case 1: break; case 2: break; default: break; } }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                assert!(matches!(&block.stmts[0], Stmt::Switch(_)));
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_goto_label() {
        let file = parse("void f() { goto end; end: return; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                assert!(matches!(&block.stmts[0], Stmt::Goto(_)));
                assert!(matches!(&block.stmts[1], Stmt::Label(_)));
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_for_range() {
        let file = parse("void f() { for (auto x : vec) { } }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::ForRange(fr) => {
                        assert!(matches!(&fr.binding, ForRangeBinding::Ident(_)));
                    }
                    other => panic!("expected ForRange, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_for_range_structured_binding() {
        let file = parse("void f() { for (const auto& [k, v] : map) { } }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::ForRange(fr) => match &fr.binding {
                        ForRangeBinding::Structured(idents) => {
                            assert_eq!(idents.len(), 2);
                            assert_eq!(idents[0].sym, "k");
                            assert_eq!(idents[1].sym, "v");
                        }
                        other => panic!("expected Structured, got {other:?}"),
                    },
                    other => panic!("expected ForRange, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_try_catch() {
        let file = parse("void f() { try { } catch (int e) { } catch (...) { } }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::TryCatch(tc) => {
                        assert_eq!(tc.catches.len(), 2);
                        assert!(matches!(&tc.catches[1].param, CatchParam::Ellipsis));
                    }
                    other => panic!("expected TryCatch, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_for_with_local_init() {
        let file = parse("void f() { for (int i = 0; i < 10; i++) { } }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::For(for_stmt) => {
                        assert!(matches!(for_stmt.init.as_deref(), Some(Stmt::Local(_))));
                    }
                    other => panic!("expected For, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    // -- Étape 2 tests: expressions --

    #[test]
    fn parse_static_cast() {
        let file = parse("void f() { static_cast<int>(x); }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Expr(se) => assert!(matches!(&se.expr, Expr::Cast(_))),
                    other => panic!("expected Expr, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_c_style_cast() {
        let file = parse("void f() { (int)3.14; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Expr(se) => assert!(matches!(&se.expr, Expr::CStyleCast(_))),
                    other => panic!("expected Expr, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_new_simple() {
        let file = parse("void f() { new int(42); }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Expr(se) => assert!(matches!(&se.expr, Expr::New(_))),
                    other => panic!("expected Expr, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_delete_expr() {
        let file = parse("void f() { delete ptr; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Expr(se) => assert!(matches!(&se.expr, Expr::Delete(_))),
                    other => panic!("expected Expr, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_delete_array() {
        let file = parse("void f() { delete[] arr; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Expr(se) => match &se.expr {
                        Expr::Delete(d) => assert!(d.array),
                        other => panic!("expected Delete, got {other:?}"),
                    },
                    other => panic!("expected Expr, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_throw() {
        let file = parse("void f() { throw 42; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Expr(se) => assert!(matches!(&se.expr, Expr::Throw(_))),
                    other => panic!("expected Expr, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_alignof_test() {
        let file = parse("void f() { alignof(int); }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Expr(se) => assert!(matches!(&se.expr, Expr::Alignof(_))),
                    other => panic!("expected Expr, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_lambda_simple() {
        let file = parse("void f() { [](int x) { return x; }; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Expr(se) => assert!(matches!(&se.expr, Expr::Lambda(_))),
                    other => panic!("expected Expr, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_lambda_captures() {
        let file = parse("void f() { [&, x](){ }; }");
        match &file.items[0] {
            Item::Fn(f) => {
                let block = f.block.as_ref().unwrap();
                match &block.stmts[0] {
                    Stmt::Expr(se) => match &se.expr {
                        Expr::Lambda(l) => {
                            assert_eq!(l.captures.len(), 2);
                            assert!(matches!(&l.captures[0], LambdaCapture::DefaultRef));
                            assert!(matches!(&l.captures[1], LambdaCapture::ByValue(_)));
                        }
                        other => panic!("expected Lambda, got {other:?}"),
                    },
                    other => panic!("expected Expr, got {other:?}"),
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_string_concat() {
        let file = parse("const char* s = \"hello\" \" \" \"world\";");
        match &file.items[0] {
            Item::Const(c) => match &c.expr {
                Expr::Lit(lit) => assert_eq!(lit.kind, LitKind::String),
                other => panic!("expected Lit, got {other:?}"),
            },
            other => panic!("expected Const, got {other:?}"),
        }
    }

    // -- Étape 3 tests: types --

    #[test]
    fn parse_type_vector_int() {
        let file = parse("void f(std::vector<int> v);");
        assert!(matches!(&file.items[0], Item::Fn(_)));
    }

    #[test]
    fn parse_type_map() {
        let file = parse("void f(std::map<std::string, int> m);");
        assert!(matches!(&file.items[0], Item::Fn(_)));
    }

    #[test]
    fn parse_type_template_inst() {
        let file = parse("void f(std::unique_ptr<int> p);");
        match &file.items[0] {
            Item::Fn(f) => {
                let param = f.sig.inputs.iter().next().unwrap();
                assert!(matches!(&param.ty, Type::TemplateInst(_)));
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_expr_template_call() {
        // std::shared_ptr<int>() should parse the template args
        let file = parse("void f() { std::shared_ptr<int>(); }");
        match &file.items[0] {
            Item::Fn(f) => {
                let stmt = &f.block.as_ref().unwrap().stmts[0];
                if let Stmt::Expr(StmtExpr {
                    expr: Expr::Call(call),
                }) = stmt
                {
                    if let Expr::Path(ExprPath { path, args }) = &*call.func {
                        assert_eq!(path.segments.len(), 2);
                        assert_eq!(path.segments[0].ident.sym, "std");
                        assert_eq!(path.segments[1].ident.sym, "shared_ptr");
                        let args = args.as_ref().expect("template args should be present");
                        assert_eq!(args.len(), 1);
                        assert!(matches!(&args[0], TemplateArg::Type(_)));
                    } else {
                        panic!("expected ExprPath with template args, got {:#?}", call.func);
                    }
                } else {
                    panic!("expected call expression, got {stmt:#?}");
                }
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    // -- Étape 5 tests: item features --

    #[test]
    fn parse_nested_namespace_syntax() {
        let file = parse("namespace a::b::c { int x; }");
        match &file.items[0] {
            Item::Namespace(ns) => {
                assert_eq!(ns.ident.unwrap().sym, "a");
                match &ns.content[0] {
                    Item::Namespace(ns2) => {
                        assert_eq!(ns2.ident.unwrap().sym, "b");
                        match &ns2.content[0] {
                            Item::Namespace(ns3) => {
                                assert_eq!(ns3.ident.unwrap().sym, "c");
                            }
                            other => panic!("expected Namespace, got {other:?}"),
                        }
                    }
                    other => panic!("expected Namespace, got {other:?}"),
                }
            }
            other => panic!("expected Namespace, got {other:?}"),
        }
    }

    #[test]
    fn parse_trailing_return_type() {
        let file = parse("auto add(int a, int b) -> int;");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.sig.ident.sym, "add");
                assert!(matches!(f.sig.return_type, Type::Fundamental(_)));
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    // -- Étape 4 tests: class members --

    #[test]
    fn parse_constructor() {
        let file = parse("class Foo { Foo(int x); };");
        match &file.items[0] {
            Item::Class(c) => {
                assert!(c.fields != Fields::Unit);
                if let Fields::Named(f) = &c.fields {
                    assert!(matches!(&f.members[0], Member::Constructor(_)));
                }
            }
            other => panic!("expected Class, got {other:?}"),
        }
    }

    #[test]
    fn parse_constructor_init_list() {
        let file = parse("class Foo { Foo(int x) : m_x(x) { } };");
        match &file.items[0] {
            Item::Class(c) => {
                if let Fields::Named(f) = &c.fields {
                    match &f.members[0] {
                        Member::Constructor(ctor) => {
                            assert_eq!(ctor.member_init_list.len(), 1);
                            assert_eq!(ctor.member_init_list[0].member.to_string(), "m_x");
                        }
                        other => panic!("expected Constructor, got {other:?}"),
                    }
                }
            }
            other => panic!("expected Class, got {other:?}"),
        }
    }

    #[test]
    fn parse_destructor() {
        let file = parse("class Foo { ~Foo(); };");
        match &file.items[0] {
            Item::Class(c) => {
                if let Fields::Named(f) = &c.fields {
                    assert!(matches!(&f.members[0], Member::Destructor(_)));
                }
            }
            other => panic!("expected Class, got {other:?}"),
        }
    }

    #[test]
    fn parse_virtual_destructor() {
        let file = parse("class Foo { virtual ~Foo() = default; };");
        match &file.items[0] {
            Item::Class(c) => {
                if let Fields::Named(f) = &c.fields {
                    match &f.members[0] {
                        Member::Destructor(d) => {
                            assert!(d.virtual_token);
                            assert!(d.defaulted);
                        }
                        other => panic!("expected Destructor, got {other:?}"),
                    }
                }
            }
            other => panic!("expected Class, got {other:?}"),
        }
    }

    #[test]
    fn parse_qualified_method() {
        let file = parse("void Foo::bar() { }");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.sig.ident.sym, "bar");
                assert!(!f.sig.is_class_constructor());
                assert!(!f.sig.is_class_destructor());
                assert!(f.block.is_some());
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_qualified_constructor() {
        let file = parse("Foo::Foo() { }");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.sig.ident.sym, "Foo");
                assert!(f.sig.is_class_constructor());
                assert!(!f.sig.is_class_destructor());
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }

    #[test]
    fn parse_qualified_destructor() {
        let file = parse("Foo::~Foo() { }");
        match &file.items[0] {
            Item::Fn(f) => {
                assert_eq!(f.sig.ident.sym, "Foo");
                assert!(!f.sig.is_class_constructor());
                assert!(f.sig.is_class_destructor());
            }
            other => panic!("expected Fn, got {other:?}"),
        }
    }
}
