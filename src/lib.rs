pub mod ast;
pub mod lex;
pub use lex::Lexer;

/// Source code span
#[derive(Debug, Clone, Copy, PartialEq)]
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
