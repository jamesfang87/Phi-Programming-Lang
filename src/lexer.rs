use crate::diagnostics::DiagCtx;
use crate::driver::source::SrcSpan;
use crate::lexer::literal::is_escape;
use crate::lexer::token::{KEYWORDS, Token, TokenKind};

pub mod describe;
pub mod literal;
pub mod token;

pub struct Lexer<'a> {
    /// [`Lexer::src`] is a slice of the file's characters.
    src: &'a [char],

    /// [`Lexer::file_offset`] allows the [`Lexer`] to produce global
    /// [`SrcSpan`]s for the [`Token`] stream it outputs.
    file_offset: usize,

    /// [`Lexer::cursor`] is the current position in [`Lexer::src`].
    cursor: usize,

    /// [`Lexer::lexeme_pos`] is position that the current lexeme (lexical unit)
    /// starts at in [`Lexer::src`].
    /// This is required for generating spans for multi-character tokens as
    /// [`Lexer::cursor`] is not the start of the token.
    lexeme_pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src_text: &'a [char], file_offset: usize) -> Lexer<'a> {
        Lexer {
            src: src_text,
            file_offset,
            cursor: 0,
            lexeme_pos: 0,
        }
    }

    /// Scans the source and returns its tokens as one list.
    pub fn tokenize(&mut self) -> Vec<Token> {
        let mut token_stream = Vec::new();

        loop {
            self.skip_trivia();
            if self.at_eof() {
                break;
            }

            self.lexeme_pos = self.cursor;
            if let Some(token) = self.scan() {
                token_stream.push(token);
            }
        }

        token_stream
    }

    /// Scans and returns one token, starting at `self.cursor`.
    ///
    /// This MUST be called only after [`Lexer::skip_trivia`]
    /// Returns `None` when the current character is invalid
    fn scan(&mut self) -> Option<Token> {
        match self.eat() {
            '(' => Some(self.make_token(TokenKind::OpenParen)),
            ')' => Some(self.make_token(TokenKind::CloseParen)),
            '{' => Some(self.make_token(TokenKind::OpenBrace)),
            '}' => Some(self.make_token(TokenKind::CloseBrace)),
            '[' => Some(self.make_token(TokenKind::OpenBracket)),
            ']' => Some(self.make_token(TokenKind::CloseBracket)),
            ',' => Some(self.make_token(TokenKind::Comma)),
            ';' => Some(self.make_token(TokenKind::Semicolon)),

            '.' => {
                if self.eat_if('.').is_some() {
                    if self.eat_if('=').is_some() {
                        Some(self.make_token(TokenKind::InclRange))
                    } else {
                        Some(self.make_token(TokenKind::ExclRange))
                    }
                } else {
                    Some(self.make_token(TokenKind::Period))
                }
            }
            ':' => {
                if self.eat_if(':').is_some() {
                    Some(self.make_token(TokenKind::DoubleColon))
                } else {
                    Some(self.make_token(TokenKind::Colon))
                }
            }

            '+' => {
                if self.eat_if('=').is_some() {
                    Some(self.make_token(TokenKind::PlusEquals))
                } else {
                    Some(self.make_token(TokenKind::Plus))
                }
            }
            '-' => {
                if self.eat_if('>').is_some() {
                    Some(self.make_token(TokenKind::Arrow))
                } else if self.eat_if('=').is_some() {
                    Some(self.make_token(TokenKind::MinusEquals))
                } else {
                    Some(self.make_token(TokenKind::Minus))
                }
            }
            '*' => {
                let kind = if self.eat_if('=').is_some() {
                    TokenKind::MulEquals
                } else {
                    TokenKind::Star
                };
                Some(self.make_token(kind))
            }
            '/' => {
                let kind = if self.eat_if('=').is_some() {
                    TokenKind::DivEquals
                } else {
                    TokenKind::Slash
                };
                Some(self.make_token(kind))
            }
            '%' => {
                let kind = if self.eat_if('=').is_some() {
                    TokenKind::ModEquals
                } else {
                    TokenKind::Percent
                };
                Some(self.make_token(kind))
            }
            '!' => {
                let kind = if self.eat_if('=').is_some() {
                    TokenKind::BangEquals
                } else {
                    TokenKind::Bang
                };
                Some(self.make_token(kind))
            }
            '=' => {
                if self.eat_if('>').is_some() {
                    Some(self.make_token(TokenKind::FatArrow))
                } else if self.eat_if('=').is_some() {
                    Some(self.make_token(TokenKind::DoubleEquals))
                } else {
                    Some(self.make_token(TokenKind::Equals))
                }
            }
            '<' => {
                let kind = if self.eat_if('=').is_some() {
                    TokenKind::LessEqual
                } else {
                    TokenKind::OpenAngle
                };
                Some(self.make_token(kind))
            }
            '>' => {
                let kind = if self.eat_if('=').is_some() {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::CloseAngle
                };
                Some(self.make_token(kind))
            }
            '&' => {
                if self.eat_if('&').is_some() {
                    Some(self.make_token(TokenKind::DoubleAmp))
                } else {
                    Some(self.make_token(TokenKind::Amp))
                }
            }
            '|' => {
                if self.eat_if('|').is_some() {
                    Some(self.make_token(TokenKind::DoublePipe))
                } else {
                    Some(self.make_token(TokenKind::Pipe))
                }
            }
            '?' => Some(self.make_token(TokenKind::Try)),
            '_' => {
                // A lone `_` is the wildcard token, but `_foo` is an identifier. Thus we check
                // the next character before deciding which one this is.
                if self.peek().is_ascii_alphanumeric() || self.peek() == '_' {
                    Some(self.lex_identifier_or_kw())
                } else {
                    Some(self.make_token(TokenKind::Wildcard))
                }
            }

            '"' => Some(self.lex_string()),
            '\'' => Some(self.lex_char()),

            c => {
                if c.is_ascii_alphabetic() || c == '_' {
                    Some(self.lex_identifier_or_kw())
                } else if c.is_ascii_digit() {
                    Some(self.lex_number())
                } else {
                    self.error(format!("unexpected character '{}'", c));
                    // The bad character is already consumed; `tokenize` skips the trivia after
                    // it and scans on, so a single stray byte does not stop the whole file
                    // from being lexed.
                    None
                }
            }
        }
    }

    /// Returns the current character without consuming it. Returns `'\0'` at end of input.
    fn peek(&self) -> char {
        self.src.get(self.cursor).copied().unwrap_or('\0')
    }

    /// Returns the character after the current one, without consuming anything. Returns `'\0'`
    /// at or past end of input.
    fn peek_next(&self) -> char {
        self.src.get(self.cursor + 1).copied().unwrap_or('\0')
    }

    /// Consumes and returns the current character.
    fn eat(&mut self) -> char {
        let temp = self.peek();
        self.cursor += 1;
        temp
    }

    /// Consumes the current character if it equals `expected`, and returns it. Returns `None`
    /// without consuming anything otherwise.
    fn eat_if(&mut self, expected: char) -> Option<char> {
        if self.at_eof() || self.peek() != expected {
            None
        } else {
            self.cursor += 1;
            Some(expected)
        }
    }

    fn at_eof(&self) -> bool {
        self.cursor >= self.src.len()
    }

    /// Builds a token of `kind` whose span covers everything consumed since `lexeme_pos` was
    /// last set. Callers must set `lexeme_pos` to the cursor at the start of each lexeme.
    fn make_token(&self, kind: TokenKind) -> Token {
        Token {
            kind,
            span: SrcSpan::new(
                self.file_offset + self.lexeme_pos,
                self.file_offset + self.cursor,
            ),
        }
    }

    fn lex_number(&mut self) -> Token {
        self.eat_digit_run();

        // A `.` only starts a fractional part if a digit follows it. Thus `1.` lexes as an int
        // literal followed by a separate `.` token, rather than an incomplete float.
        let is_float = if self.peek() == '.' && self.peek_next().is_ascii_digit() {
            self.eat();
            self.eat_digit_run();
            true
        } else {
            false
        };

        if self.peek() == '_' && self.peek_next().is_ascii_alphabetic() {
            self.eat();
            while self.peek().is_ascii_alphanumeric() {
                self.eat();
            }
        }

        self.make_token(if is_float {
            TokenKind::FloatLiteral
        } else {
            TokenKind::IntLiteral
        })
    }

    /// Consumes a sequence of ASCII digits
    fn eat_digit_run(&mut self) {
        while self.peek().is_ascii_digit()
            || (self.peek() == '_' && self.peek_next().is_ascii_digit())
        {
            self.eat();
        }
    }

    fn lex_identifier_or_kw(&mut self) -> Token {
        while self.peek().is_ascii_alphanumeric() || self.peek() == '_' {
            self.eat();
        }

        let ident: String = self.src[self.lexeme_pos..self.cursor].iter().collect();

        let kind = KEYWORDS
            .iter()
            .find(|(spelling, _)| *spelling == ident.as_str())
            .map_or(TokenKind::Identifier, |&(_, kind)| kind);

        self.make_token(kind)
    }

    // The opening quote is already consumed by `scan`.
    fn lex_string(&mut self) -> Token {
        loop {
            if self.at_eof() || self.peek() == '"' {
                break;
            }

            if self.peek() == '\\' {
                self.eat();
                self.lex_escape_seq();
                continue;
            }

            self.eat();
        }

        if self.at_eof() {
            self.error("unterminated string literal");
            return self.make_token(TokenKind::StrLiteral);
        }

        self.eat(); // the closing quote
        self.make_token(TokenKind::StrLiteral)
    }

    // The opening quote is already consumed by `scan`.
    fn lex_char(&mut self) -> Token {
        // `''` has no content, so we check for the closing quote before reading a character.
        if self.peek() == '\'' {
            self.eat();
            self.error("empty character literal");
            return self.make_token(TokenKind::CharLiteral);
        }

        let mut unknown_escape = false;
        if self.peek() != '\\' {
            self.eat();
        } else {
            self.eat();
            unknown_escape = !self.lex_escape_seq();
        }

        if self.peek() == '\'' {
            self.eat();
            self.make_token(TokenKind::CharLiteral)
        } else if self.at_eof() || self.peek() == '\n' || self.peek() == ';' {
            self.error("unterminated character literal");
            self.make_token(TokenKind::CharLiteral)
        } else {
            while !self.at_eof() && self.peek() != '\'' {
                self.eat();
            }
            if self.peek() == '\'' {
                self.eat();
            }
            if !unknown_escape {
                self.error("character literal contains too many characters");
            }
            self.make_token(TokenKind::CharLiteral)
        }
    }

    fn lex_escape_seq(&mut self) -> bool {
        if self.at_eof() {
            return true;
        }

        let escaped = self.eat();
        let known = is_escape(escaped);
        if !known {
            self.error(format!("unknown escape sequence `\\{escaped}`"));
        }
        known
    }

    /// Record a diagnostic pointing at the span of the lexeme currently being scanned.
    fn error(&self, message: impl Into<String>) {
        let span = SrcSpan::new(
            self.file_offset + self.lexeme_pos,
            self.file_offset + self.cursor,
        );
        DiagCtx::error(message, span);
    }

    /// Consumes whitespace and comments.
    fn skip_trivia(&mut self) {
        loop {
            if self.peek().is_ascii_whitespace() {
                self.eat();
                continue;
            }

            // Line comment: `//...`
            if self.peek() == '/' && self.peek_next() == '/' {
                self.eat();
                self.eat();
                while !self.at_eof() && self.peek() != '\n' {
                    self.eat();
                }
                continue;
            }

            // Block comment: `/* ... */`, which may nest.
            if self.peek() == '/' && self.peek_next() == '*' {
                self.lexeme_pos = self.cursor;
                self.eat();
                self.eat();
                let mut depth = 1;
                loop {
                    if self.at_eof() {
                        self.error("unterminated block comment");
                        break;
                    }

                    if self.peek() == '/' && self.peek_next() == '*' {
                        self.eat();
                        self.eat();
                        depth += 1;
                        continue;
                    }

                    if self.peek() == '*' && self.peek_next() == '/' {
                        self.eat();
                        self.eat();
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        continue;
                    }
                    self.eat();
                }
                continue;
            }

            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagCtx, Diagnostic};
    use crate::lexer::describe::Descriptor;

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

    /// Reports the single diagnostic `src` raises, or panics otherwise.
    fn only_diagnostic(src: &str) -> Diagnostic {
        let (_, raised) = lex(src);
        assert_eq!(raised.len(), 1, "expected exactly one diagnostic");
        raised.into_iter().next().unwrap()
    }

    #[test]
    fn reports_unknown_string_escape() {
        // `"\q"` must not silently decode to `"aq"`; the escape has to be called out.
        let diagnostic = only_diagnostic(r#""a\qb""#);
        assert!(diagnostic.message.contains("unknown escape sequence `\\q`"));
    }

    #[test]
    fn reports_unknown_char_escape() {
        // `'\u{41}'` previously collapsed to `Char('u')` behind a misleading "too many
        // characters" report; the unknown escape itself must be what gets named.
        let diagnostic = only_diagnostic(r"'\u{41}'");
        assert!(diagnostic.message.contains("unknown escape sequence `\\u`"));
    }

    #[test]
    fn accepts_every_escape_the_ast_layer_decodes() {
        // The spellings `lex_escape_seq` accepts must match what `escape` in
        // `ast::expr_impls` decodes, so neither side can gain an escape the other drops.
        for escaped in ['"', '\'', 'n', 't', 'r', '\\', '0'] {
            let src = format!(r#""\{escaped}""#);
            let (tokens, diagnostics) = lex(&src);
            assert!(diagnostics.is_empty(), "for {escaped:?}: {diagnostics:?}");
            assert_eq!(tokens.len(), 1);
            assert_eq!(tokens[0].kind, TokenKind::StrLiteral);
        }
    }

    #[test]
    fn a_file_ending_in_a_bad_character_gains_no_phantom_token() {
        // The stray byte is reported, but nothing is appended past the end of input.
        let (tokens, diagnostics) = lex("fun main() {} @");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            tokens.iter().map(|t| &t.kind).collect::<Vec<_>>(),
            vec![
                &TokenKind::FunKw,
                &TokenKind::Identifier,
                &TokenKind::OpenParen,
                &TokenKind::CloseParen,
                &TokenKind::OpenBrace,
                &TokenKind::CloseBrace,
            ]
        );
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
    fn every_keyword_round_trips_through_its_own_token_kind() {
        // Lexing a keyword's spelling must produce exactly the kind whose `to_string` is that
        // spelling, so the [`KEYWORDS`] table the lexer looks spellings up in and the spellings
        // diagnostics name cannot drift apart.
        for (spelling, kind) in KEYWORDS {
            assert_eq!(kinds(spelling), vec![*kind], "lexing {spelling:?}");
            assert_eq!(Descriptor::of(*kind).name(), *spelling, "for {kind:?}");
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
    fn digit_separators_only_apply_between_digits() {
        assert_eq!(kinds("1_000_000"), vec![TokenKind::IntLiteral]);
        assert_eq!(kinds("3.14_15"), vec![TokenKind::FloatLiteral]);
        // A trailing underscore not followed by a digit doesn't get folded into the number: it
        // starts a separate token, here the wildcard `_`.
        assert_eq!(
            kinds("1_ _"),
            vec![
                TokenKind::IntLiteral,
                TokenKind::Wildcard,
                TokenKind::Wildcard
            ]
        );
    }

    #[test]
    fn tokenizes_suffixed_int_literals_as_one_token() {
        for src in [
            "42_i8",
            "42_i16",
            "42_i32",
            "42_i64",
            "42_u8",
            "42_u16",
            "42_u32",
            "42_u64",
            "1_000_000_i64",
        ] {
            let (tokens, diagnostics) = lex(src);
            assert!(diagnostics.is_empty(), "unexpected diagnostics for {src:?}");
            assert_eq!(
                tokens.iter().map(|t| t.kind).collect::<Vec<_>>(),
                vec![TokenKind::IntLiteral],
                "lexing {src:?}"
            );
            assert_eq!(
                tokens[0].span.as_tuple(),
                (0, src.chars().count()),
                "the suffix should be part of the literal's span for {src:?}"
            );
        }
    }

    #[test]
    fn tokenizes_suffixed_float_literals_as_one_token() {
        for src in ["3.14_f32", "3.14_f64"] {
            assert_eq!(kinds(src), vec![TokenKind::FloatLiteral], "lexing {src:?}");
        }
    }

    #[test]
    fn int_syntax_literal_with_a_float_suffix_is_still_an_int_literal_token() {
        // The lexer only distinguishes `IntLiteral`/`FloatLiteral` by whether a `.` was written;
        // a `_f64` suffix on a whole number (`5_f64`) doesn't retroactively add one. Type
        // checking, not the lexer, is what decides `5_f64` is a float.
        assert_eq!(kinds("5_f64"), vec![TokenKind::IntLiteral]);
    }

    #[test]
    fn lexer_does_not_validate_suffix_names() {
        // Any `_name` right after a number's digits is lexed as its suffix; whether `name` is a
        // real numeric type is left for type checking to say.
        assert_eq!(kinds("42_bogus"), vec![TokenKind::IntLiteral]);
    }

    #[test]
    fn suffix_must_be_directly_adjacent_to_the_number() {
        // A space between the number and what would be a suffix means there's no suffix -- just
        // two separate tokens.
        assert_eq!(
            kinds("42 _i64"),
            vec![TokenKind::IntLiteral, TokenKind::Identifier]
        );
    }

    #[test]
    fn range_after_int_literal_is_not_lexed_as_float() {
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
            ("==", TokenKind::DoubleEquals),
            ("!=", TokenKind::BangEquals),
            ("&&", TokenKind::DoubleAmp),
            ("||", TokenKind::DoublePipe),
            ("<=", TokenKind::LessEqual),
            (">=", TokenKind::GreaterEqual),
            ("+=", TokenKind::PlusEquals),
            ("-=", TokenKind::MinusEquals),
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

    /// Phi has no increment/decrement operators, so `++` and `--` are not single tokens: they
    /// lex as their separate characters and fail later, at the parser, with its own diagnostic.
    #[test]
    fn double_plus_and_double_minus_are_not_single_tokens() {
        assert_eq!(
            kinds("a++b"),
            vec![
                TokenKind::Identifier,
                TokenKind::Plus,
                TokenKind::Plus,
                TokenKind::Identifier
            ]
        );
        assert_eq!(
            kinds("a--b"),
            vec![
                TokenKind::Identifier,
                TokenKind::Minus,
                TokenKind::Minus,
                TokenKind::Identifier
            ]
        );
    }

    #[test]
    fn tokenizes_single_char_operators_not_greedily_extended() {
        let pairs = [
            ("&", TokenKind::Amp),
            ("|", TokenKind::Pipe),
            ("<", TokenKind::OpenAngle),
            (">", TokenKind::CloseAngle),
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
        assert_eq!(kinds("- >"), vec![TokenKind::Minus, TokenKind::CloseAngle]);
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
    fn block_comments_nest() {
        // The inner `/* ... */` opens a nested comment, so only the outer `*/` closes it.
        let (tokens, diagnostics) = lex("/* outer /* inner */ 1 */");
        assert!(diagnostics.is_empty());
        assert!(tokens.is_empty());
    }

    #[test]
    fn nested_block_comment_missing_inner_close_is_unterminated() {
        // The outer `*/` closes only the inner comment; the outer one is left open.
        let (_, diagnostics) = lex("/* outer /* inner */");
        assert_eq!(diagnostics.len(), 1);
        assert!(
            diagnostics[0]
                .message
                .contains("unterminated block comment")
        );
    }

    #[test]
    fn deeply_nested_block_comments_are_skipped() {
        let (tokens, diagnostics) = lex("/* a /* b /* c */ d */ e */ 1");
        assert!(diagnostics.is_empty());
        assert_eq!(
            tokens.iter().map(|t| &t.kind).collect::<Vec<_>>(),
            vec![&TokenKind::IntLiteral]
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
        let offset = crate::driver::source::SrcMap::add_file(
            "<test>".to_string(),
            chars.clone(),
            crate::driver::source::FileOrigin::User,
        );

        Lexer::new(&chars, offset).tokenize();
        DiagCtx::report();
    }
}
