//! Parse error types with rich diagnostics via miette

use miette::Diagnostic;
use thiserror::Error;

use crate::lex::TokenKind;

/// Errors that can occur during parsing
#[derive(Diagnostic, Debug, Error)]
pub enum ParseError {
    /// Unexpected end of input
    #[error("Unexpected end of input, expected {expected}")]
    UnexpectedEof {
        expected: String,
        #[source_code]
        src: String,
        #[label = "input ends here"]
        err_span: miette::SourceSpan,
    },

    /// Wrong token type encountered
    #[error("Expected {expected}, found `{found}`")]
    UnexpectedToken {
        expected: String,
        found: TokenKind,
        #[source_code]
        src: String,
        #[label = "this token"]
        err_span: miette::SourceSpan,
    },

    /// Custom parse error with a message
    #[error("{message}")]
    Custom {
        message: String,
        #[source_code]
        src: String,
        #[label = "{message}"]
        err_span: miette::SourceSpan,
    },

    /// Lexer error forwarded during ParseStream construction
    #[error("Lexer error: {0}")]
    LexerError(String),
}
