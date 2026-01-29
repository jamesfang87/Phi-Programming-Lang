#include "Lexer/Lexer.hpp"

#include <regex>
#include <unordered_map>

#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

/**
 * @brief Parses numeric literals (integers and floating-point numbers)
 *
 * This method handles the parsing of numeric literals, automatically detecting
 * whether the number is an integer or floating-point based on the presence of
 * a decimal point. The method supports:
 * - Integer literals: 42, 123, 0
 * - Floating-point literals: 3.14, 0.5, 123.456
 *
 * @note Exponential notation (e.g., 1.23e4) is planned but not yet implemented
 * @return A token of type tok_int_literal or tok_float_literal
 */
Token Lexer::parseNumber() {
  while (std::isdigit(peekChar())) {
    advanceChar();
  }

  // fractional part, only if the next two chars match .[0-9]
  std::regex pattern("^.[0-9]$");
  if (std::regex_match(peekNextN(2), pattern)) {
    advanceChar(); // consume '.'
    while (std::isdigit(peekChar())) {
      advanceChar();
    }
  } else {
    return makeToken(TokenKind::IntLiteral);
  }

  // TODO(jamesfang): implement exponents
  return makeToken(TokenKind::FloatLiteral);
}

/**
 * @brief Parses identifiers and distinguishes them from keywords
 *
 * This method parses sequences of alphanumeric characters and underscores
 * that start with a letter or underscore. It then checks the parsed identifier
 * against the keyword table to determine if it should be tokenized
 * as a keyword or as a user-defined identifier.
 *
 * The keyword table includes:
 * - Control flow keywords (if, else, for, while, etc.)
 * - Declaration keywords (let, fun, class, etc.)
 * - Type keywords (i32, str, bool, etc.)
 * - Literal keywords (true, false)
 *
 * @return A token of the appropriate keyword type or tok_identifier
 */
Token Lexer::parseIdentifierOrKw() {
  while (std::isalnum(peekChar()) || peekChar() == '_') {
    advanceChar();
  }
  const std::string Id(CurLexeme, CurChar);

  static const std::unordered_map<std::string, TokenKind::Kind> Kws = {
      {"as", TokenKind::AsKw},
      {"bool", TokenKind::BoolKw},
      {"break", TokenKind::BreakKw},
      {"const", TokenKind::ConstKw},
      {"continue", TokenKind::ContinueKw},
      {"defer", TokenKind::DeferKw},
      {"else", TokenKind::ElseKw},
      {"enum", TokenKind::EnumKw},
      {"false", TokenKind::FalseKw},
      {"for", TokenKind::ForKw},
      {"fun", TokenKind::FunKw},
      {"if", TokenKind::IfKw},
      {"import", TokenKind::ImportKw},
      {"match", TokenKind::MatchKw},
      {"module", TokenKind::ModuleKw},
      {"in", TokenKind::InKw},
      {"public", TokenKind::PublicKw},
      {"return", TokenKind::ReturnKw},
      {"struct", TokenKind::StructKw},
      {"true", TokenKind::TrueKw},
      {"this", TokenKind::ThisKw},
      {"var", TokenKind::VarKw},
      {"while", TokenKind::WhileKw},
      {"i8", TokenKind::I8},
      {"i16", TokenKind::I16},
      {"i32", TokenKind::I32},
      {"i64", TokenKind::I64},
      {"u8", TokenKind::U8},
      {"u16", TokenKind::U16},
      {"u32", TokenKind::U32},
      {"u64", TokenKind::U64},
      {"f32", TokenKind::F32},
      {"f64", TokenKind::F64},
      {"string", TokenKind::String},
      {"char", TokenKind::Char},
      {"panic", TokenKind::Panic},
      {"assert", TokenKind::Assert},
      {"unreachable", TokenKind::Unreachable},
      {"type_of", TokenKind::TypeOf}};

  const auto It = Kws.find(Id);
  return It != Kws.end() ? makeToken(It->second)
                         : makeToken(TokenKind::Identifier);
}

/**
 * @brief Parses string literals with escape sequence support
 *
 * This method parses a complete string literal enclosed in double quotes.
 * It handles escape sequences, multi-line strings, and proper error reporting
 * for unterminated strings. The method tracks line numbers correctly when
 * strings span multiple lines.
 *
 * Features:
 * - Full escape sequence support via parse_escape_sequence()
 * - Multi-line string support
 * - Proper line number tracking
 * - Error reporting for unterminated strings
 *
 * @return A token of type tok_str_literal containing the parsed string content
 */
Token Lexer::parseString() {
  InsideStr = true;
  const int StartLineNum = LineNum;
  auto StartPos = CurLexeme;
  auto StartLinePos = LexemeLine;
  std::string Str;

  // parse until we see closing double quote
  while (InsideStr && !atEOF() && peekChar() != '"') {
    if (peekChar() == '\\') {
      advanceChar(); // consume the forward slash
      Str.push_back(parseEscapeSeq());
      continue;
    }
    // increment line number if we see '\n'
    if (peekChar() == '\n') {
      LineNum++;
      CurLine = CurChar;
      advanceChar();
    } else {
      Str.push_back(advanceChar());
    }
  }

  if (atEOF()) {
    // reached eof without finding closing double quote
    int Col = static_cast<int>(StartPos - StartLinePos) + 1;
    SrcLocation Start{.Path = Path, .Line = StartLineNum, .Col = Col};
    SrcSpan Span{Start};
    error("unterminated string literal")
        .with_primary_label(Span, "string starts here")
        .with_help("add a closing double quote (\") to terminate the string")
        .emit(*Diags);
    return makeToken(TokenKind::Error);
  }
  advanceChar();     // consume closing quote
  InsideStr = false; // we are no longer inside str

  SrcLocation Start = {.Path = this->Path,
                       .Line = StartLineNum,
                       .Col = static_cast<int>(CurLexeme - LexemeLine) + 1};

  SrcLocation End = {.Path = this->Path,
                     .Line = LineNum,
                     .Col = static_cast<int>(CurChar - CurLine) + 1};

  return {Start, End, TokenKind::StrLiteral, Str};
}

/**
 * @brief Parses character literals with escape sequence support
 *
 * This method parses a complete character literal enclosed in single quotes.
 * It supports both regular characters and escape sequences. The character
 * literal must contain exactly one character (after escape sequence
 * processing).
 *
 * Examples:
 * - 'a' -> character 'a'
 * - '\n' -> newline character
 * - '\x41' -> character 'A'
 *
 * @return A token of type tok_char_literal containing the character value
 * @throws Scanning error for unterminated character literals
 */
Token Lexer::parseChar() {
  // handle case that char literal is empty
  if (peekChar() == '\'') {
    advanceChar(); // consume the opening quote
    advanceChar(); // consume the closing quote
    error("empty character literal")
        .with_primary_label(getCurSpan(), "character literal is empty")
        .with_help("character literals must contain exactly one character")
        .with_note(
            "try using a space character: ' ' or an escape sequence like '\\n'")
        .emit(*Diags);
    return makeToken(TokenKind::Error);
  }

  char C = 0;
  if (peekChar() != '\\') {
    C = advanceChar();
  } else {
    advanceChar(); // consume the forward slash
    C = parseEscapeSeq();
  }

  if (peekChar() == '\'') {
    advanceChar(); // consume closing quote

    SrcLocation Start = {.Path = this->Path,
                         .Line = LineNum,
                         .Col = static_cast<int>(CurLexeme - LexemeLine) + 1};

    SrcLocation End = {.Path = this->Path,
                       .Line = LineNum,
                       .Col = static_cast<int>(CurChar - CurLine) + 1};

    return {Start, End, TokenKind::CharLiteral, std::string(1, C)};
  }

  if (atEOF() || peekChar() == '\n' || peekChar() == ';') {
    int Col = static_cast<int>(CurLexeme - LexemeLine) + 1;
    SrcLocation Start{.Path = Path, .Line = LineNum, .Col = Col};
    SrcSpan Span{Start};

    error("unterminated character literal")
        .with_primary_label(Span, "character started here")
        .with_help("add a closing single quote (') to terminate the character")
        .emit(*Diags);
  } else {
    error("character literal contains too many characters")
        .with_primary_label(getCurSpan(), "too many characters")
        .with_help("character literals must contain exactly one character")
        .with_note("use a string literal (\"\") for multiple characters")
        .emit(*Diags);
  }
  return makeToken(TokenKind::Error);
}

/**
 * @brief Parses escape sequences within string and character literals
 *
 * This method handles the parsing of escape sequences that begin with a
 * backslash. Supported escape sequences include:
 * - \' (single quote)
 * - \" (double quote)
 * - \n (newline)
 * - \t (tab)
 * - \r (carriage return)
 * - \\\ (backslash)
 * - \0 (null character)
 * - \xNN (hexadecimal escape)
 *
 * @return The actual character value represented by the escape sequence
 * @throws Scanning error for invalid escape sequences
 */
char Lexer::parseEscapeSeq() {
  if (atEOF()) {
    error("unfinished escape sequence")
        .with_primary_label(getCurSpan(), "escape sequence incomplete")
        .with_help("add a valid escape character after the backslash")
        .with_note("valid escape sequences: \\n, \\t, \\r, \\\\, \\\", \\', "
                   "\\0, \\xNN")
        .emit(*Diags);
    return '\0';
  }

  // save location for error reporting
  SrcLocation Loc = getCurLocation();
  switch (const char C = advanceChar()) {
  case '\'':
    return '\'';
  case '"':
    return '\"';
  case 'n':
    return '\n';
  case 't':
    return '\t';
  case 'r':
    return '\r';
  case '\\':
    return '\\';
  case '0':
    return '\0';
  case 'x':
    return parseHexEscape();
  default:
    error("unknown escape sequence")
        .with_primary_label(
            SrcSpan(Loc),
            std::format("invalid char for escape sequence '\\{}'", C))
        .with_help("use a valid escape sequence")
        .with_note("valid escape sequences: \\n, \\t, \\r, \\\\, \\\", \\', "
                   "\\0, \\xNN")
        .emit(*Diags);
    return C; // Return character as-is for error recovery
  }
}

/**
 * @brief Parses hexadecimal escape sequences (\xNN format)
 *
 * This method parses exactly two hexadecimal digits following \x and converts
 * them to the corresponding character value. For example, \x41 becomes 'A'.
 *
 * @return The character value represented by the hex digits
 * @throws Scanning error if EOF is reached or invalid hex digits are found
 */
char Lexer::parseHexEscape() {
  if (!isxdigit(peekChar()) || !isxdigit(peekNext())) {
    error("incomplete hexadecimal escape sequence")
        .with_primary_label(getCurSpan(), "expected two hex digits here")
        .with_help(
            "hexadecimal escapes require exactly two digits: \\x00 to \\xFF")
        .with_note("example: \\x41 represents the character 'A'")
        .emit(*Diags);
    return '\0';
  }

  char Hex[3] = {0};
  for (int I = 0; I < 2; I++) {
    if (atEOF() || !isxdigit(peekChar())) {
      error("incomplete hexadecimal escape sequence")
          .with_primary_label(getCurSpan(), "expected hex digit here")
          .with_help("hexadecimal escapes require exactly two digits")
          .emit(*Diags);
      return '\0';
    }
    Hex[I] = advanceChar();
  }
  return static_cast<char>(std::stoi(Hex, nullptr, 16));
}

} // namespace phi
