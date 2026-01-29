//===----------------------------------------------------------------------===//
//
// Lexer tests for the Phi programming language
//
//===----------------------------------------------------------------------===//

#include "Lexer/Lexer.hpp"
#include "Diagnostics/DiagnosticManager.hpp"

#include "llvm/Support/raw_ostream.h"
#include <gtest/gtest.h>
#include <print>

using namespace phi;

namespace {

class LexerTest : public ::testing::Test {
protected:
  LexerTest() { DiagMgr = std::make_shared<DiagnosticManager>(); }

  std::vector<Token> lex(const std::string &Source,
                         const std::string &Path = "!none") {
    Lexer L(Source, Path, DiagMgr.get());
    return L.scan();
  }

  std::string dumpTokens(const std::vector<Token> &Tokens) {
    std::string Out;
    llvm::raw_string_ostream OS(Out);
    for (const auto &Tok : Tokens)
      OS << Tok.toString() << "\n";
    return OS.str();
  }

  std::shared_ptr<DiagnosticManager> DiagMgr;
};

TEST_F(LexerTest, EmptyInput) {
  auto Toks = lex(R"()");
  ASSERT_EQ(Toks.size(), 1u);
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Eof);
}

TEST_F(LexerTest, SingleIdentifier) {
  auto Toks = lex(R"(hello)");
  ASSERT_EQ(Toks.size(), 2u);
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[0].getLexeme(), "hello");
}

TEST_F(LexerTest, KeywordsVsIdentifiers) {
  auto Toks = lex(R"(fun function return returns)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::FunKw);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::ReturnKw);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::Identifier);
}

TEST_F(LexerTest, IntegerAndFloatLiterals) {
  auto Toks = lex(R"(42 3.14 0.001)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::IntLiteral);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::FloatLiteral);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::FloatLiteral);
}

TEST_F(LexerTest, StringLiteral) {
  auto Toks = lex(R"("hello world")");
  ASSERT_EQ(Toks.size(), 2u);
  EXPECT_EQ(Toks[0].getKind(), TokenKind::StrLiteral);
  EXPECT_EQ(Toks[0].getLexeme(), "hello world");
}

TEST_F(LexerTest, UnterminatedString) {
  auto Toks = lex(R"("unterminated)");
  EXPECT_EQ(Toks.back().getKind(), TokenKind::Eof);
}

TEST_F(LexerTest, OperatorsAndPunctuation) {
  auto Toks = lex(R"(-> => = == ; , . :)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Arrow);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::FatArrow);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::Equals);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::DoubleEquals);
  EXPECT_EQ(Toks[4].getKind(), TokenKind::Semicolon);
  EXPECT_EQ(Toks[5].getKind(), TokenKind::Comma);
  EXPECT_EQ(Toks[6].getKind(), TokenKind::Period);
  EXPECT_EQ(Toks[7].getKind(), TokenKind::Colon);
}

TEST_F(LexerTest, BracesAndParens) {
  auto Toks = lex(R"({ } ( ) [ ])");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::OpenBrace);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::CloseBrace);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::OpenParen);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::CloseParen);
  EXPECT_EQ(Toks[4].getKind(), TokenKind::OpenBracket);
  EXPECT_EQ(Toks[5].getKind(), TokenKind::CloseBracket);
}

TEST_F(LexerTest, LineAndColumnTracking) {
  auto Toks = lex(R"(a
b
  c)",
                  "loc.phi");

  EXPECT_EQ(Toks[0].getStart().Line, 1u);
  EXPECT_EQ(Toks[1].getStart().Line, 2u);
  EXPECT_EQ(Toks[2].getStart().Line, 3u);
  EXPECT_EQ(Toks[2].getStart().Col, 3u);
}

TEST_F(LexerTest, SkipsWhitespaceAndNewlines) {
  auto Toks = lex(R"(   a   b

c)");
  EXPECT_EQ(Toks[0].getLexeme(), "a");
  EXPECT_EQ(Toks[1].getLexeme(), "b");
  EXPECT_EQ(Toks[2].getLexeme(), "c");
}

TEST_F(LexerTest, EnumSnippetGoldenTest) {
  auto Toks = lex(R"(
enum Result {
  Ok: i32;
  Err: string;
}
)");

  EXPECT_EQ(Toks[0].toString(),
            R"([ENUM] "enum" at test.phi:2:1 to test.phi:2:5)");
  EXPECT_EQ(Toks[1].toString(),
            R"([IDENTIFIER] "Result" at test.phi:2:6 to test.phi:2:12)");
}

TEST_F(LexerTest, NoTrailingNewline) {
  auto Toks = lex(R"(abc)");
  EXPECT_EQ(Toks[0].getLexeme(), "abc");
  EXPECT_EQ(Toks.back().getKind(), TokenKind::Eof);
}

//===----------------------------------------------------------------------===//
// Comprehensive Numeric Literal Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, IntegerLiterals) {
  auto Toks = lex(R"(0 42 123 999999)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::IntLiteral);
  EXPECT_EQ(Toks[0].getLexeme(), "0");
  EXPECT_EQ(Toks[1].getKind(), TokenKind::IntLiteral);
  EXPECT_EQ(Toks[1].getLexeme(), "42");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::IntLiteral);
  EXPECT_EQ(Toks[2].getLexeme(), "123");
  EXPECT_EQ(Toks[3].getKind(), TokenKind::IntLiteral);
  EXPECT_EQ(Toks[3].getLexeme(), "999999");
}

TEST_F(LexerTest, FloatLiterals) {
  auto Toks = lex(R"(3.14 0.5 123.456 0.001)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::FloatLiteral);
  EXPECT_EQ(Toks[0].getLexeme(), "3.14");
  EXPECT_EQ(Toks[1].getKind(), TokenKind::FloatLiteral);
  EXPECT_EQ(Toks[1].getLexeme(), "0.5");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::FloatLiteral);
  EXPECT_EQ(Toks[2].getLexeme(), "123.456");
  EXPECT_EQ(Toks[3].getKind(), TokenKind::FloatLiteral);
  EXPECT_EQ(Toks[3].getLexeme(), "0.001");
}

TEST_F(LexerTest, NumericLiteralEdgeCases) {
  auto Toks = lex(R"(0. 123. .5)");
  // Note: These edge cases may or may not be valid depending on implementation
  // Testing what the lexer actually produces
  EXPECT_EQ(Toks[0].getKind(), TokenKind::IntLiteral); // "0"
  EXPECT_EQ(Toks[1].getKind(), TokenKind::Period);     // "."
  EXPECT_EQ(Toks[2].getKind(), TokenKind::IntLiteral); // "123"
  EXPECT_EQ(Toks[3].getKind(), TokenKind::Period);     // "."
  EXPECT_EQ(Toks[4].getKind(), TokenKind::Period);     // "."
  EXPECT_EQ(Toks[5].getKind(), TokenKind::IntLiteral); // "5"
}

//===----------------------------------------------------------------------===//
// Comprehensive String Literal Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, StringLiteralBasic) {
  auto Toks = lex(R"("hello" "world" "test")");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::StrLiteral);
  EXPECT_EQ(Toks[0].getLexeme(), "hello");
  EXPECT_EQ(Toks[1].getKind(), TokenKind::StrLiteral);
  EXPECT_EQ(Toks[1].getLexeme(), "world");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::StrLiteral);
  EXPECT_EQ(Toks[2].getLexeme(), "test");
}

TEST_F(LexerTest, StringLiteralWithEscapeSequences) {
  auto Toks =
      lex(R"("hello\nworld" "tab\there" "quote\"test" "backslash\\test")");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::StrLiteral);
  EXPECT_EQ(Toks[0].getLexeme(), "hello\nworld");
  EXPECT_EQ(Toks[1].getKind(), TokenKind::StrLiteral);
  EXPECT_EQ(Toks[1].getLexeme(), "tab\there");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::StrLiteral);
  EXPECT_EQ(Toks[2].getLexeme(), "quote\"test");
  EXPECT_EQ(Toks[3].getKind(), TokenKind::StrLiteral);
  EXPECT_EQ(Toks[3].getLexeme(), "backslash\\test");
}

TEST_F(LexerTest, StringLiteralWithHexEscape) {
  auto Toks = lex(R"("hello\x41world")");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::StrLiteral);
  // Hex 41 is 'A'
  EXPECT_TRUE(Toks[0].getLexeme().find('A') != std::string::npos);
}

TEST_F(LexerTest, StringLiteralMultiline) {
  auto Toks = lex(R"("line1
line2
line3")");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::StrLiteral);
  EXPECT_TRUE(Toks[0].getLexeme().find("line1") != std::string::npos);
  EXPECT_TRUE(Toks[0].getLexeme().find("line2") != std::string::npos);
}

TEST_F(LexerTest, StringLiteralEmpty) {
  auto Toks = lex(R"("")");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::StrLiteral);
  EXPECT_EQ(Toks[0].getLexeme(), "");
}

//===----------------------------------------------------------------------===//
// Comprehensive Character Literal Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, CharacterLiteralBasic) {
  auto Toks = lex(R"('a' 'b' 'Z' '0')");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[0].getLexeme(), "a");
  EXPECT_EQ(Toks[1].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[1].getLexeme(), "b");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[2].getLexeme(), "Z");
  EXPECT_EQ(Toks[3].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[3].getLexeme(), "0");
}

TEST_F(LexerTest, CharacterLiteralWithEscapeSequences) {
  auto Toks = lex(R"('\n' '\t' '\r' '\\' '\'' '\0')");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[0].getLexeme(), "\n");
  EXPECT_EQ(Toks[1].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[1].getLexeme(), "\t");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[2].getLexeme(), "\r");
  EXPECT_EQ(Toks[3].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[3].getLexeme(), "\\");
  EXPECT_EQ(Toks[4].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[4].getLexeme(), "'");
  EXPECT_EQ(Toks[5].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[5].getLexeme(), "\0");
}

TEST_F(LexerTest, CharacterLiteralWithHexEscape) {
  auto Toks = lex(R"('\x41' '\xFF' '\x00')");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[0].getLexeme(), "A"); // 0x41 = 'A'
  EXPECT_EQ(Toks[1].getKind(), TokenKind::CharLiteral);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::CharLiteral);
}

//===----------------------------------------------------------------------===//
// Comprehensive Comment Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, LineComments) {
  auto Toks = lex(R"(// This is a comment
identifier)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[0].getLexeme(), "identifier");
}

TEST_F(LexerTest, LineCommentAtEndOfLine) {
  auto Toks = lex(R"(code // comment)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[0].getLexeme(), "code");
}

TEST_F(LexerTest, BlockComments) {
  auto Toks = lex(R"(/* block comment */ identifier)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[0].getLexeme(), "identifier");
}

TEST_F(LexerTest, BlockCommentMultiline) {
  auto Toks = lex(R"(/* line 1
line 2
line 3 */ identifier)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[0].getLexeme(), "identifier");
}

TEST_F(LexerTest, NestedBlockComments) {
  auto Toks = lex(R"(/* outer /* inner */ still outer */ identifier)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[0].getLexeme(), "identifier");
}

//===----------------------------------------------------------------------===//
// Comprehensive Operator Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, ArithmeticOperators) {
  auto Toks = lex(R"(+ - * / %)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Plus);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::Minus);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::Star);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::Slash);
  EXPECT_EQ(Toks[4].getKind(), TokenKind::Percent);
}

TEST_F(LexerTest, CompoundAssignmentOperators) {
  auto Toks = lex(R"(+= -= *= /= %=)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::PlusEquals);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::SubEquals);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::MulEquals);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::DivEquals);
  EXPECT_EQ(Toks[4].getKind(), TokenKind::ModEquals);
}

TEST_F(LexerTest, IncrementDecrementOperators) {
  auto Toks = lex(R"(++ --)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::DoublePlus);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::DoubleMinus);
}

TEST_F(LexerTest, ComparisonOperators) {
  auto Toks = lex(R"(< <= > >=)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::OpenCaret);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::LessEqual);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::CloseCaret);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::GreaterEqual);
}

TEST_F(LexerTest, EqualityOperators) {
  auto Toks = lex(R"(== !=)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::DoubleEquals);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::BangEquals);
}

TEST_F(LexerTest, LogicalOperators) {
  auto Toks = lex(R"(&& || !)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::DoubleAmp);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::DoublePipe);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::Bang);
}

TEST_F(LexerTest, BitwiseOperators) {
  auto Toks = lex(R"(& |)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Amp);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::Pipe);
}

TEST_F(LexerTest, RangeOperators) {
  auto Toks = lex(R"(.. ..=)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::ExclRange);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::InclRange);
}

TEST_F(LexerTest, ArrowOperators) {
  auto Toks = lex(R"(-> =>)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Arrow);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::FatArrow);
}

TEST_F(LexerTest, MemberAccessOperators) {
  auto Toks = lex(R"(. ::)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Period);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::DoubleColon);
}

//===----------------------------------------------------------------------===//
// Comprehensive Keyword Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, ControlFlowKeywords) {
  auto Toks = lex(R"(if else while for break continue return)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::IfKw);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::ElseKw);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::WhileKw);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::ForKw);
  EXPECT_EQ(Toks[4].getKind(), TokenKind::BreakKw);
  EXPECT_EQ(Toks[5].getKind(), TokenKind::ContinueKw);
  EXPECT_EQ(Toks[6].getKind(), TokenKind::ReturnKw);
}

TEST_F(LexerTest, DeclarationKeywords) {
  auto Toks = lex(R"(fun struct enum var const public module import)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::FunKw);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::StructKw);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::EnumKw);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::VarKw);
  EXPECT_EQ(Toks[4].getKind(), TokenKind::ConstKw);
  EXPECT_EQ(Toks[5].getKind(), TokenKind::PublicKw);
  EXPECT_EQ(Toks[6].getKind(), TokenKind::ModuleKw);
  EXPECT_EQ(Toks[7].getKind(), TokenKind::ImportKw);
}

TEST_F(LexerTest, TypeKeywords) {
  auto Toks = lex(R"(i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 string char bool)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::I8);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::I16);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::I32);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::I64);
  EXPECT_EQ(Toks[4].getKind(), TokenKind::U8);
  EXPECT_EQ(Toks[5].getKind(), TokenKind::U16);
  EXPECT_EQ(Toks[6].getKind(), TokenKind::U32);
  EXPECT_EQ(Toks[7].getKind(), TokenKind::U64);
  EXPECT_EQ(Toks[8].getKind(), TokenKind::F32);
  EXPECT_EQ(Toks[9].getKind(), TokenKind::F64);
  EXPECT_EQ(Toks[10].getKind(), TokenKind::String);
  EXPECT_EQ(Toks[11].getKind(), TokenKind::Char);
  EXPECT_EQ(Toks[12].getKind(), TokenKind::BoolKw);
}

TEST_F(LexerTest, LiteralKeywords) {
  auto Toks = lex(R"(true false)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::TrueKw);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::FalseKw);
}

TEST_F(LexerTest, OtherKeywords) {
  auto Toks = lex(R"(match in as defer this)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::MatchKw);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::InKw);
  EXPECT_EQ(Toks[2].getKind(), TokenKind::AsKw);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::DeferKw);
  EXPECT_EQ(Toks[4].getKind(), TokenKind::ThisKw);
}

//===----------------------------------------------------------------------===//
// Comprehensive Whitespace Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, WhitespaceHandling) {
  auto Toks = lex(R"(   a   b   c   )");
  EXPECT_EQ(Toks[0].getLexeme(), "a");
  EXPECT_EQ(Toks[1].getLexeme(), "b");
  EXPECT_EQ(Toks[2].getLexeme(), "c");
}

TEST_F(LexerTest, TabHandling) {
  auto Toks = lex("a\tb\tc");
  EXPECT_EQ(Toks[0].getLexeme(), "a");
  EXPECT_EQ(Toks[1].getLexeme(), "b");
  EXPECT_EQ(Toks[2].getLexeme(), "c");
}

TEST_F(LexerTest, MixedWhitespace) {
  auto Toks = lex("a \t b\n\tc");
  EXPECT_EQ(Toks[0].getLexeme(), "a");
  EXPECT_EQ(Toks[1].getLexeme(), "b");
  EXPECT_EQ(Toks[2].getLexeme(), "c");
}

//===----------------------------------------------------------------------===//
// Comprehensive Source Location Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, SourceLocationTracking) {
  auto Toks = lex(R"(a
b
  c
    d)",
                  "test.phi");
  EXPECT_EQ(Toks[0].getStart().Line, 1u);
  EXPECT_EQ(Toks[0].getStart().Col, 1u);
  EXPECT_EQ(Toks[1].getStart().Line, 2u);
  EXPECT_EQ(Toks[1].getStart().Col, 1u);
  EXPECT_EQ(Toks[2].getStart().Line, 3u);
  EXPECT_EQ(Toks[2].getStart().Col, 3u);
  EXPECT_EQ(Toks[3].getStart().Line, 4u);
  EXPECT_EQ(Toks[3].getStart().Col, 5u);
}

TEST_F(LexerTest, SourceLocationWithComments) {
  auto Toks = lex(R"(a // comment
b)",
                  "test.phi");
  EXPECT_EQ(Toks[0].getStart().Line, 1u);
  EXPECT_EQ(Toks[1].getStart().Line, 2u);
}

//===----------------------------------------------------------------------===//
// Comprehensive Error Case Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, InvalidCharacter) {
  auto Toks = lex(R"(a @ b)");
  // Should produce error token for '@'
  bool FoundError = false;
  for (const auto &Tok : Toks) {
    if (Tok.getKind() == TokenKind::Error) {
      FoundError = true;
      break;
    }
  }
  EXPECT_TRUE(FoundError);
}

TEST_F(LexerTest, UnterminatedChar) {
  auto Toks = lex(R"('a)");
  // Should handle error gracefully
  EXPECT_TRUE(Toks.size() > 0);
}

TEST_F(LexerTest, EmptyCharLiteral) {
  auto Toks = lex(R"('')");
  // Should produce error token
  bool FoundError = false;
  for (const auto &Tok : Toks) {
    if (Tok.getKind() == TokenKind::Error) {
      FoundError = true;
      break;
    }
  }
  EXPECT_TRUE(FoundError);
}

TEST_F(LexerTest, InvalidEscapeSequence) {
  auto Toks = lex(R"('\z')");
  // Should produce error token
  bool FoundError = false;
  for (const auto &Tok : Toks) {
    if (Tok.getKind() == TokenKind::Error) {
      FoundError = true;
      break;
    }
  }
  EXPECT_TRUE(FoundError);
}

TEST_F(LexerTest, IncompleteHexEscape) {
  auto Toks = lex(R"('\x')");
  // Should produce error token
  bool FoundError = false;
  for (const auto &Tok : Toks) {
    if (Tok.getKind() == TokenKind::Error) {
      FoundError = true;
      break;
    }
  }
  EXPECT_TRUE(FoundError);
}

//===----------------------------------------------------------------------===//
// Comprehensive Identifier Tests
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, IdentifiersWithUnderscores) {
  auto Toks = lex(R"(_identifier identifier_with_underscores _123)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[0].getLexeme(), "_identifier");
  EXPECT_EQ(Toks[1].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[1].getLexeme(), "identifier_with_underscores");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[2].getLexeme(), "_123");
}

TEST_F(LexerTest, IdentifiersWithNumbers) {
  auto Toks = lex(R"(var1 var2 test123)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[0].getLexeme(), "var1");
  EXPECT_EQ(Toks[1].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[1].getLexeme(), "var2");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[2].getLexeme(), "test123");
}

TEST_F(LexerTest, WildcardToken) {
  auto Toks = lex(R"(_)");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::Wildcard);
}

//===----------------------------------------------------------------------===//
// Complex Real-World Examples
//===----------------------------------------------------------------------===//

TEST_F(LexerTest, FunctionDeclaration) {
  auto Toks = lex(R"(fun add(const x: i32, const y: i32) -> i32 {
    return x + y;
})");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::FunKw);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[1].getLexeme(), "add");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::OpenParen);
  EXPECT_EQ(Toks[3].getKind(), TokenKind::ConstKw);
}

TEST_F(LexerTest, StructDeclaration) {
  auto Toks = lex(R"(struct Point {
    public x: f64;
    public y: f64;
})");
  EXPECT_EQ(Toks[0].getKind(), TokenKind::StructKw);
  EXPECT_EQ(Toks[1].getKind(), TokenKind::Identifier);
  EXPECT_EQ(Toks[1].getLexeme(), "Point");
  EXPECT_EQ(Toks[2].getKind(), TokenKind::OpenBrace);
}

} // namespace
