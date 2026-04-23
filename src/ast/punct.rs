//! Punctuated sequences - lists of syntax tree nodes separated by punctuation tokens
//!
//! Inspired by `syn::punctuated::Punctuated`.

use std::slice;

use crate::{SourceCodeSpan, SourceSpan, lex::Token};

/// A sequence of syntax tree nodes of type `T` separated by punctuation tokens.
///
/// For example, `Punctuated<Expr, Token>` represents comma-separated expressions
/// like function arguments: `a, b, c`.
#[derive(Debug, Clone, PartialEq)]
pub struct Punctuated<'de, T> {
    pub(crate) inner: Vec<(T, Option<Token<'de>>)>,
}

impl<'de, T> Punctuated<'de, T> {
    /// Create a new empty punctuated sequence.
    pub fn new() -> Self {
        Punctuated { inner: Vec::new() }
    }

    /// Push a value without trailing punctuation.
    pub fn push_value(&mut self, value: T) {
        self.inner.push((value, None));
    }

    /// Push a value with its trailing punctuation.
    pub fn push_pair(&mut self, value: T, punct: Token<'de>) {
        self.inner.push((value, Some(punct)));
    }

    /// Returns the number of values in the sequence.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the sequence is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over the values only (without punctuation).
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inner.iter().map(|(v, _)| v)
    }

    /// Iterate over the (value, punctuation) pairs.
    pub fn pairs(&self) -> slice::Iter<'_, (T, Option<Token<'de>>)> {
        self.inner.iter()
    }

    /// Returns true if the last element has trailing punctuation.
    pub fn trailing_punct(&self) -> bool {
        self.inner.last().is_some_and(|(_, p)| p.is_some())
    }

    /// Convert into a Vec of just the values.
    pub fn into_values(self) -> Vec<T> {
        self.inner.into_iter().map(|(v, _)| v).collect()
    }
}

impl<'de, T: SourceCodeSpan<'de>> SourceCodeSpan<'de> for Punctuated<'de, T> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        let mut span: Option<SourceSpan<'de>> = None;
        for punctuate in &self.inner {
            if let Some(span) = span {
                if let Some(inner_span) = punctuate.0.span() {
                    span.extend(inner_span);
                }
            } else {
                span = punctuate.0.span();
            }
        }

        span
    }
}

impl<'de, T> Default for Punctuated<'de, T> {
    fn default() -> Self {
        Self::new()
    }
}
