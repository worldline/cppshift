//! C++ Lexer to preprocess C++ code
//! This lexer is build out of [C++20 ISO standard](https://isocpp.org/files/papers/N4860.pdf) - Chapter 5. Lexical conventions

use std::{fmt, iter::Peekable, str::CharIndices};

use miette::Diagnostic;
use thiserror::Error;

use crate::SourceSpan;

/// Errors that can occur during lexing
#[derive(Clone, Diagnostic, Debug, Error)]
pub enum LexError {
    /// Unexpected token
    #[error("Unexpected token '{token}'")]
    SingleTokenError {
        #[source_code]
        src: String,
        token: char,
        #[label = "this input character"]
        err_span: miette::SourceSpan,
    },
    /// Wrong token separator
    #[error("Wrong token separator `{separator}`")]
    TokenSeparatorError {
        #[source_code]
        src: String,
        separator: char,
        #[label = "this input separator"]
        err_span: miette::SourceSpan,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TokenKind {
    /// Ident
    Ident,
    /// Number
    Number,
    /// String value
    String,
    /// Character value
    Char,
    /// Comment value
    Comment,
    /// Dot `.`
    Dot,
    /// Ellipsis `...`
    Ellipsis,
    /// Semicolon `;`
    Semicolon,
    /// Comma `,`
    Comma,
    /// Colon `:`
    Colon,
    /// Double Colon `::`
    DoubleColon,
    /// Equal `=`
    Equal,
    /// Star `*`
    Star,
    /// `(`
    LeftParenthese,
    /// `)`
    RightParenthese,
    /// `[`
    LeftBracket,
    /// `]`
    RightBracket,
    /// `[[`
    DoubleLeftBracket,
    /// `]]`
    DoubleRightBracket,
    /// `{`
    LeftBrace,
    /// `}`
    RightBrace,
    /// `<`
    LeftChevron,
    /// `>`
    RightChevron,
    /// `#`
    NumberSign,
    /// `##`
    DoubleNumberSign,
    /// And `&&`
    And,
    /// Bit Or `|`
    BitOr,
    /// Or `||`
    Or,
    /// Xor `^`
    Xor,
    /// Compl `~`
    Compl,
    /// Bit And `&`
    BitAnd,
    /// And Eq `&=`
    AndEq,
    /// Or Eq `|=`
    OrEq,
    /// Xor Eq `^=`
    XorEq,
    /// Not `!`
    Not,
    /// Ternary `?`
    Ternary,

    /// Plus `+`
    Plus,
    /// Minus `-`
    Minus,
    /// Div `/`
    Div,
    /// Modulo `%`
    Modulo,
    /// Increment `++`
    Increment,
    /// Decrement `--`
    Decrement,
    /// `<<`
    ShiftLeft,
    /// `>>`
    ShiftRight,
    /// `+=`
    CompoundAdd,
    /// `-=`
    CompoundSub,
    /// `*=`
    CompoundMult,
    /// `/=`
    CompoundDiv,
    /// `%=`
    CompoundModulo,
    /// `<<=`
    CompoundShiftLeft,
    /// `>>=`
    CompoundShiftRight,
    /// `&=`
    CompoundAnd,
    /// `^=`
    CompoundXor,
    /// `|=`
    CompoundOr,

    /// `==`
    EqualTo,
    /// `!=`
    NotEqualTo,
    /// `<=`
    LessOrEqualTo,
    /// `>=`
    GreaterOrEqualTo,
    /// `<=>`
    Compare,

    /// `->`
    PointerMember,
    /// `.*`
    PointerObjMember,
    /// `->*`
    PointerObjAccess,

    // Keywords
    KeywordAlignas,
    KeywordAlignof,
    KeywordAsm,
    KeywordAuto,
    KeywordBool,
    KeywordBreak,
    KeywordCase,
    KeywordCatch,
    KeywordChar,
    KeywordChar8,
    KeywordChar16,
    KeywordChar32,
    KeywordClass,
    KeywordConcept,
    KeywordConst,
    KeywordConsteval,
    KeywordConstexpr,
    KeywordConstinit,
    KeywordConstCast,
    KeywordContinue,
    KeywordCoAwait,
    KeywordCoReturn,
    KeywordCoYield,
    KeywordDecltype,
    KeywordDefault,
    KeywordDelete,
    KeywordDo,
    KeywordDouble,
    KeywordDynamicCast,
    KeywordElse,
    KeywordEnum,
    KeywordExplicit,
    KeywordExport,
    KeywordExtern,
    KeywordFalse,
    KeywordFinal,
    KeywordFloat,
    KeywordFor,
    KeywordFriend,
    KeywordGoto,
    KeywordIf,
    KeywordInline,
    KeywordInt,
    KeywordImport,
    KeywordLong,
    KeywordModule,
    KeywordMutable,
    KeywordNamespace,
    KeywordNew,
    KeywordNoexcept,
    KeywordNullptr,
    KeywordOperator,
    KeywordOverride,
    KeywordPrivate,
    KeywordProtected,
    KeywordPublic,
    KeywordRegister,
    KeywordReinterpretCast,
    KeywordRequires,
    KeywordReturn,
    KeywordShort,
    KeywordSigned,
    KeywordSizeof,
    KeywordStatic,
    KeywordStaticAssert,
    KeywordStaticCast,
    KeywordStruct,
    KeywordSwitch,
    KeywordTemplate,
    KeywordThis,
    KeywordThreadLocal,
    KeywordThrow,
    KeywordTrue,
    KeywordTry,
    KeywordTypedef,
    KeywordTypeid,
    KeywordTypename,
    KeywordUnion,
    KeywordUnsigned,
    KeywordUsing,
    KeywordVirtual,
    KeywordVoid,
    KeywordVolatile,
    KeywordWchar,
    KeywordWhile,
}

/// Token from [`Lexer`] parsing
/// Its lifetime is bound to the source code
///
/// ```
/// use cppshift::{Lexer, lex::TokenKind};
///
/// let mut lexer = Lexer::new("#");
/// let token = lexer.next().and_then(|t| t.ok()).unwrap();
/// assert_eq!(TokenKind::NumberSign, token.kind());
/// assert_eq!("#", token.src());
/// assert_eq!(
///     "Token { src: \"#\", kind: NumberSign }".to_string(),
///     format!("{token:?}")
/// );
/// ```
#[derive(Clone, Copy, PartialEq)]
pub struct Token<'de> {
    src_span: SourceSpan<'de>,
    kind: TokenKind,
}

impl<'de> Token<'de> {
    pub(crate) fn new(src_span: SourceSpan<'de>, kind: TokenKind) -> Token<'de> {
        Token { src_span, kind }
    }

    fn new_ident(src_span: SourceSpan<'de>) -> Token<'de> {
        let kind = match src_span.src() {
            "alignas" => TokenKind::KeywordAlignas,
            "alignof" => TokenKind::KeywordAlignof,
            "and" => TokenKind::And,
            "and_eq" => TokenKind::AndEq,
            "asm" => TokenKind::KeywordAsm,
            "auto" => TokenKind::KeywordAuto,
            "bitand" => TokenKind::BitAnd,
            "bitor" => TokenKind::BitOr,
            "bool" => TokenKind::KeywordBool,
            "break" => TokenKind::KeywordBreak,
            "case" => TokenKind::KeywordCase,
            "catch" => TokenKind::KeywordCatch,
            "char" => TokenKind::KeywordChar,
            "char8_t" => TokenKind::KeywordChar8,
            "char16_t" => TokenKind::KeywordChar16,
            "char32_t" => TokenKind::KeywordChar32,
            "class" => TokenKind::KeywordClass,
            "compl" => TokenKind::Compl,
            "concept" => TokenKind::KeywordConcept,
            "const" => TokenKind::KeywordConst,
            "consteval" => TokenKind::KeywordConsteval,
            "constexpr" => TokenKind::KeywordConstexpr,
            "constinit" => TokenKind::KeywordConstinit,
            "const_cast" => TokenKind::KeywordConstCast,
            "continue" => TokenKind::KeywordContinue,
            "co_await" => TokenKind::KeywordCoAwait,
            "co_return" => TokenKind::KeywordCoReturn,
            "co_yield" => TokenKind::KeywordCoYield,
            "decltype" => TokenKind::KeywordDecltype,
            "default" => TokenKind::KeywordDefault,
            "delete" => TokenKind::KeywordDelete,
            "do" => TokenKind::KeywordDo,
            "double" => TokenKind::KeywordDouble,
            "dynamic_cast" => TokenKind::KeywordDynamicCast,
            "else" => TokenKind::KeywordElse,
            "enum" => TokenKind::KeywordEnum,
            "explicit" => TokenKind::KeywordExplicit,
            "export" => TokenKind::KeywordExport,
            "extern" => TokenKind::KeywordExtern,
            "false" => TokenKind::KeywordFalse,
            "final" => TokenKind::KeywordFinal,
            "float" => TokenKind::KeywordFloat,
            "for" => TokenKind::KeywordFor,
            "friend" => TokenKind::KeywordFriend,
            "goto" => TokenKind::KeywordGoto,
            "if" => TokenKind::KeywordIf,
            "inline" => TokenKind::KeywordInline,
            "int" => TokenKind::KeywordInt,
            "import" => TokenKind::KeywordImport,
            "long" => TokenKind::KeywordLong,
            "module" => TokenKind::KeywordModule,
            "mutable" => TokenKind::KeywordMutable,
            "namespace" => TokenKind::KeywordNamespace,
            "new" => TokenKind::KeywordNew,
            "noexcept" => TokenKind::KeywordNoexcept,
            "not" => TokenKind::Not,
            "not_eq" => TokenKind::NotEqualTo,
            "nullptr" => TokenKind::KeywordNullptr,
            "operator" => TokenKind::KeywordOperator,
            "override" => TokenKind::KeywordOverride,
            "or" => TokenKind::Or,
            "or_eq" => TokenKind::OrEq,
            "private" => TokenKind::KeywordPrivate,
            "protected" => TokenKind::KeywordProtected,
            "public" => TokenKind::KeywordPublic,
            "register" => TokenKind::KeywordRegister,
            "reinterpret_cast" => TokenKind::KeywordReinterpretCast,
            "requires" => TokenKind::KeywordRequires,
            "return" => TokenKind::KeywordReturn,
            "short" => TokenKind::KeywordShort,
            "signed" => TokenKind::KeywordSigned,
            "sizeof" => TokenKind::KeywordSizeof,
            "static" => TokenKind::KeywordStatic,
            "static_assert" => TokenKind::KeywordStaticAssert,
            "static_cast" => TokenKind::KeywordStaticCast,
            "struct" => TokenKind::KeywordStruct,
            "switch" => TokenKind::KeywordSwitch,
            "template" => TokenKind::KeywordTemplate,
            "this" => TokenKind::KeywordThis,
            "thread_local" => TokenKind::KeywordThreadLocal,
            "throw" => TokenKind::KeywordThrow,
            "true" => TokenKind::KeywordTrue,
            "try" => TokenKind::KeywordTry,
            "typedef" => TokenKind::KeywordTypedef,
            "typeid" => TokenKind::KeywordTypeid,
            "typename" => TokenKind::KeywordTypename,
            "union" => TokenKind::KeywordUnion,
            "unsigned" => TokenKind::KeywordUnsigned,
            "using" => TokenKind::KeywordUsing,
            "virtual" => TokenKind::KeywordVirtual,
            "void" => TokenKind::KeywordVoid,
            "volatile" => TokenKind::KeywordVolatile,
            "wchar_t" => TokenKind::KeywordWchar,
            "while" => TokenKind::KeywordWhile,
            "xor" => TokenKind::Xor,
            "xor_eq" => TokenKind::XorEq,
            _ => TokenKind::Ident,
        };

        Token { src_span, kind }
    }

    /// Getter of the kind of the token
    pub fn kind(&self) -> TokenKind {
        self.kind
    }

    /// Getter of the token src value
    pub fn src(&self) -> &'de str {
        self.src_span.src()
    }

    /// Getter of the token src span value
    pub fn src_span(&self) -> SourceSpan<'de> {
        self.src_span
    }
}

impl<'de> fmt::Debug for Token<'de> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Token")
            .field("src", &self.src())
            .field("kind", &self.kind)
            .finish()
    }
}

/// Lexer to parse C++ source code
/// It's created from string source code and is used as an [`Iterator`]
///
/// ```
/// use cppshift::Lexer;
///
/// let main = r#"
///     #include <iostream>
///
///     int main(int argc, char* argv[]) {
///         std::cout << "Hello, world" << std::endl;
///         return 0;
///     }
/// "#;
/// let main_lex = Lexer::new(main);
/// for token in main_lex {
///     assert!(
///         token.is_ok(),
///         "token can't be parsed: {}",
///         token.err().unwrap()
///     );
/// }
/// ```
#[derive(Clone)]
pub struct Lexer<'de> {
    /// Source file content
    src: &'de str,
    /// Remaining content to parse
    rest: Peekable<CharIndices<'de>>,
}

macro_rules! new_token {
    // New token with its kind
    ($self:ident, $offset:expr, $len:expr, $kind:expr) => {
        Some(Ok(Token::new(
            SourceSpan::new($self.src, $offset, $len),
            $kind,
        )))
    };
    // New ident
    ($self:ident, $offset:expr, $len:expr) => {
        Some(Ok(Token::new_ident(SourceSpan::new(
            $self.src, $offset, $len,
        ))))
    };
    // New token from a source span
    ($src_span:expr, $kind:expr) => {
        Some(Ok(Token::new($src_span, $kind)))
    };
}

impl<'de> Lexer<'de> {
    /// Initialize a Lexer from a C++ source code
    ///
    /// ```no_run
    /// use std::fs;
    ///
    /// let cpp_src = fs::read_to_string("main.cpp").unwrap();
    /// let lexer = cppshift::Lexer::new(cpp_src.as_str());
    ///
    /// // Use the Lexer as an iterator
    /// ```
    pub fn new(input: &'de str) -> Self {
        Self {
            src: input,
            rest: input.char_indices().peekable(),
        }
    }
}

impl<'de> Iterator for Lexer<'de> {
    type Item = Result<Token<'de>, LexError>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((c_at, c)) = self.rest.next() {
            match c {
                'a'..='z' | 'A'..='Z' | '_' => {
                    let mut end_offset = c_at + c.len_utf8();
                    while let Some((offset, c)) = self.rest.peek() {
                        if matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_') {
                            end_offset = offset + c.len_utf8();
                            self.rest.next();
                        } else {
                            break;
                        }
                    }
                    return new_token!(self, c_at, end_offset - c_at);
                }
                '0'..='9' => {
                    let mut end_offset = c_at + 1;
                    // Check for hex (0x), binary (0b), or octal prefix
                    if c == '0' {
                        if let Some((_, next_c)) = self.rest.peek() {
                            match next_c {
                                'x' | 'X' => {
                                    self.rest.next();
                                    end_offset += 1;
                                    // Parse hex digits
                                    while let Some((offset, c)) = self.rest.peek() {
                                        if matches!(c, '0'..='9' | 'a'..='f' | 'A'..='F' | '\'') {
                                            end_offset = offset + c.len_utf8();
                                            self.rest.next();
                                        } else {
                                            break;
                                        }
                                    }
                                }
                                'b' | 'B' => {
                                    self.rest.next();
                                    end_offset += 1;
                                    // Parse binary digits
                                    while let Some((offset, c)) = self.rest.peek() {
                                        if matches!(c, '0' | '1' | '\'') {
                                            end_offset = offset + c.len_utf8();
                                            self.rest.next();
                                        } else {
                                            break;
                                        }
                                    }
                                }
                                _ => {
                                    // Decimal or octal - parse all valid chars
                                    while let Some((offset, c)) = self.rest.peek() {
                                        if matches!(c, '0'..='9' | '.' | 'e' | 'E' | '+' | '-' | '\'' | 'a'..='z' | 'A'..='Z')
                                        {
                                            end_offset = offset + c.len_utf8();
                                            self.rest.next();
                                        } else {
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        // Regular decimal number with possible exponent and suffix
                        while let Some((offset, c)) = self.rest.peek() {
                            if matches!(c, '0'..='9' | '.' | 'e' | 'E' | '+' | '-' | '\'' | 'a'..='z' | 'A'..='Z')
                            {
                                end_offset = offset + c.len_utf8();
                                self.rest.next();
                            } else {
                                break;
                            }
                        }
                    }

                    // Parse any remaining suffix (u, l, ll, ul, etc.)
                    while let Some((offset, c)) = self.rest.peek() {
                        if matches!(c, 'u' | 'U' | 'l' | 'L' | 'f' | 'F' | 'z' | 'Z') {
                            end_offset = offset + c.len_utf8();
                            self.rest.next();
                        } else {
                            break;
                        }
                    }

                    return new_token!(self, c_at, end_offset - c_at, TokenKind::Number);
                }
                '{' => return new_token!(self, c_at, 1, TokenKind::LeftBrace),
                '}' => return new_token!(self, c_at, 1, TokenKind::RightBrace),
                '[' => {
                    if let Some((_, '[')) = self.rest.peek() {
                        self.rest.next();
                        return new_token!(self, c_at, 2, TokenKind::DoubleLeftBracket);
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::LeftBracket);
                    }
                }
                ']' => {
                    if let Some((_, ']')) = self.rest.peek() {
                        self.rest.next();
                        return new_token!(self, c_at, 2, TokenKind::DoubleRightBracket);
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::RightBracket);
                    }
                }
                '#' => {
                    if let Some((_, '#')) = self.rest.peek() {
                        self.rest.next();
                        return new_token!(self, c_at, 2, TokenKind::DoubleNumberSign);
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::NumberSign);
                    }
                }
                '(' => {
                    return new_token!(self, c_at, 1, TokenKind::LeftParenthese);
                }
                ')' => {
                    return new_token!(self, c_at, 1, TokenKind::RightParenthese);
                }
                '<' => {
                    if let Some((_, c)) = self.rest.peek() {
                        match c {
                            ':' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::LeftBracket);
                            }
                            '%' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::LeftBrace);
                            }
                            '=' => {
                                self.rest.next();
                                if let Some((_, '>')) = self.rest.peek() {
                                    self.rest.next();
                                    return new_token!(self, c_at, 3, TokenKind::Compare);
                                } else {
                                    return new_token!(self, c_at, 2, TokenKind::LessOrEqualTo);
                                }
                            }
                            '<' => {
                                self.rest.next();
                                if let Some((_, '=')) = self.rest.peek() {
                                    self.rest.next();
                                    return new_token!(self, c_at, 3, TokenKind::CompoundShiftLeft);
                                } else {
                                    return new_token!(self, c_at, 2, TokenKind::ShiftLeft);
                                }
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::LeftChevron);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::LeftChevron);
                    }
                }
                '>' => {
                    if let Some((_, c)) = self.rest.peek() {
                        match c {
                            '=' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::GreaterOrEqualTo);
                            }
                            '>' => {
                                self.rest.next();
                                if let Some((_, '=')) = self.rest.peek() {
                                    self.rest.next();
                                    return new_token!(
                                        self,
                                        c_at,
                                        3,
                                        TokenKind::CompoundShiftRight
                                    );
                                } else {
                                    return new_token!(self, c_at, 2, TokenKind::ShiftRight);
                                }
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::RightChevron);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::RightChevron);
                    }
                }
                '%' => {
                    if let Some((_, c)) = self.rest.peek() {
                        match c {
                            '=' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::CompoundModulo);
                            }
                            ':' => {
                                self.rest.next();
                                let mut chars_clone = self.rest.clone();
                                if let Some((_, '%')) = chars_clone.next()
                                    && let Some((_, ':')) = chars_clone.next()
                                {
                                    self.rest.next();
                                    self.rest.next();
                                    return new_token!(self, c_at, 4, TokenKind::DoubleNumberSign);
                                } else {
                                    return new_token!(self, c_at, 2, TokenKind::NumberSign);
                                }
                            }
                            '>' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::RightBrace);
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::Modulo);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Modulo);
                    }
                }
                ':' => {
                    if let Some((_, c)) = self.rest.peek() {
                        match c {
                            ':' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::DoubleColon);
                            }
                            '>' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::RightBracket);
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::Colon);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Colon);
                    }
                }
                ';' => return new_token!(self, c_at, 1, TokenKind::Semicolon),
                '.' => {
                    if let Some((_, next_c)) = self.rest.peek() {
                        match next_c {
                            '*' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::PointerObjMember);
                            }
                            '.' => {
                                self.rest.next();
                                if let Some((_, '.')) = self.rest.peek() {
                                    self.rest.next();
                                    return new_token!(self, c_at, 3, TokenKind::Ellipsis);
                                } else {
                                    // C++ can't contain two dots in a row.
                                    return Some(Err(LexError::SingleTokenError {
                                        src: self.src.to_string(),
                                        token: '.',
                                        err_span: SourceSpan::new(self.src, c_at, 2).into(),
                                    }));
                                }
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::Dot);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Dot);
                    }
                }
                '?' => return new_token!(self, c_at, 1, TokenKind::Ternary),
                '*' => {
                    if let Some((_, '=')) = self.rest.peek() {
                        self.rest.next();
                        return new_token!(self, c_at, 2, TokenKind::CompoundMult);
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Star);
                    }
                }
                '+' => {
                    if let Some((_, c)) = self.rest.peek() {
                        match c {
                            '=' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::CompoundAdd);
                            }
                            '+' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::Increment);
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::Plus);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Plus);
                    }
                }
                '-' => {
                    if let Some((_, c)) = self.rest.peek() {
                        match c {
                            '>' => {
                                self.rest.next();
                                if let Some((_, '*')) = self.rest.peek() {
                                    self.rest.next();
                                    return new_token!(self, c_at, 3, TokenKind::PointerObjAccess);
                                } else {
                                    return new_token!(self, c_at, 2, TokenKind::PointerMember);
                                }
                            }
                            '=' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::CompoundSub);
                            }
                            '-' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::Decrement);
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::Minus);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Minus);
                    }
                }
                '/' => {
                    if let Some((_, c)) = self.rest.peek() {
                        match c {
                            '*' => {
                                self.rest.next();
                                let mut last_offset = c_at + 2;
                                while let Some((offset, c)) = self.rest.next() {
                                    last_offset = offset + c.len_utf8();
                                    if c == '*'
                                        && let Some((end_offset, '/')) = self.rest.peek().copied()
                                    {
                                        self.rest.next();
                                        return new_token!(
                                            self,
                                            c_at,
                                            end_offset + 1 - c_at,
                                            TokenKind::Comment
                                        );
                                    }
                                }
                                // Unterminated comment - return what we have
                                return new_token!(
                                    self,
                                    c_at,
                                    last_offset - c_at,
                                    TokenKind::Comment
                                );
                            }
                            '/' => {
                                self.rest.next();
                                let mut last_offset = c_at + 2;
                                while let Some((offset, c)) = self.rest.peek().copied() {
                                    if c == '\n' {
                                        return new_token!(
                                            self,
                                            c_at,
                                            offset - c_at,
                                            TokenKind::Comment
                                        );
                                    }
                                    last_offset = offset + c.len_utf8();
                                    self.rest.next();
                                }

                                return new_token!(
                                    self,
                                    c_at,
                                    last_offset - c_at,
                                    TokenKind::Comment
                                );
                            }
                            '=' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::CompoundDiv);
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::Div);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Div);
                    }
                }
                '^' => {
                    if let Some((_, '=')) = self.rest.peek() {
                        self.rest.next();
                        return new_token!(self, c_at, 2, TokenKind::XorEq);
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Xor);
                    }
                }
                '&' => {
                    if let Some((_, c)) = self.rest.peek() {
                        match c {
                            '&' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::And);
                            }
                            '=' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::AndEq);
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::BitAnd);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::BitAnd);
                    }
                }
                '|' => {
                    if let Some((_, c)) = self.rest.peek() {
                        match c {
                            '|' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::Or);
                            }
                            '=' => {
                                self.rest.next();
                                return new_token!(self, c_at, 2, TokenKind::OrEq);
                            }
                            _ => {
                                return new_token!(self, c_at, 1, TokenKind::BitOr);
                            }
                        }
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::BitOr);
                    }
                }
                '~' => return new_token!(self, c_at, 1, TokenKind::Compl),
                '!' => {
                    if let Some((_, '=')) = self.rest.peek() {
                        self.rest.next();
                        return new_token!(self, c_at, 2, TokenKind::NotEqualTo);
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Not);
                    }
                }
                '=' => {
                    if let Some((_, '=')) = self.rest.peek() {
                        self.rest.next();
                        return new_token!(self, c_at, 2, TokenKind::EqualTo);
                    } else {
                        return new_token!(self, c_at, 1, TokenKind::Equal);
                    }
                }
                ',' => return new_token!(self, c_at, 1, TokenKind::Comma),
                '\\' => {
                    self.rest.next();
                    continue;
                }
                '"' => {
                    while let Some((offset, c)) = self.rest.next() {
                        if c == '\\' {
                            self.rest.next();
                        } else if c == '"' {
                            return new_token!(self, c_at, offset + 1 - c_at, TokenKind::String);
                        }
                    }
                }
                '\'' => {
                    if let Some((_, c)) = self.rest.next() {
                        if c == '\\'
                            && let Some((_, esc)) = self.rest.next()
                        {
                            // For hex (\xNN), octal (\0nn), and unicode (\uNNNN, \UNNNNNNNN)
                            // escapes, consume remaining digits of the sequence
                            match esc {
                                'x' => {
                                    while self
                                        .rest
                                        .peek()
                                        .is_some_and(|(_, ch)| ch.is_ascii_hexdigit())
                                    {
                                        self.rest.next();
                                    }
                                }
                                '0'..='7' => {
                                    for _ in 0..2 {
                                        if self
                                            .rest
                                            .peek()
                                            .is_some_and(|(_, ch)| matches!(ch, '0'..='7'))
                                        {
                                            self.rest.next();
                                        } else {
                                            break;
                                        }
                                    }
                                }
                                'u' => {
                                    for _ in 0..4 {
                                        if self
                                            .rest
                                            .peek()
                                            .is_some_and(|(_, ch)| ch.is_ascii_hexdigit())
                                        {
                                            self.rest.next();
                                        }
                                    }
                                }
                                'U' => {
                                    for _ in 0..8 {
                                        if self
                                            .rest
                                            .peek()
                                            .is_some_and(|(_, ch)| ch.is_ascii_hexdigit())
                                        {
                                            self.rest.next();
                                        }
                                    }
                                }
                                // Simple escapes (\n, \t, \\, \', \", etc.)
                                _ => {}
                            }
                        }

                        if let Some((offset, '\'')) = self.rest.next() {
                            return new_token!(self, c_at, offset + 1 - c_at, TokenKind::Char);
                        } else {
                            return Some(Err(LexError::TokenSeparatorError {
                                src: self.src.to_string(),
                                separator: '\'',
                                err_span: miette::SourceSpan::from(c_at..c_at + 1),
                            }));
                        }
                    }
                }
                c if c.is_whitespace() => continue,
                c => {
                    return Some(Err(LexError::SingleTokenError {
                        src: self.src.to_string(),
                        token: c,
                        err_span: miette::SourceSpan::from(c_at..c_at + 1),
                    }));
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test the lexer with the gtest main file from google
    #[tokio::test]
    async fn gtests_lex() {
        let gtest_src = reqwest::get("https://raw.githubusercontent.com/google/googletest/refs/heads/main/googletest/src/gtest.cc")
            .await.unwrap()
            .text()
            .await.unwrap();
        assert!(!gtest_src.is_empty());

        for gtest_token in Lexer::new(&gtest_src) {
            assert!(
                gtest_token.is_ok(),
                "token can't be parsed: {}",
                gtest_token.err().unwrap()
            );
        }
    }

    macro_rules! next_kind {
        ($l:ident) => {
            $l.next().and_then(|t| t.ok().map(|t| t.kind()))
        };
    }

    macro_rules! next_is_kind {
        ($l:ident, $k:expr) => {
            $l.next().is_some_and(|t| t.is_ok_and(|t| t.kind == $k))
        };
    }

    macro_rules! next_dbg {
        ($l:ident) => {
            $l.next().and_then(|t| t.ok().map(|t| format!("{t:?}")))
        };
    }

    #[test]
    fn main_lex() {
        let main = r#"
            #include <iostream>

            #define ArgText(x) \
                x##TEXT

            // main function
            int main(int argc, char* argv[]) {
                std::cout << "Hello, world" << std::endl;
                switch (argc)
                {
                    case 1:
                    case 2:
                        std::cout << "first and second" << std::endl;
                        [[fallthrough]];
                    case 3:
                        std::cout << "fallthrough" << std::endl;
                        break;
                }

                return 0;
            }
        "#;
        let mut main_lex = Lexer::new(main);
        assert_eq!(Some(TokenKind::NumberSign), next_kind!(main_lex));
        if let Some(Ok(include_token)) = main_lex.next() {
            let src_span = include_token.src_span();
            assert_eq!(SourceSpan::new(main, 14, 7), src_span);
            assert_eq!("include", src_span.src());
            assert_eq!(TokenKind::Ident, include_token.kind());
        } else {
            panic!("Can't parse the first include token");
        }

        assert_eq!(Some(TokenKind::LeftChevron), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"iostream\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::RightChevron), next_kind!(main_lex));

        assert_eq!(Some(TokenKind::NumberSign), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"define\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(
            Some("Token { src: \"ArgText\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::LeftParenthese), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"x\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::RightParenthese), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"x\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::DoubleNumberSign), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"TEXT\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );

        assert_eq!(
            Some("Token { src: \"// main function\", kind: Comment }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::KeywordInt), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"main\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::LeftParenthese), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::KeywordInt), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"argc\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::Comma), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::KeywordChar), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::Star), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"argv\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::LeftBracket), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::RightBracket), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::RightParenthese), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::LeftBrace), next_kind!(main_lex));

        assert_eq!(
            Some("Token { src: \"std\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::DoubleColon), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"cout\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::ShiftLeft), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"\\\"Hello, world\\\"\", kind: String }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::ShiftLeft), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"std\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::DoubleColon), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"endl\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::Semicolon), next_kind!(main_lex));

        assert_eq!(Some(TokenKind::KeywordSwitch), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::LeftParenthese), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"argc\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::RightParenthese), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::LeftBrace), next_kind!(main_lex));

        assert_eq!(Some(TokenKind::KeywordCase), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"1\", kind: Number }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::Colon), next_kind!(main_lex));

        assert_eq!(Some(TokenKind::KeywordCase), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"2\", kind: Number }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::Colon), next_kind!(main_lex));

        assert_eq!(
            Some("Token { src: \"std\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::DoubleColon), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"cout\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::ShiftLeft), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"\\\"first and second\\\"\", kind: String }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::ShiftLeft), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"std\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::DoubleColon), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"endl\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::Semicolon), next_kind!(main_lex));

        assert_eq!(Some(TokenKind::DoubleLeftBracket), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"fallthrough\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::DoubleRightBracket), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::Semicolon), next_kind!(main_lex));

        assert_eq!(Some(TokenKind::KeywordCase), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"3\", kind: Number }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::Colon), next_kind!(main_lex));

        assert_eq!(
            Some("Token { src: \"std\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::DoubleColon), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"cout\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::ShiftLeft), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"\\\"fallthrough\\\"\", kind: String }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::ShiftLeft), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"std\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::DoubleColon), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"endl\", kind: Ident }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::Semicolon), next_kind!(main_lex));

        assert_eq!(Some(TokenKind::KeywordBreak), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::Semicolon), next_kind!(main_lex));

        assert_eq!(Some(TokenKind::RightBrace), next_kind!(main_lex));

        assert_eq!(Some(TokenKind::KeywordReturn), next_kind!(main_lex));
        assert_eq!(
            Some("Token { src: \"0\", kind: Number }".to_string()),
            next_dbg!(main_lex)
        );
        assert_eq!(Some(TokenKind::Semicolon), next_kind!(main_lex));
        assert_eq!(Some(TokenKind::RightBrace), next_kind!(main_lex));

        assert!(main_lex.next().is_none());
    }

    #[test]
    fn err_lex() {
        let wrong_token = "⚡";
        let mut wrong_token_lex = Lexer::new(wrong_token);
        assert_eq!(
            Some(Err(String::from("Unexpected token '⚡'"))),
            wrong_token_lex.next().map(|t| t.map_err(|e| e.to_string()))
        );

        let wrong_multiple_char = r#"char test = 'ab'"#;
        let mut wrong_mult_char_lex = Lexer::new(wrong_multiple_char);
        assert!(next_is_kind!(wrong_mult_char_lex, TokenKind::KeywordChar)); // `char`
        assert!(next_is_kind!(wrong_mult_char_lex, TokenKind::Ident)); // `test`
        assert!(next_is_kind!(wrong_mult_char_lex, TokenKind::Equal)); // `=`
        assert_eq!(
            Some(Err(String::from("Wrong token separator `'`"))),
            wrong_mult_char_lex
                .next()
                .map(|t| t.map_err(|e| e.to_string()))
        );

        let wrong_mult_char2 = r#"char test = '\'a'"#;
        let mut wrong_mult_char2_lex = Lexer::new(wrong_mult_char2);
        assert!(next_is_kind!(wrong_mult_char2_lex, TokenKind::KeywordChar)); // `char`
        assert!(next_is_kind!(wrong_mult_char2_lex, TokenKind::Ident)); // `test`
        assert!(next_is_kind!(wrong_mult_char2_lex, TokenKind::Equal)); // `=`
        assert_eq!(
            Some(Err(String::from("Wrong token separator `'`"))),
            wrong_mult_char2_lex
                .next()
                .map(|t| t.map_err(|e| e.to_string()))
        );

        let wrong_two_dot = r#"var..method();"#;
        let mut wrong_two_dot_lex = Lexer::new(wrong_two_dot);
        assert!(next_is_kind!(wrong_two_dot_lex, TokenKind::Ident)); // `var`
        assert_eq!(
            Some(Err(String::from("Unexpected token '.'"))),
            wrong_two_dot_lex
                .next()
                .map(|t| t.map_err(|e| e.to_string()))
        );
    }

    #[test]
    fn alternate_lex() {
        let alternate = r#"
            %:include <iostream>

            %:define ArgText(x) x%:%:TEXT

            int main(int argc, char* argv<::>) <%
                std::cout << "Hello, world" << std::endl;
                return 0;
            %>
        "#;

        let mut alternate_lex = Lexer::new(alternate);
        assert_eq!(Some(TokenKind::NumberSign), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"include\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::LeftChevron), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"iostream\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::RightChevron), next_kind!(alternate_lex));

        assert_eq!(Some(TokenKind::NumberSign), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"define\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(
            Some("Token { src: \"ArgText\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::LeftParenthese), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"x\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::RightParenthese), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"x\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::DoubleNumberSign), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"TEXT\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );

        assert_eq!(Some(TokenKind::KeywordInt), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"main\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::LeftParenthese), next_kind!(alternate_lex));
        assert_eq!(Some(TokenKind::KeywordInt), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"argc\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::Comma), next_kind!(alternate_lex));
        assert_eq!(Some(TokenKind::KeywordChar), next_kind!(alternate_lex));
        assert_eq!(Some(TokenKind::Star), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"argv\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::LeftBracket), next_kind!(alternate_lex));
        assert_eq!(Some(TokenKind::RightBracket), next_kind!(alternate_lex));
        assert_eq!(Some(TokenKind::RightParenthese), next_kind!(alternate_lex));
        assert_eq!(Some(TokenKind::LeftBrace), next_kind!(alternate_lex));

        assert_eq!(
            Some("Token { src: \"std\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::DoubleColon), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"cout\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::ShiftLeft), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"\\\"Hello, world\\\"\", kind: String }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::ShiftLeft), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"std\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::DoubleColon), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"endl\", kind: Ident }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::Semicolon), next_kind!(alternate_lex));

        assert_eq!(Some(TokenKind::KeywordReturn), next_kind!(alternate_lex));
        assert_eq!(
            Some("Token { src: \"0\", kind: Number }".to_string()),
            next_dbg!(alternate_lex)
        );
        assert_eq!(Some(TokenKind::Semicolon), next_kind!(alternate_lex));
        assert_eq!(Some(TokenKind::RightBrace), next_kind!(alternate_lex));

        assert!(alternate_lex.next().is_none());
    }

    #[test]
    fn conditions_lex() {
        macro_rules! if_cond {
            ($l:ident, $t:expr) => {
                assert_eq!(
                    Some(TokenKind::KeywordIf),
                    $l.next().and_then(|t| t.ok().map(|t| t.kind()))
                );
                assert_eq!(
                    Some(TokenKind::LeftParenthese),
                    $l.next().and_then(|t| t.ok().map(|t| t.kind()))
                );
                $t
                assert_eq!(
                    Some(TokenKind::RightParenthese),
                    $l.next().and_then(|t| t.ok().map(|t| t.kind()))
                );
                assert_eq!(
                    Some(TokenKind::Semicolon),
                    $l.next().and_then(|t| t.ok().map(|t| t.kind()))
                );
            };
        }

        let conditions = r#"
            if (true && false);
            if (true and false);
            if (true || false);
            if (true or false);
            if (!false);
            if (not false);
            if (1 == 1);
            if (1 != 2);
            if (1 not_eq 2);
            if (2 - 1 <=> 2);
            if (1 + 1 < 3);
            if (1 * 1 <= 2);
            if (2 / 1 > 1);
            if (2 % 2 >= 0);
        "#;

        let mut conditions_lex = Lexer::new(conditions);

        for k in [TokenKind::And, TokenKind::Or] {
            for _ in 0..2 {
                if_cond!(conditions_lex, {
                    assert_eq!(Some(TokenKind::KeywordTrue), next_kind!(conditions_lex));
                    assert_eq!(Some(k), next_kind!(conditions_lex));
                    assert_eq!(Some(TokenKind::KeywordFalse), next_kind!(conditions_lex));
                });
            }
        }

        for _ in 0..2 {
            if_cond!(conditions_lex, {
                assert_eq!(Some(TokenKind::Not), next_kind!(conditions_lex));
                assert_eq!(Some(TokenKind::KeywordFalse), next_kind!(conditions_lex));
            });
        }

        if_cond!(conditions_lex, {
            assert_eq!(
                Some("Token { src: \"1\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::EqualTo), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"1\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
        });

        for _ in 0..2 {
            if_cond!(conditions_lex, {
                assert_eq!(
                    Some("Token { src: \"1\", kind: Number }".to_string()),
                    next_dbg!(conditions_lex)
                );
                assert_eq!(Some(TokenKind::NotEqualTo), next_kind!(conditions_lex));
                assert_eq!(
                    Some("Token { src: \"2\", kind: Number }".to_string()),
                    next_dbg!(conditions_lex)
                );
            });
        }

        if_cond!(conditions_lex, {
            assert_eq!(
                Some("Token { src: \"2\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::Minus), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"1\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::Compare), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"2\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
        });

        if_cond!(conditions_lex, {
            assert_eq!(
                Some("Token { src: \"1\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::Plus), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"1\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::LeftChevron), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"3\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
        });

        if_cond!(conditions_lex, {
            assert_eq!(
                Some("Token { src: \"1\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::Star), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"1\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::LessOrEqualTo), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"2\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
        });

        if_cond!(conditions_lex, {
            assert_eq!(
                Some("Token { src: \"2\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::Div), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"1\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::RightChevron), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"1\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
        });

        if_cond!(conditions_lex, {
            assert_eq!(
                Some("Token { src: \"2\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(Some(TokenKind::Modulo), next_kind!(conditions_lex));
            assert_eq!(
                Some("Token { src: \"2\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
            assert_eq!(
                Some(TokenKind::GreaterOrEqualTo),
                next_kind!(conditions_lex)
            );
            assert_eq!(
                Some("Token { src: \"0\", kind: Number }".to_string()),
                next_dbg!(conditions_lex)
            );
        });

        assert!(conditions_lex.next().is_none());
    }

    #[test]
    fn operations_lex() {
        let operations = r#"
            int x = 2;
            x++;
            x--;

            x = x | 2;
            x = x bitor 2;
            x = x & 2;
            x = x bitand 2;
            x = x ^ 2;
            x = x xor 2;
            x = x ~ 2;
            x = x compl 2;
            x = x << 2;
            x = x >> 2;

            x += 2;
            x -= 2;
            x *= 2;
            x /= 2;
            x %= 2;
            x |= 2;
            x or_eq 2;
            x &= 2;
            x and_eq 2;
            x ^= 2;
            x xor_eq 2;
            x <<= 2;
            x >>= 2;

            a->b;
            a->*b;
            a.*b;
        "#;

        let mut operations_lex = Lexer::new(operations);
        assert_eq!(Some(TokenKind::KeywordInt), next_kind!(operations_lex));
        assert_eq!(
            Some("Token { src: \"x\", kind: Ident }".to_string()),
            next_dbg!(operations_lex)
        );
        assert_eq!(Some(TokenKind::Equal), next_kind!(operations_lex));
        assert_eq!(
            Some("Token { src: \"2\", kind: Number }".to_string()),
            next_dbg!(operations_lex)
        );
        assert_eq!(Some(TokenKind::Semicolon), next_kind!(operations_lex));

        for k in [TokenKind::Increment, TokenKind::Decrement] {
            assert_eq!(
                Some("Token { src: \"x\", kind: Ident }".to_string()),
                next_dbg!(operations_lex)
            );
            assert_eq!(Some(k), next_kind!(operations_lex));
            assert_eq!(Some(TokenKind::Semicolon), next_kind!(operations_lex));
        }

        for operand in [
            TokenKind::BitOr,
            TokenKind::BitOr,
            TokenKind::BitAnd,
            TokenKind::BitAnd,
            TokenKind::Xor,
            TokenKind::Xor,
            TokenKind::Compl,
            TokenKind::Compl,
            TokenKind::ShiftLeft,
            TokenKind::ShiftRight,
        ] {
            assert_eq!(
                Some("Token { src: \"x\", kind: Ident }".to_string()),
                next_dbg!(operations_lex)
            );
            assert_eq!(Some(TokenKind::Equal), next_kind!(operations_lex));
            assert_eq!(
                Some("Token { src: \"x\", kind: Ident }".to_string()),
                next_dbg!(operations_lex)
            );
            assert_eq!(Some(operand), next_kind!(operations_lex));
            assert_eq!(
                Some("Token { src: \"2\", kind: Number }".to_string()),
                next_dbg!(operations_lex)
            );
            assert_eq!(Some(TokenKind::Semicolon), next_kind!(operations_lex));
        }

        for operand in [
            TokenKind::CompoundAdd,
            TokenKind::CompoundSub,
            TokenKind::CompoundMult,
            TokenKind::CompoundDiv,
            TokenKind::CompoundModulo,
            TokenKind::OrEq,
            TokenKind::OrEq,
            TokenKind::AndEq,
            TokenKind::AndEq,
            TokenKind::XorEq,
            TokenKind::XorEq,
            TokenKind::CompoundShiftLeft,
            TokenKind::CompoundShiftRight,
        ] {
            assert_eq!(
                Some("Token { src: \"x\", kind: Ident }".to_string()),
                next_dbg!(operations_lex)
            );
            assert_eq!(Some(operand), next_kind!(operations_lex));
            assert_eq!(
                Some("Token { src: \"2\", kind: Number }".to_string()),
                next_dbg!(operations_lex)
            );
            assert_eq!(Some(TokenKind::Semicolon), next_kind!(operations_lex));
        }

        for k in [
            TokenKind::PointerMember,
            TokenKind::PointerObjAccess,
            TokenKind::PointerObjMember,
        ] {
            assert_eq!(
                Some("Token { src: \"a\", kind: Ident }".to_string()),
                next_dbg!(operations_lex)
            );
            assert_eq!(Some(k), next_kind!(operations_lex));
            assert_eq!(
                Some("Token { src: \"b\", kind: Ident }".to_string()),
                next_dbg!(operations_lex)
            );
            assert_eq!(Some(TokenKind::Semicolon), next_kind!(operations_lex));
        }

        assert!(operations_lex.next().is_none());
    }

    #[test]
    fn ellipsis_lex() {
        let variadic = "template<typename... Args>";
        let mut lex = Lexer::new(variadic);
        assert_eq!(Some(TokenKind::KeywordTemplate), next_kind!(lex));
        assert_eq!(Some(TokenKind::LeftChevron), next_kind!(lex));
        assert_eq!(Some(TokenKind::KeywordTypename), next_kind!(lex));
        assert_eq!(Some(TokenKind::Ellipsis), next_kind!(lex));
        assert_eq!(
            Some("Token { src: \"Args\", kind: Ident }".to_string()),
            next_dbg!(lex)
        );
        assert_eq!(Some(TokenKind::RightChevron), next_kind!(lex));
        assert!(lex.next().is_none());
    }

    #[test]
    fn numbers_lex() {
        // Hexadecimal
        let mut lex = Lexer::new("0xFF 0x1a2B");
        assert_eq!(
            Some("Token { src: \"0xFF\", kind: Number }".to_string()),
            next_dbg!(lex)
        );
        assert_eq!(
            Some("Token { src: \"0x1a2B\", kind: Number }".to_string()),
            next_dbg!(lex)
        );

        // Binary
        let mut lex = Lexer::new("0b1010 0B1111");
        assert_eq!(
            Some("Token { src: \"0b1010\", kind: Number }".to_string()),
            next_dbg!(lex)
        );
        assert_eq!(
            Some("Token { src: \"0B1111\", kind: Number }".to_string()),
            next_dbg!(lex)
        );

        // Digit separators
        let mut lex = Lexer::new("1'000'000");
        assert_eq!(
            Some("Token { src: \"1'000'000\", kind: Number }".to_string()),
            next_dbg!(lex)
        );

        // Suffixes
        let mut lex = Lexer::new("1u 2UL 3ull 4.0f");
        assert_eq!(
            Some("Token { src: \"1u\", kind: Number }".to_string()),
            next_dbg!(lex)
        );
        assert_eq!(
            Some("Token { src: \"2UL\", kind: Number }".to_string()),
            next_dbg!(lex)
        );
        assert_eq!(
            Some("Token { src: \"3ull\", kind: Number }".to_string()),
            next_dbg!(lex)
        );
        assert_eq!(
            Some("Token { src: \"4.0f\", kind: Number }".to_string()),
            next_dbg!(lex)
        );

        // Scientific notation
        let mut lex = Lexer::new("1e10 2.5E-3");
        assert_eq!(
            Some("Token { src: \"1e10\", kind: Number }".to_string()),
            next_dbg!(lex)
        );
        assert_eq!(
            Some("Token { src: \"2.5E-3\", kind: Number }".to_string()),
            next_dbg!(lex)
        );
    }

    #[test]
    fn eof_tokens_lex() {
        // Test tokens at EOF without trailing whitespace
        let mut lex = Lexer::new("test");
        assert_eq!(
            Some("Token { src: \"test\", kind: Ident }".to_string()),
            next_dbg!(lex)
        );

        let mut lex = Lexer::new("123");
        assert_eq!(
            Some("Token { src: \"123\", kind: Number }".to_string()),
            next_dbg!(lex)
        );

        let mut lex = Lexer::new("<");
        assert_eq!(Some(TokenKind::LeftChevron), next_kind!(lex));

        let mut lex = Lexer::new(">");
        assert_eq!(Some(TokenKind::RightChevron), next_kind!(lex));

        let mut lex = Lexer::new("+");
        assert_eq!(Some(TokenKind::Plus), next_kind!(lex));

        let mut lex = Lexer::new("-");
        assert_eq!(Some(TokenKind::Minus), next_kind!(lex));

        let mut lex = Lexer::new("&");
        assert_eq!(Some(TokenKind::BitAnd), next_kind!(lex));

        let mut lex = Lexer::new("|");
        assert_eq!(Some(TokenKind::BitOr), next_kind!(lex));

        let mut lex = Lexer::new(":");
        assert_eq!(Some(TokenKind::Colon), next_kind!(lex));

        let mut lex = Lexer::new("/");
        assert_eq!(Some(TokenKind::Div), next_kind!(lex));

        let mut lex = Lexer::new("%");
        assert_eq!(Some(TokenKind::Modulo), next_kind!(lex));

        // Comment at EOF without newline
        let mut lex = Lexer::new("// comment at eof");
        assert_eq!(
            Some("Token { src: \"// comment at eof\", kind: Comment }".to_string()),
            next_dbg!(lex)
        );
    }

    #[test]
    fn peekable_lexer() {
        let mut lex = Lexer::new("int x;").peekable();

        // Peek should return the token without consuming
        assert_eq!(
            Some(TokenKind::KeywordInt),
            lex.peek().and_then(|t| t.as_ref().ok().map(|t| t.kind()))
        );
        assert_eq!(
            Some(TokenKind::KeywordInt),
            lex.peek().and_then(|t| t.as_ref().ok().map(|t| t.kind()))
        );

        lex.next();
        assert_ne!(
            Some(TokenKind::KeywordInt),
            lex.peek().and_then(|t| t.as_ref().ok().map(|t| t.kind()))
        );
    }
}
