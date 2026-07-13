use crate::diag::DiagCtx;
use crate::lexer::{
    src_span::SrcSpan,
    token::{Token, TokenKind},
};

pub mod src_span;
pub mod token;

pub struct Lexer<'a> {
    src: &'a Vec<char>,
    file_offset: usize,
    cursor: usize,
    lexeme_pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src_text: &'a Vec<char>, file_offset: usize) -> Lexer<'a> {
        Lexer {
            src: src_text,
            file_offset,
            cursor: 0,
            lexeme_pos: 0,
        }
    }

    /// Record a diagnostic pointing at the span of the lexeme currently being scanned.
    fn error(&self, message: impl Into<String>) {
        let span = SrcSpan::new(
            self.file_offset + self.lexeme_pos,
            self.file_offset + self.cursor,
        );
        DiagCtx::error(message, span);
    }

    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut token_stream = Vec::new();

        loop {
            self.skip_trivia();
            if self.at_eof() {
                break;
            }

            self.lexeme_pos = self.cursor;
            token_stream.push(self.scan());
        }

        token_stream
    }

    /// Consumes whitespace and comments. Used both between tokens and to resynchronize after
    /// an unexpected character, so trivia is never accidentally re-lexed as a token.
    fn skip_trivia(&mut self) {
        loop {
            if self.peek().is_ascii_whitespace() {
                self.eat();
                continue;
            }

            // Line comment: `//...`
            if self.peek() == '/' && self.next() == '/' {
                self.eat();
                self.eat();
                while !self.at_eof() && self.peek() != '\n' {
                    self.eat();
                }
                continue;
            }

            // Block comment: `/* ... */`
            if self.peek() == '/' && self.next() == '*' {
                self.lexeme_pos = self.cursor;
                self.eat();
                self.eat();
                loop {
                    if self.at_eof() {
                        self.error("unterminated block comment");
                        break;
                    }

                    if self.peek() == '*' && self.next() == '/' {
                        self.eat(); // consume '*'
                        self.eat(); // consume '/'
                        break;
                    }
                    self.eat();
                }
                continue;
            }

            break;
        }
    }

    pub fn scan(&mut self) -> Token {
        match self.eat() {
            '(' => self.make_token(TokenKind::OpenParen),
            ')' => self.make_token(TokenKind::CloseParen),
            '{' => self.make_token(TokenKind::OpenBrace),
            '}' => self.make_token(TokenKind::CloseBrace),
            '[' => self.make_token(TokenKind::OpenBracket),
            ']' => self.make_token(TokenKind::CloseBracket),
            ',' => self.make_token(TokenKind::Comma),
            ';' => self.make_token(TokenKind::Semicolon),

            '.' => {
                if self.match_next_n(".=") {
                    self.make_token(TokenKind::InclRange)
                } else if self.match_next('.').is_some() {
                    self.make_token(TokenKind::ExclRange)
                } else {
                    self.make_token(TokenKind::Period)
                }
            }
            ':' => {
                if self.match_next(':').is_some() {
                    self.make_token(TokenKind::DoubleColon)
                } else {
                    self.make_token(TokenKind::Colon)
                }
            }

            '+' => {
                if self.match_next('+').is_some() {
                    self.make_token(TokenKind::DoublePlus)
                } else if self.match_next('=').is_some() {
                    self.make_token(TokenKind::PlusEquals)
                } else {
                    self.make_token(TokenKind::Plus)
                }
            }
            '-' => {
                if self.match_next('>').is_some() {
                    self.make_token(TokenKind::Arrow)
                } else if self.match_next('-').is_some() {
                    self.make_token(TokenKind::DoubleMinus)
                } else if self.match_next('=').is_some() {
                    self.make_token(TokenKind::SubEquals)
                } else {
                    self.make_token(TokenKind::Minus)
                }
            }
            '*' => {
                let kind = if self.match_next('=').is_some() {
                    TokenKind::MulEquals
                } else {
                    TokenKind::Star
                };
                self.make_token(kind)
            }
            '/' => {
                let kind = if self.match_next('=').is_some() {
                    TokenKind::DivEquals
                } else {
                    TokenKind::Slash
                };
                self.make_token(kind)
            }
            '%' => {
                let kind = if self.match_next('=').is_some() {
                    TokenKind::ModEquals
                } else {
                    TokenKind::Percent
                };
                self.make_token(kind)
            }
            '!' => {
                let kind = if self.match_next('=').is_some() {
                    TokenKind::BangEquals
                } else {
                    TokenKind::Bang
                };
                self.make_token(kind)
            }
            '=' => {
                if self.match_next('>').is_some() {
                    self.make_token(TokenKind::FatArrow)
                } else if self.match_next('=').is_some() {
                    self.make_token(TokenKind::DoubleEquals)
                } else {
                    self.make_token(TokenKind::Equals)
                }
            }
            '<' => {
                let kind = if self.match_next('=').is_some() {
                    TokenKind::LessEqual
                } else {
                    TokenKind::OpenCaret
                };
                self.make_token(kind)
            }
            '>' => {
                let kind = if self.match_next('=').is_some() {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::CloseCaret
                };
                self.make_token(kind)
            }
            '&' => {
                if self.match_next('&').is_some() {
                    self.make_token(TokenKind::DoubleAmp)
                } else {
                    self.make_token(TokenKind::Amp)
                }
            }
            '|' => {
                if self.match_next('|').is_some() {
                    self.make_token(TokenKind::DoublePipe)
                } else {
                    self.make_token(TokenKind::Pipe)
                }
            }
            '?' => self.make_token(TokenKind::Try),
            '_' => {
                // A lone `_` is the wildcard token, but `_foo` is an identifier —
                // only decide once we know whether more identifier chars follow.
                if self.peek().is_ascii_alphanumeric() || self.peek() == '_' {
                    self.parse_identifier_or_kw()
                } else {
                    self.make_token(TokenKind::Wildcard)
                }
            }

            '"' => self.parse_string(),
            '\'' => self.parse_char(),

            c => {
                if c.is_ascii_alphabetic() || c == '_' {
                    self.parse_identifier_or_kw()
                } else if c.is_ascii_digit() {
                    self.parse_number()
                } else {
                    self.error(format!("unexpected character '{}'", c));
                    // Skip the bad character (and any trivia after it) and keep scanning so a
                    // single stray byte doesn't stop the whole file from being lexed.
                    self.skip_trivia();
                    self.lexeme_pos = self.cursor;
                    if self.at_eof() {
                        self.make_token(TokenKind::Eof)
                    } else {
                        self.scan()
                    }
                }
            }
        }
    }

    pub fn peek(&self) -> char {
        self.src.get(self.cursor).copied().unwrap_or('\0')
    }

    pub fn next(&self) -> char {
        self.src.get(self.cursor + 1).copied().unwrap_or('\0')
    }

    pub fn eat(&mut self) -> char {
        let temp = self.peek();
        self.cursor += 1;
        temp
    }

    pub fn match_next(&mut self, next: char) -> Option<char> {
        if self.at_eof() || self.peek() != next {
            None
        } else {
            self.cursor += 1;
            Some(next)
        }
    }

    /// Like [`Self::match_next`] but matches (and consumes) a multi-character lookahead.
    pub fn match_next_n(&mut self, expected: &str) -> bool {
        let expected: Vec<char> = expected.chars().collect();
        if self.cursor + expected.len() > self.src.len() {
            return false;
        }
        if self.src[self.cursor..self.cursor + expected.len()] == expected[..] {
            self.cursor += expected.len();
            true
        } else {
            false
        }
    }

    pub fn at_eof(&self) -> bool {
        self.cursor >= self.src.len()
    }

    pub fn make_token(&self, kind: TokenKind) -> Token {
        Token {
            kind,
            span: SrcSpan::new(
                self.file_offset + self.lexeme_pos,
                self.file_offset + self.cursor,
            ),
        }
    }

    fn parse_number(&mut self) -> Token {
        // integer part
        while self.peek().is_ascii_digit() || self.peek() == '_' {
            self.eat();
        }

        // fractional part: require "." followed by at least one digit
        if self.peek() == '.' && self.next().is_ascii_digit() {
            self.eat(); // consume '.'
            while self.peek().is_ascii_digit() {
                self.eat();
            }
        } else {
            return self.make_token(TokenKind::IntLiteral);
        }

        // TODO: exponent notation
        self.make_token(TokenKind::FloatLiteral)
    }

    fn parse_identifier_or_kw(&mut self) -> Token {
        while self.peek().is_ascii_alphanumeric() || self.peek() == '_' {
            self.eat();
        }

        // extract the identifier lexeme
        let ident: String = self.src[self.lexeme_pos..self.cursor].iter().collect();

        // keyword table
        let kind = match ident.as_str() {
            "any" => TokenKind::AnyKw,
            "as" => TokenKind::AsKw,
            "bool" => TokenKind::BoolKw,
            "break" => TokenKind::BreakKw,
            "concurrent" => TokenKind::ConcurrentKw,
            "continue" => TokenKind::ContinueKw,
            "defer" => TokenKind::DeferKw,
            "dyn" => TokenKind::DynKw,
            "else" => TokenKind::ElseKw,
            "enum" => TokenKind::EnumKw,
            "extend" => TokenKind::ExtendKw,
            "false" => TokenKind::FalseKw,
            "for" => TokenKind::ForKw,
            "fun" => TokenKind::FunKw,
            "if" => TokenKind::IfKw,
            "import" => TokenKind::ImportKw,
            "in" => TokenKind::InKw,
            "let" => TokenKind::LetKw,
            "match" => TokenKind::MatchKw,
            "module" => TokenKind::ModuleKw,
            "mut" => TokenKind::MutKw,
            "public" => TokenKind::PublicKw,
            "return" => TokenKind::ReturnKw,
            "self" => TokenKind::LowerSelfKw,
            "Self" => TokenKind::UpperSelfKw,
            "spawn" => TokenKind::SpawnKw,
            "struct" => TokenKind::StructKw,
            "trait" => TokenKind::TraitKw,
            "true" => TokenKind::TrueKw,
            "use" => TokenKind::UseKw,
            "while" => TokenKind::WhileKw,
            "with" => TokenKind::WithKw,
            "i8" => TokenKind::I8,
            "i16" => TokenKind::I16,
            "i32" => TokenKind::I32,
            "i64" => TokenKind::I64,
            "u8" => TokenKind::U8,
            "u16" => TokenKind::U16,
            "u32" => TokenKind::U32,
            "u64" => TokenKind::U64,
            "f32" => TokenKind::F32,
            "f64" => TokenKind::F64,
            "string" => TokenKind::String,
            "char" => TokenKind::Char,
            "panic" => TokenKind::Panic,
            "assert" => TokenKind::Assert,
            "unreachable" => TokenKind::Unreachable,
            "type_of" => TokenKind::TypeOf,
            _ => TokenKind::Identifier,
        };

        self.make_token(kind)
    }

    fn parse_string(&mut self) -> Token {
        loop {
            if self.at_eof() || self.peek() == '"' {
                break;
            }

            if self.peek() == '\\' {
                self.eat(); // consume the backslash
                self.parse_escape_seq();
                continue;
            }

            self.eat();
        }

        if self.at_eof() {
            self.error("unterminated string literal");
            return self.make_token(TokenKind::StrLiteral);
        }

        // consume the closing quote
        self.eat();
        self.make_token(TokenKind::StrLiteral)
    }

    fn parse_char(&mut self) -> Token {
        // empty character literal: `''` (opening quote already consumed by scan())
        if self.peek() == '\'' {
            self.eat(); // closing quote
            self.error("empty character literal");
            return self.make_token(TokenKind::CharLiteral);
        }

        if self.peek() != '\\' {
            self.eat();
        } else {
            self.eat(); // consume backslash
            self.parse_escape_seq();
        }

        if self.peek() == '\'' {
            self.eat(); // closing quote
            self.make_token(TokenKind::CharLiteral)
        } else if self.at_eof() || self.peek() == '\n' || self.peek() == ';' {
            self.error("unterminated character literal");
            self.make_token(TokenKind::CharLiteral)
        } else {
            // too many characters; consume until closing quote for recovery
            while !self.at_eof() && self.peek() != '\'' {
                self.eat();
            }
            if self.peek() == '\'' {
                self.eat();
            }
            self.error("character literal contains too many characters");
            self.make_token(TokenKind::CharLiteral)
        }
    }

    fn parse_escape_seq(&mut self) -> char {
        if self.at_eof() {
            return '\0';
        }

        escape_char(self.eat())
    }
}

/// Maps the character following a `\` to the value it escapes. Unrecognized characters are
/// returned as-is, for error recovery.
fn escape_char(c: char) -> char {
    match c {
        '\'' => '\'',
        '"' => '"',
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '\\' => '\\',
        '0' => '\0',
        other => other,
    }
}

/// Un-escapes the contents of a string/char literal (quotes already stripped), using the same
/// escape-sequence mapping the lexer itself validates against.
pub(crate) fn unescape(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            out.push(escape_char(chars[i + 1]));
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::{DiagCtx, Diagnostic};

    fn lex(src: &str) -> (Vec<Token>, Vec<Diagnostic>) {
        DiagCtx::clear();
        let chars: Vec<char> = src.chars().collect();
        let tokens = Lexer::new(&chars, 0).tokenize();
        (tokens, DiagCtx::diagnostics())
    }

    #[test]
    fn tokenizes_valid_source_without_diagnostics() {
        let (tokens, diagnostics) = lex("let x = 1 + 2;\n");
        assert!(diagnostics.is_empty());
        assert_eq!(
            tokens.iter().map(|t| &t.kind).collect::<Vec<_>>(),
            vec![
                &TokenKind::LetKw,
                &TokenKind::Identifier,
                &TokenKind::Equals,
                &TokenKind::IntLiteral,
                &TokenKind::Plus,
                &TokenKind::IntLiteral,
                &TokenKind::Semicolon,
            ]
        );
    }

    #[test]
    fn reports_unterminated_string() {
        let (_, diagnostics) = lex("\"never closed");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("unterminated string"));
    }

    #[test]
    fn reports_unterminated_block_comment() {
        let (_, diagnostics) = lex("/* never closed");
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("unterminated block comment")
        );
    }

    #[test]
    fn reports_empty_char_literal() {
        let (tokens, diagnostics) = lex("''");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("empty character literal"));
        assert_eq!(tokens[0].kind, TokenKind::CharLiteral);
    }

    #[test]
    fn reports_unexpected_character_and_recovers() {
        let (tokens, diagnostics) = lex("@ 1");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("unexpected character '@'"));
        // Lexing continues past the bad character and still finds the real token.
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::IntLiteral);
    }

    fn kinds(src: &str) -> Vec<TokenKind> {
        let (tokens, diagnostics) = lex(src);
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics for {src:?}: {diagnostics:?}"
        );
        tokens.into_iter().map(|t| t.kind).collect()
    }

    // --- keywords -----------------------------------------------------

    #[test]
    fn tokenizes_all_keywords() {
        let pairs = [
            ("as", TokenKind::AsKw),
            ("bool", TokenKind::BoolKw),
            ("break", TokenKind::BreakKw),
            ("continue", TokenKind::ContinueKw),
            ("defer", TokenKind::DeferKw),
            ("else", TokenKind::ElseKw),
            ("enum", TokenKind::EnumKw),
            ("false", TokenKind::FalseKw),
            ("for", TokenKind::ForKw),
            ("fun", TokenKind::FunKw),
            ("if", TokenKind::IfKw),
            ("import", TokenKind::ImportKw),
            ("in", TokenKind::InKw),
            ("let", TokenKind::LetKw),
            ("match", TokenKind::MatchKw),
            ("module", TokenKind::ModuleKw),
            ("mut", TokenKind::MutKw),
            ("public", TokenKind::PublicKw),
            ("return", TokenKind::ReturnKw),
            ("Self", TokenKind::UpperSelfKw),
            ("struct", TokenKind::StructKw),
            ("true", TokenKind::TrueKw),
            ("use", TokenKind::UseKw),
            ("while", TokenKind::WhileKw),
            ("panic", TokenKind::Panic),
            ("assert", TokenKind::Assert),
            ("unreachable", TokenKind::Unreachable),
            ("type_of", TokenKind::TypeOf),
            ("i8", TokenKind::I8),
            ("i16", TokenKind::I16),
            ("i32", TokenKind::I32),
            ("i64", TokenKind::I64),
            ("u8", TokenKind::U8),
            ("u16", TokenKind::U16),
            ("u32", TokenKind::U32),
            ("u64", TokenKind::U64),
            ("f32", TokenKind::F32),
            ("f64", TokenKind::F64),
            ("string", TokenKind::String),
            ("char", TokenKind::Char),
        ];
        for (src, expected) in pairs {
            assert_eq!(kinds(src), vec![expected], "lexing {src:?}");
        }
    }

    #[test]
    fn keywords_use_maximal_munch_not_prefix_match() {
        // A keyword that's merely a prefix of the identifier must not match early.
        assert_eq!(kinds("structure"), vec![TokenKind::Identifier]);
        assert_eq!(kinds("iffy"), vec![TokenKind::Identifier]);
        assert_eq!(kinds("forever"), vec![TokenKind::Identifier]);
        assert_eq!(kinds("i32x"), vec![TokenKind::Identifier]);
        assert_eq!(kinds("in_range"), vec![TokenKind::Identifier]);
    }

    #[test]
    fn wildcard_vs_identifier() {
        assert_eq!(kinds("_"), vec![TokenKind::Wildcard]);
        assert_eq!(kinds("_foo"), vec![TokenKind::Identifier]);
        assert_eq!(kinds("foo_bar_1"), vec![TokenKind::Identifier]);
    }

    // --- numbers --------------------------------------------------------

    #[test]
    fn tokenizes_integers_and_floats() {
        assert_eq!(kinds("42"), vec![TokenKind::IntLiteral]);
        assert_eq!(kinds("3.14"), vec![TokenKind::FloatLiteral]);
        assert_eq!(kinds("0.0"), vec![TokenKind::FloatLiteral]);
    }

    #[test]
    fn trailing_dot_without_digit_is_not_a_float() {
        // "1." should not greedily consume the '.' as part of a float since there's no
        // fractional digit; it's an int literal followed by a separate period token.
        assert_eq!(kinds("1."), vec![TokenKind::IntLiteral, TokenKind::Period]);
    }

    #[test]
    fn range_after_int_literal_is_not_parsed_as_float() {
        assert_eq!(
            kinds("5..10"),
            vec![
                TokenKind::IntLiteral,
                TokenKind::ExclRange,
                TokenKind::IntLiteral
            ]
        );
        assert_eq!(
            kinds("5..=10"),
            vec![
                TokenKind::IntLiteral,
                TokenKind::InclRange,
                TokenKind::IntLiteral
            ]
        );
    }

    // --- operators --------------------------------------------------------

    #[test]
    fn tokenizes_multi_char_operators() {
        let pairs = [
            ("->", TokenKind::Arrow),
            ("=>", TokenKind::FatArrow),
            ("::", TokenKind::DoubleColon),
            ("++", TokenKind::DoublePlus),
            ("--", TokenKind::DoubleMinus),
            ("==", TokenKind::DoubleEquals),
            ("!=", TokenKind::BangEquals),
            ("&&", TokenKind::DoubleAmp),
            ("||", TokenKind::DoublePipe),
            ("<=", TokenKind::LessEqual),
            (">=", TokenKind::GreaterEqual),
            ("+=", TokenKind::PlusEquals),
            ("-=", TokenKind::SubEquals),
            ("*=", TokenKind::MulEquals),
            ("/=", TokenKind::DivEquals),
            ("%=", TokenKind::ModEquals),
            ("..", TokenKind::ExclRange),
            ("..=", TokenKind::InclRange),
        ];
        for (src, expected) in pairs {
            assert_eq!(kinds(src), vec![expected], "lexing {src:?}");
        }
    }

    #[test]
    fn tokenizes_single_char_operators_not_greedily_extended() {
        let pairs = [
            ("&", TokenKind::Amp),
            ("|", TokenKind::Pipe),
            ("<", TokenKind::OpenCaret),
            (">", TokenKind::CloseCaret),
            ("=", TokenKind::Equals),
            (":", TokenKind::Colon),
            (".", TokenKind::Period),
            ("+", TokenKind::Plus),
            ("-", TokenKind::Minus),
            ("!", TokenKind::Bang),
            ("?", TokenKind::Try),
        ];
        for (src, expected) in pairs {
            assert_eq!(kinds(src), vec![expected], "lexing {src:?}");
        }
    }

    #[test]
    fn minus_arrow_is_not_confused_with_decrement() {
        assert_eq!(kinds("->"), vec![TokenKind::Arrow]);
        assert_eq!(kinds("- >"), vec![TokenKind::Minus, TokenKind::CloseCaret]);
    }

    #[test]
    fn negative_numbers_lex_as_separate_minus_and_literal() {
        // Phi has no negative literal syntax; unary minus is a distinct token.
        assert_eq!(kinds("-1"), vec![TokenKind::Minus, TokenKind::IntLiteral]);
    }

    // --- strings and chars --------------------------------------------------------

    #[test]
    fn tokenizes_string_with_escapes() {
        let (tokens, diagnostics) = lex(r#""hello\n\t\"world\"""#);
        assert!(diagnostics.is_empty());
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::StrLiteral);
    }

    #[test]
    fn string_literal_can_span_multiple_lines() {
        let (tokens, diagnostics) = lex("\"line1\nline2\"");
        assert!(diagnostics.is_empty());
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::StrLiteral);
    }

    #[test]
    fn tokenizes_char_literals() {
        assert_eq!(kinds("'a'"), vec![TokenKind::CharLiteral]);
        assert_eq!(kinds(r"'\n'"), vec![TokenKind::CharLiteral]);
        assert_eq!(kinds(r"'\''"), vec![TokenKind::CharLiteral]);
        assert_eq!(kinds(r"'\\'"), vec![TokenKind::CharLiteral]);
    }

    #[test]
    fn reports_char_literal_with_too_many_characters() {
        let (tokens, diagnostics) = lex("'ab'");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("too many characters"));
        assert_eq!(tokens[0].kind, TokenKind::CharLiteral);
    }

    #[test]
    fn reports_unterminated_char_literal_at_newline() {
        let (_, diagnostics) = lex("'a\nlet");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("unterminated character"));
    }

    #[test]
    fn reports_unterminated_char_literal_at_semicolon() {
        let (_, diagnostics) = lex("'a;");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("unterminated character"));
    }

    #[test]
    fn reports_unterminated_char_literal_at_eof() {
        let (_, diagnostics) = lex("'a");
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("unterminated character"));
    }

    // --- comments and whitespace --------------------------------------------------------

    #[test]
    fn line_comment_stops_at_newline() {
        let (tokens, diagnostics) = lex("1 // comment\n2");
        assert!(diagnostics.is_empty());
        assert_eq!(
            tokens.iter().map(|t| &t.kind).collect::<Vec<_>>(),
            vec![&TokenKind::IntLiteral, &TokenKind::IntLiteral]
        );
    }

    #[test]
    fn block_comment_is_skipped() {
        let (tokens, diagnostics) = lex("1 /* comment \n spanning lines */ 2");
        assert!(diagnostics.is_empty());
        assert_eq!(
            tokens.iter().map(|t| &t.kind).collect::<Vec<_>>(),
            vec![&TokenKind::IntLiteral, &TokenKind::IntLiteral]
        );
    }

    #[test]
    fn block_comments_do_not_nest() {
        // The first `*/` closes the comment, so the trailing `*/` is re-lexed as real tokens.
        let (tokens, diagnostics) = lex("/* outer /* inner */ 1 */");
        assert!(diagnostics.is_empty());
        assert_eq!(
            tokens.iter().map(|t| &t.kind).collect::<Vec<_>>(),
            vec![&TokenKind::IntLiteral, &TokenKind::Star, &TokenKind::Slash,]
        );
    }

    #[test]
    fn mixed_whitespace_is_skipped() {
        assert_eq!(
            kinds("1\t\n \r\n2"),
            vec![TokenKind::IntLiteral, TokenKind::IntLiteral]
        );
    }

    // --- error recovery and multiple diagnostics --------------------------------------------------------

    #[test]
    fn accumulates_multiple_diagnostics_across_a_file() {
        let (_, diagnostics) = lex("@ 1 # 2");
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].message.contains("'@'"));
        assert!(diagnostics[1].message.contains("'#'"));
    }

    // --- spans --------------------------------------------------------

    #[test]
    fn token_spans_are_correct_char_offsets() {
        let (tokens, _) = lex("let x = 1;");
        // "let" @ [0,3), "x" @ [4,5), "=" @ [6,7), "1" @ [8,9), ";" @ [9,10)
        assert_eq!(tokens[0].span.as_tuple(), (0, 3));
        assert_eq!(tokens[1].span.as_tuple(), (4, 5));
        assert_eq!(tokens[2].span.as_tuple(), (6, 7));
        assert_eq!(tokens[3].span.as_tuple(), (8, 9));
        assert_eq!(tokens[4].span.as_tuple(), (9, 10));
    }

    #[test]
    fn file_offset_shifts_all_spans() {
        DiagCtx::clear();
        let chars: Vec<char> = "foo".chars().collect();
        let tokens = Lexer::new(&chars, 100).tokenize();
        assert_eq!(tokens[0].span.as_tuple(), (100, 103));
    }

    // --- realistic snippet from the README --------------------------------------------------------

    #[test]
    fn tokenizes_function_declaration() {
        assert_eq!(
            kinds("fun add(x: i32, y: i32) -> i32 {\n    return x + y;\n}"),
            vec![
                TokenKind::FunKw,
                TokenKind::Identifier,
                TokenKind::OpenParen,
                TokenKind::Identifier,
                TokenKind::Colon,
                TokenKind::I32,
                TokenKind::Comma,
                TokenKind::Identifier,
                TokenKind::Colon,
                TokenKind::I32,
                TokenKind::CloseParen,
                TokenKind::Arrow,
                TokenKind::I32,
                TokenKind::OpenBrace,
                TokenKind::ReturnKw,
                TokenKind::Identifier,
                TokenKind::Plus,
                TokenKind::Identifier,
                TokenKind::Semicolon,
                TokenKind::CloseBrace,
            ]
        );
    }

    #[test]
    fn tokenizes_projecting_function_signature() {
        // `:` for projecting return type and `any` as a plain identifier-like keyword-free name.
        assert_eq!(
            kinds("fun min(x: any i32, y: any i32) -> any i32 {"),
            vec![
                TokenKind::FunKw,
                TokenKind::Identifier,
                TokenKind::OpenParen,
                TokenKind::Identifier,
                TokenKind::Colon,
                TokenKind::AnyKw,
                TokenKind::I32,
                TokenKind::Comma,
                TokenKind::Identifier,
                TokenKind::Colon,
                TokenKind::AnyKw,
                TokenKind::I32,
                TokenKind::CloseParen,
                TokenKind::Arrow,
                TokenKind::AnyKw,
                TokenKind::I32,
                TokenKind::OpenBrace,
            ]
        );
    }

    #[test]
    fn tokenizes_enum_with_match() {
        let src =
            "enum Shape {\n    Circle: f64;\n}\nmatch shape {\n    .Circle => 1,\n    _ => 0\n};";
        assert_eq!(
            kinds(src),
            vec![
                TokenKind::EnumKw,
                TokenKind::Identifier,
                TokenKind::OpenBrace,
                TokenKind::Identifier,
                TokenKind::Colon,
                TokenKind::F64,
                TokenKind::Semicolon,
                TokenKind::CloseBrace,
                TokenKind::MatchKw,
                TokenKind::Identifier,
                TokenKind::OpenBrace,
                TokenKind::Period,
                TokenKind::Identifier,
                TokenKind::FatArrow,
                TokenKind::IntLiteral,
                TokenKind::Comma,
                TokenKind::Wildcard,
                TokenKind::FatArrow,
                TokenKind::IntLiteral,
                TokenKind::CloseBrace,
                TokenKind::Semicolon,
            ]
        );
    }

    /// Not a real assertion — run with `cargo test render_sample_report -- --nocapture` to see
    /// what `ariadne`'s rendered output actually looks like for a couple of these diagnostics.
    #[test]
    fn render_sample_report() {
        DiagCtx::clear();
        let src = "let x = \"never closed\nlet y = '';\nlet z = @;\n";
        let chars: Vec<char> = src.chars().collect();
        let offset = crate::driver::src_map::SrcMap::add_file("<test>".to_string(), chars.clone());

        Lexer::new(&chars, offset).tokenize();
        DiagCtx::report();
    }
}
