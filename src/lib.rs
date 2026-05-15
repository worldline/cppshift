#[cfg(feature = "ast")]
pub mod ast;
pub mod lex;
#[cfg(feature = "transpiler")]
pub mod transpile;
use std::{collections::LinkedList, fmt};

pub use lex::Lexer;

/// Source code span
/// Useful to track the position of a text fragment in the source code, for error reporting.
///
/// ```
/// use cppshift::SourceSpan;
///
/// let src = "test";
/// let src_span = SourceSpan::new(src, 1, 2);
/// assert_eq!("es", src_span.src());
/// assert_eq!((1..3), core::ops::Range::<usize>::from(src_span));
/// ```
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan<'de> {
    src: &'de str,
    offset: usize,
    len: usize,
}

impl<'de> SourceSpan<'de> {
    /// Create a new Span from an offset and a length
    pub fn new(src: &'de str, offset: usize, len: usize) -> SourceSpan<'de> {
        SourceSpan { src, offset, len }
    }

    /// Getter of the source code value
    pub fn src(&self) -> &'de str {
        &self.src[core::ops::Range::from(*self)]
    }

    /// Returns the full backing source string (not just this span's slice).
    pub fn full_source(&self) -> &'de str {
        self.src
    }

    /// Extend this span to include another span, returning a new span that covers both.
    pub fn extend(&self, other: SourceSpan<'de>) -> SourceSpan<'de> {
        let start = self.offset.min(other.offset);
        let end = (self.offset + self.len).max(other.offset + other.len);
        SourceSpan::new(self.src, start, end - start)
    }
}

impl<'de> fmt::Debug for SourceSpan<'de> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceSpan")
            .field("src", &self.src())
            .field("offset", &self.offset)
            .field("len", &self.len)
            .finish()
    }
}

impl<'de> From<SourceSpan<'de>> for miette::SourceSpan {
    fn from(span: SourceSpan<'de>) -> Self {
        miette::SourceSpan::new(span.offset.into(), span.len)
    }
}

impl<'de> From<SourceSpan<'de>> for core::ops::Range<usize> {
    fn from(span: SourceSpan<'de>) -> Self {
        core::ops::Range {
            start: span.offset,
            end: span.offset + span.len,
        }
    }
}

/// Trait to implement a SourceSpan getter
pub trait SourceCodeSpan<'de> {
    /// Get the source span, if available.
    fn span(&self) -> Option<SourceSpan<'de>>;
}

impl<'de> SourceCodeSpan<'de> for SourceSpan<'de> {
    fn span(&self) -> Option<SourceSpan<'de>> {
        Some(*self)
    }
}

#[doc(hidden)]
#[macro_export]
macro_rules! __span_expr {
    ( Some, $self:ident, $member:ident ) => {
        Some($self.$member)
    };
    ( and_then, $self:ident, $member:ident ) => {
        $self.$member.as_ref().and_then(|m| m.span())
    };
    ( plain, $self:ident, $member:ident ) => {
        $self.$member.span()
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __span_chain {
    // Base case: nothing left to process, emit the accumulator.
    ( $self:ident, $acc:expr $(,)? ) => {
        $acc
    };

    // Recursive cases: consume one (modifier, member) pair, wrap with extend logic.
    ( $self:ident, $acc:expr, Some, $member:ident $( , $rest:tt )* ) => {
        $crate::__span_chain!(
            $self,
            {
                if let Some(span) = $acc {
                    if let Some(span_expr) = $crate::__span_expr!(Some, $self, $member) {
                        Some(span.extend(span_expr))
                    } else {
                        Some(span)
                    }
                } else {
                    $acc
                }
            }
            $(, $rest)*
        )
    };
    ( $self:ident, $acc:expr, and_then, $member:ident $( , $rest:tt )* ) => {
        $crate::__span_chain!(
            $self,
            {
                if let Some(span) = $acc {
                    if let Some(span_expr) = $crate::__span_expr!(and_then, $self, $member) {
                        Some(span.extend(span_expr))
                    } else {
                        Some(span)
                    }
                } else {
                    $acc
                }
            }
            $(, $rest)*
        )
    };
    ( $self:ident, $acc:expr, $member:ident $( , $rest:tt )* ) => {
        $crate::__span_chain!(
            $self,
            {
                if let Some(span) = $acc {
                    if let Some(span_expr) = $crate::__span_expr!(plain, $self, $member) {
                        Some(span.extend(span_expr))
                    } else {
                        Some(span)
                    }
                } else {
                    $acc
                }
            }
            $(, $rest)*
        )
    };
}

/// Public macro: implement SourceCodeSpan for a struct.
///
/// Syntax:
///   source_code_span_impl!(StructName, [modifier,] member, [modifier,] member, ...)
///
/// Each element is an optional modifier (`Some` or `and_then`) followed by a field name.
/// Elements are combined left-to-right: spans are extended when both are present,
/// or the non-None one is used when only one is present.
#[macro_export]
macro_rules! source_code_span_impl {
    ( $s:ident, Some, $member:ident ) => {
        impl<'de> SourceCodeSpan<'de> for $s<'de> {
            fn span(&self) -> Option<SourceSpan<'de>> {
                $crate::__span_expr!(Some, self, $member)
            }
        }
    };
    ( $s:ident, Some, $member:ident, $( $rest:tt ),+ ) => {
        impl<'de> SourceCodeSpan<'de> for $s<'de> {
            fn span(&self) -> Option<SourceSpan<'de>> {
                $crate::__span_chain!(self, $crate::__span_expr!(Some, self, $member), $( $rest ),+)
            }
        }
    };
    ( $s:ident, and_then, $member:ident ) => {
        impl<'de> SourceCodeSpan<'de> for $s<'de> {
            fn span(&self) -> Option<SourceSpan<'de>> {
                $crate::__span_expr!(and_then, self, $member)
            }
        }
    };
    ( $s:ident, and_then, $member:ident, $( $rest:tt ),+ ) => {
        impl<'de> SourceCodeSpan<'de> for $s<'de> {
            fn span(&self) -> Option<SourceSpan<'de>> {
                $crate::__span_chain!(self, $crate::__span_expr!(and_then, self, $member), $( $rest ),+)
            }
        }
    };
    ( $s:ident, $member:ident ) => {
        impl<'de> SourceCodeSpan<'de> for $s<'de> {
            fn span(&self) -> Option<SourceSpan<'de>> {
                $crate::__span_expr!(plain, self, $member)
            }
        }
    };
    ( $s:ident, $member:ident, $( $rest:tt ),+ ) => {
        impl<'de> SourceCodeSpan<'de> for $s<'de> {
            fn span(&self) -> Option<SourceSpan<'de>> {
                $crate::__span_chain!(self, $crate::__span_expr!(plain, self, $member), $( $rest ),+)
            }
        }
    };
}

#[macro_export]
macro_rules! list_span_impl {
    ( $t:ty ) => {
        impl<'de, T: SourceCodeSpan<'de>> SourceCodeSpan<'de> for $t {
            fn span(&self) -> Option<SourceSpan<'de>> {
                let mut first_span: Option<SourceSpan<'de>> = None;
                for item in self.iter() {
                    first_span = item.span();
                    if first_span.is_some() {
                        break;
                    }
                }

                if let Some(first_item_span) = first_span {
                    for item in self.iter().rev() {
                        if let Some(item_span) = item.span() {
                            return Some(first_item_span.extend(item_span));
                        }
                    }

                    Some(first_item_span)
                } else {
                    None
                }
            }
        }
    };
}

list_span_impl!([T]);
list_span_impl!(LinkedList<T>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_span() {
        let src = "test";
        let src_span = SourceSpan::new(src, 1, 2);
        assert_eq!("es", src_span.src());
        assert_eq!((1..3), core::ops::Range::<usize>::from(src_span));
    }
}
