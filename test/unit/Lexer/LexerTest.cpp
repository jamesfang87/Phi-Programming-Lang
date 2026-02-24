#include <gtest/gtest.h>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"

#include <string>
#include <vector>

using namespace phi;

// Helper: lex source and return tokens (last token is always EOF)
static std::vector<Token> lex(const std::string &Src) {
  DiagnosticConfig Cfg;
  Cfg.UseColors = false;
  DiagnosticManager Diags(Cfg);
  Diags.getSrcManager().addSrcFile("test.phi", Src);
  Lexer L(Src, "test.phi", &Diags);
  return L.scan();
}

// Helper: lex and verify no errors
static std::vector<Token> lexOk(const std::string &Src) {
  DiagnosticConfig Cfg;
  Cfg.UseColors = false;
  DiagnosticManager Diags(Cfg);
  Diags.getSrcManager().addSrcFile("test.phi", Src);
  Lexer L(Src, "test.phi", &Diags);
  auto Tokens = L.scan();
  EXPECT_FALSE(Diags.hasError()) << "Unexpected lexer error for: " << Src;
  return Tokens;
}

//===----------------------------------------------------------------------===//
// Keywords
//===----------------------------------------------------------------------===//

TEST(Lexer, Keywords) {
  auto Tokens = lexOk("fun if else while for return const var struct enum "
                       "match import break continue defer public true false "
                       "as in module this");

  std::vector<TokenKind::Kind> Expected = {
      TokenKind::FunKw,      TokenKind::IfKw,       TokenKind::ElseKw,
      TokenKind::WhileKw,    TokenKind::ForKw,       TokenKind::ReturnKw,
      TokenKind::ConstKw,    TokenKind::VarKw,       TokenKind::StructKw,
      TokenKind::EnumKw,     TokenKind::MatchKw,     TokenKind::ImportKw,
      TokenKind::BreakKw,    TokenKind::ContinueKw,  TokenKind::DeferKw,
      TokenKind::PublicKw,   TokenKind::TrueKw,      TokenKind::FalseKw,
      TokenKind::AsKw,       TokenKind::InKw,        TokenKind::ModuleKw,
      TokenKind::ThisKw,     TokenKind::Eof,
  };

  ASSERT_EQ(Tokens.size(), Expected.size());
  for (size_t I = 0; I < Expected.size(); ++I) {
    EXPECT_EQ(Tokens[I].getKind().Value, Expected[I])
        << "Mismatch at token " << I << ": " << Tokens[I].getLexeme();
  }
}

//===----------------------------------------------------------------------===//
// Intrinsics
//===----------------------------------------------------------------------===//

TEST(Lexer, Intrinsics) {
  auto Tokens = lexOk("panic assert unreachable type_of");
  std::vector<TokenKind::Kind> Expected = {
      TokenKind::Panic, TokenKind::Assert, TokenKind::Unreachable,
      TokenKind::TypeOf, TokenKind::Eof,
  };
  ASSERT_EQ(Tokens.size(), Expected.size());
  for (size_t I = 0; I < Expected.size(); ++I) {
    EXPECT_EQ(Tokens[I].getKind().Value, Expected[I]);
  }
}

//===----------------------------------------------------------------------===//
// Built-in Type Keywords
//===----------------------------------------------------------------------===//

TEST(Lexer, TypeKeywords) {
  auto Tokens = lexOk("i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 string char bool");
  std::vector<TokenKind::Kind> Expected = {
      TokenKind::I8,  TokenKind::I16, TokenKind::I32,    TokenKind::I64,
      TokenKind::U8,  TokenKind::U16, TokenKind::U32,    TokenKind::U64,
      TokenKind::F32, TokenKind::F64, TokenKind::String, TokenKind::Char,
      TokenKind::BoolKw, TokenKind::Eof,
  };
  ASSERT_EQ(Tokens.size(), Expected.size());
  for (size_t I = 0; I < Expected.size(); ++I) {
    EXPECT_EQ(Tokens[I].getKind().Value, Expected[I]);
  }
}

//===----------------------------------------------------------------------===//
// Identifiers
//===----------------------------------------------------------------------===//

TEST(Lexer, Identifiers) {
  auto Tokens = lexOk("foo Bar my_var123 _underscore x");
  for (size_t I = 0; I < Tokens.size() - 1; ++I) {
    // _underscore maps to Wildcard only if it's exactly "_"
    if (Tokens[I].getLexeme() != "_") {
      EXPECT_EQ(Tokens[I].getKind().Value, TokenKind::Identifier)
          << "Token '" << Tokens[I].getLexeme() << "' should be Identifier";
    }
  }
  EXPECT_EQ(Tokens.back().getKind().Value, TokenKind::Eof);
}

TEST(Lexer, Wildcard) {
  auto Tokens = lexOk("_");
  ASSERT_GE(Tokens.size(), 2u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::Wildcard);
}

//===----------------------------------------------------------------------===//
// Integer Literals
//===----------------------------------------------------------------------===//

TEST(Lexer, IntLiterals) {
  auto Tokens = lexOk("0 42 1000000");
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::IntLiteral);
  EXPECT_EQ(Tokens[0].getLexeme(), "0");
  EXPECT_EQ(Tokens[1].getKind().Value, TokenKind::IntLiteral);
  EXPECT_EQ(Tokens[1].getLexeme(), "42");
  EXPECT_EQ(Tokens[2].getKind().Value, TokenKind::IntLiteral);
  EXPECT_EQ(Tokens[2].getLexeme(), "1000000");
}

//===----------------------------------------------------------------------===//
// Float Literals
//===----------------------------------------------------------------------===//

TEST(Lexer, FloatLiterals) {
  auto Tokens = lexOk("3.14 0.5 100.0");
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::FloatLiteral);
  EXPECT_EQ(Tokens[0].getLexeme(), "3.14");
  EXPECT_EQ(Tokens[1].getKind().Value, TokenKind::FloatLiteral);
  EXPECT_EQ(Tokens[1].getLexeme(), "0.5");
  EXPECT_EQ(Tokens[2].getKind().Value, TokenKind::FloatLiteral);
  EXPECT_EQ(Tokens[2].getLexeme(), "100.0");
}

//===----------------------------------------------------------------------===//
// String Literals
//===----------------------------------------------------------------------===//

TEST(Lexer, StringLiteral) {
  auto Tokens = lexOk(R"("hello")");
  ASSERT_GE(Tokens.size(), 2u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::StrLiteral);
}

TEST(Lexer, StringLiteralWithEscapes) {
  auto Tokens = lexOk(R"("escape: \n\t\\\"")");
  ASSERT_GE(Tokens.size(), 2u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::StrLiteral);
}

//===----------------------------------------------------------------------===//
// Char Literals
//===----------------------------------------------------------------------===//

TEST(Lexer, CharLiteral) {
  auto Tokens = lexOk("'a'");
  ASSERT_GE(Tokens.size(), 2u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::CharLiteral);
}

TEST(Lexer, CharLiteralEscape) {
  auto Tokens = lexOk(R"('\n')");
  ASSERT_GE(Tokens.size(), 2u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::CharLiteral);
}

TEST(Lexer, CharLiteralNull) {
  auto Tokens = lexOk(R"('\0')");
  ASSERT_GE(Tokens.size(), 2u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::CharLiteral);
}

//===----------------------------------------------------------------------===//
// Single-Character Operators & Punctuation
//===----------------------------------------------------------------------===//

TEST(Lexer, SingleCharOperators) {
  auto Tokens = lexOk("+ - * / % ! & ? . : = < > ( ) { } [ ] , ; |");
  std::vector<TokenKind::Kind> Expected = {
      TokenKind::Plus,         TokenKind::Minus,    TokenKind::Star,
      TokenKind::Slash,        TokenKind::Percent,  TokenKind::Bang,
      TokenKind::Amp,          TokenKind::Try,      TokenKind::Period,
      TokenKind::Colon,        TokenKind::Equals,   TokenKind::OpenCaret,
      TokenKind::CloseCaret,   TokenKind::OpenParen, TokenKind::CloseParen,
      TokenKind::OpenBrace,    TokenKind::CloseBrace,
      TokenKind::OpenBracket,  TokenKind::CloseBracket,
      TokenKind::Comma,        TokenKind::Semicolon, TokenKind::Pipe,
      TokenKind::Eof,
  };
  ASSERT_EQ(Tokens.size(), Expected.size());
  for (size_t I = 0; I < Expected.size(); ++I) {
    EXPECT_EQ(Tokens[I].getKind().Value, Expected[I])
        << "Mismatch at token " << I << ": '" << Tokens[I].getLexeme() << "'";
  }
}

//===----------------------------------------------------------------------===//
// Multi-Character Operators
//===----------------------------------------------------------------------===//

TEST(Lexer, MultiCharOperators) {
  auto Tokens = lexOk("-> => :: == != <= >= && || ++ -- += -= *= /= %= .. ..=");
  std::vector<TokenKind::Kind> Expected = {
      TokenKind::Arrow,        TokenKind::FatArrow,
      TokenKind::DoubleColon,  TokenKind::DoubleEquals,
      TokenKind::BangEquals,   TokenKind::LessEqual,
      TokenKind::GreaterEqual, TokenKind::DoubleAmp,
      TokenKind::DoublePipe,   TokenKind::DoublePlus,
      TokenKind::DoubleMinus,  TokenKind::PlusEquals,
      TokenKind::SubEquals,    TokenKind::MulEquals,
      TokenKind::DivEquals,    TokenKind::ModEquals,
      TokenKind::ExclRange,    TokenKind::InclRange,
      TokenKind::Eof,
  };
  ASSERT_EQ(Tokens.size(), Expected.size());
  for (size_t I = 0; I < Expected.size(); ++I) {
    EXPECT_EQ(Tokens[I].getKind().Value, Expected[I])
        << "Mismatch at token " << I << ": '" << Tokens[I].getLexeme() << "'";
  }
}

//===----------------------------------------------------------------------===//
// Comments
//===----------------------------------------------------------------------===//

TEST(Lexer, LineComment) {
  auto Tokens = lexOk("42 // this is a comment\n100");
  // Should get: IntLiteral(42), IntLiteral(100), Eof
  ASSERT_EQ(Tokens.size(), 3u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::IntLiteral);
  EXPECT_EQ(Tokens[0].getLexeme(), "42");
  EXPECT_EQ(Tokens[1].getKind().Value, TokenKind::IntLiteral);
  EXPECT_EQ(Tokens[1].getLexeme(), "100");
}

TEST(Lexer, BlockComment) {
  auto Tokens = lexOk("42 /* block comment */ 100");
  ASSERT_EQ(Tokens.size(), 3u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::IntLiteral);
  EXPECT_EQ(Tokens[1].getKind().Value, TokenKind::IntLiteral);
}

TEST(Lexer, MultilineBlockComment) {
  auto Tokens = lexOk("42 /* multi\nline\ncomment */ 100");
  ASSERT_EQ(Tokens.size(), 3u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::IntLiteral);
  EXPECT_EQ(Tokens[1].getKind().Value, TokenKind::IntLiteral);
}

//===----------------------------------------------------------------------===//
// Source Locations
//===----------------------------------------------------------------------===//

TEST(Lexer, SourceLocation) {
  auto Tokens = lexOk("fun main");
  ASSERT_GE(Tokens.size(), 3u);

  // 'fun' starts at line 1, col 1
  EXPECT_EQ(Tokens[0].getStart().Line, 1);
  EXPECT_EQ(Tokens[0].getStart().Col, 1);

  // 'main' starts at line 1, col 5
  EXPECT_EQ(Tokens[1].getStart().Line, 1);
  EXPECT_EQ(Tokens[1].getStart().Col, 5);
}

TEST(Lexer, SourceLocationMultiLine) {
  auto Tokens = lexOk("fun\nmain");
  ASSERT_GE(Tokens.size(), 3u);

  EXPECT_EQ(Tokens[0].getStart().Line, 1);
  EXPECT_EQ(Tokens[1].getStart().Line, 2);
  EXPECT_EQ(Tokens[1].getStart().Col, 1);
}

//===----------------------------------------------------------------------===//
// Empty Input
//===----------------------------------------------------------------------===//

TEST(Lexer, EmptyInput) {
  auto Tokens = lexOk("");
  ASSERT_EQ(Tokens.size(), 1u);
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::Eof);
}

//===----------------------------------------------------------------------===//
// Complex Token Sequences
//===----------------------------------------------------------------------===//

TEST(Lexer, FunctionDeclaration) {
  auto Tokens = lexOk("fun add(const a: i32, const b: i32) -> i32 { return a + b; }");
  // Verify key tokens
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::FunKw);
  EXPECT_EQ(Tokens[1].getKind().Value, TokenKind::Identifier);
  EXPECT_EQ(Tokens[1].getLexeme(), "add");
  EXPECT_EQ(Tokens[2].getKind().Value, TokenKind::OpenParen);
}

TEST(Lexer, StructDeclaration) {
  auto Tokens = lexOk("struct Point { public x: f64, public y: f64 }");
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::StructKw);
  EXPECT_EQ(Tokens[1].getKind().Value, TokenKind::Identifier);
  EXPECT_EQ(Tokens[1].getLexeme(), "Point");
}

TEST(Lexer, ArrayType) {
  auto Tokens = lexOk("[i32]");
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::OpenBracket);
  EXPECT_EQ(Tokens[1].getKind().Value, TokenKind::I32);
  EXPECT_EQ(Tokens[2].getKind().Value, TokenKind::CloseBracket);
}

TEST(Lexer, GenericType) {
  auto Tokens = lexOk("Option<i32>");
  EXPECT_EQ(Tokens[0].getKind().Value, TokenKind::Identifier);
  EXPECT_EQ(Tokens[0].getLexeme(), "Option");
  EXPECT_EQ(Tokens[1].getKind().Value, TokenKind::OpenCaret);
  EXPECT_EQ(Tokens[2].getKind().Value, TokenKind::I32);
  EXPECT_EQ(Tokens[3].getKind().Value, TokenKind::CloseCaret);
}
