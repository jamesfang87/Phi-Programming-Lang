use crate::lexer::token::TokenKind;

#[derive(Clone, Copy)]
pub(crate) enum Descriptor {
    Spelling(&'static str),
    Named(&'static str),
}

impl Descriptor {
    pub(crate) fn of(kind: TokenKind) -> Self {
        match kind {
            TokenKind::Eof => Descriptor::Named("end of file"),
            TokenKind::AnyKw => Descriptor::Spelling("any"),
            TokenKind::AsKw => Descriptor::Spelling("as"),
            TokenKind::BoolKw => Descriptor::Spelling("bool"),
            TokenKind::BreakKw => Descriptor::Spelling("break"),
            TokenKind::ConcurrentKw => Descriptor::Spelling("concurrent"),
            TokenKind::ContinueKw => Descriptor::Spelling("continue"),
            TokenKind::DeferKw => Descriptor::Spelling("defer"),
            TokenKind::DynKw => Descriptor::Spelling("dyn"),
            TokenKind::ElseKw => Descriptor::Spelling("else"),
            TokenKind::EnumKw => Descriptor::Spelling("enum"),
            TokenKind::ExtendKw => Descriptor::Spelling("extend"),
            TokenKind::FalseKw => Descriptor::Spelling("false"),
            TokenKind::ForKw => Descriptor::Spelling("for"),
            TokenKind::FunKw => Descriptor::Spelling("fun"),
            TokenKind::IfKw => Descriptor::Spelling("if"),
            TokenKind::ImportKw => Descriptor::Spelling("import"),
            TokenKind::InKw => Descriptor::Spelling("in"),
            TokenKind::IsoKw => Descriptor::Spelling("iso"),
            TokenKind::LetKw => Descriptor::Spelling("let"),
            TokenKind::MatchKw => Descriptor::Spelling("match"),
            TokenKind::ModuleKw => Descriptor::Spelling("module"),
            TokenKind::MutKw => Descriptor::Spelling("mut"),
            TokenKind::NewKw => Descriptor::Spelling("new"),
            TokenKind::PublicKw => Descriptor::Spelling("public"),
            TokenKind::ReturnKw => Descriptor::Spelling("return"),
            TokenKind::LowerSelfKw => Descriptor::Spelling("self"),
            TokenKind::UpperSelfKw => Descriptor::Spelling("Self"),
            TokenKind::SpawnKw => Descriptor::Spelling("spawn"),
            TokenKind::StructKw => Descriptor::Spelling("struct"),
            TokenKind::TraitKw => Descriptor::Spelling("trait"),
            TokenKind::TrueKw => Descriptor::Spelling("true"),
            TokenKind::WhileKw => Descriptor::Spelling("while"),
            TokenKind::WithKw => Descriptor::Spelling("with"),
            TokenKind::Panic => Descriptor::Spelling("panic"),
            TokenKind::Assert => Descriptor::Spelling("assert"),
            TokenKind::Unreachable => Descriptor::Spelling("unreachable"),
            TokenKind::I8 => Descriptor::Spelling("i8"),
            TokenKind::I16 => Descriptor::Spelling("i16"),
            TokenKind::I32 => Descriptor::Spelling("i32"),
            TokenKind::I64 => Descriptor::Spelling("i64"),
            TokenKind::U8 => Descriptor::Spelling("u8"),
            TokenKind::U16 => Descriptor::Spelling("u16"),
            TokenKind::U32 => Descriptor::Spelling("u32"),
            TokenKind::U64 => Descriptor::Spelling("u64"),
            TokenKind::Usize => Descriptor::Spelling("usize"),
            TokenKind::F32 => Descriptor::Spelling("f32"),
            TokenKind::F64 => Descriptor::Spelling("f64"),
            TokenKind::Str => Descriptor::Spelling("str"),
            TokenKind::Char => Descriptor::Spelling("char"),
            TokenKind::OpenParen => Descriptor::Spelling("("),
            TokenKind::CloseParen => Descriptor::Spelling(")"),
            TokenKind::OpenBrace => Descriptor::Spelling("{"),
            TokenKind::CloseBrace => Descriptor::Spelling("}"),
            TokenKind::OpenBracket => Descriptor::Spelling("["),
            TokenKind::CloseBracket => Descriptor::Spelling("]"),
            TokenKind::Arrow => Descriptor::Spelling("->"),
            TokenKind::FatArrow => Descriptor::Spelling("=>"),
            TokenKind::Comma => Descriptor::Spelling(","),
            TokenKind::Semicolon => Descriptor::Spelling(";"),
            TokenKind::Plus => Descriptor::Spelling("+"),
            TokenKind::Minus => Descriptor::Spelling("-"),
            TokenKind::Star => Descriptor::Spelling("*"),
            TokenKind::Slash => Descriptor::Spelling("/"),
            TokenKind::Percent => Descriptor::Spelling("%"),
            TokenKind::Bang => Descriptor::Spelling("!"),
            TokenKind::Amp => Descriptor::Spelling("&"),
            TokenKind::Try => Descriptor::Spelling("?"),
            TokenKind::PlusEquals => Descriptor::Spelling("+="),
            TokenKind::MinusEquals => Descriptor::Spelling("-="),
            TokenKind::MulEquals => Descriptor::Spelling("*="),
            TokenKind::DivEquals => Descriptor::Spelling("/="),
            TokenKind::ModEquals => Descriptor::Spelling("%="),
            TokenKind::Period => Descriptor::Spelling("."),
            TokenKind::DoubleColon => Descriptor::Spelling("::"),
            TokenKind::DoubleEquals => Descriptor::Spelling("=="),
            TokenKind::BangEquals => Descriptor::Spelling("!="),
            TokenKind::DoubleAmp => Descriptor::Spelling("&&"),
            TokenKind::DoublePipe => Descriptor::Spelling("||"),
            TokenKind::Pipe => Descriptor::Spelling("|"),
            TokenKind::OpenAngle => Descriptor::Spelling("<"),
            TokenKind::LessEqual => Descriptor::Spelling("<="),
            TokenKind::CloseAngle => Descriptor::Spelling(">"),
            TokenKind::GreaterEqual => Descriptor::Spelling(">="),
            TokenKind::Equals => Descriptor::Spelling("="),
            TokenKind::Colon => Descriptor::Spelling(":"),
            TokenKind::ExclRange => Descriptor::Spelling(".."),
            TokenKind::InclRange => Descriptor::Spelling("..="),
            TokenKind::Wildcard => Descriptor::Spelling("_"),
            TokenKind::IntLiteral => Descriptor::Named("integer literal"),
            TokenKind::FloatLiteral => Descriptor::Named("float literal"),
            TokenKind::StrLiteral => Descriptor::Named("string literal"),
            TokenKind::CharLiteral => Descriptor::Named("char literal"),
            TokenKind::Identifier => Descriptor::Named("identifier"),
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Descriptor::Spelling(name) | Descriptor::Named(name) => name,
        }
    }

    /// Returns the one source spelling every token of this kind has, or `None` for kinds that
    /// stand for a class of lexemes and so have no fixed spelling.
    pub(crate) fn spelling(self) -> Option<&'static str> {
        match self {
            Descriptor::Spelling(spelling) => Some(spelling),
            Descriptor::Named(_) => None,
        }
    }

    /// Whether this kind's spelling is made only of identifier characters, which is what makes
    /// it a kind the lexer could also have produced a [`TokenKind::Identifier`] for.
    pub(crate) fn is_word(self) -> bool {
        self.spelling().is_some_and(is_word_spelling)
    }

    /// Returns this kind's name for use in a diagnostic message.
    ///
    /// A kind with a [`Descriptor::spelling`] is wrapped in backticks so it reads as source text
    /// (`` `;` ``); one without is named by its [`Descriptor::name`] instead (`identifier`).
    /// `diagnostics::parser::Expected` relies on the backticks to tell the two apart.
    pub(crate) fn describe(self) -> String {
        match self {
            Descriptor::Spelling(spelling) => quoted_spelling(spelling),
            Descriptor::Named(name) => name.to_owned(),
        }
    }
}

/// Whether `text` is made only of identifier characters, which is what makes it the spelling of
/// a word-like token such as a keyword that the lexer could also have lexed as a name.
pub(crate) fn is_word_spelling(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub(crate) fn quoted_spelling(spelling: &str) -> String {
    format!("`{spelling}`")
}
