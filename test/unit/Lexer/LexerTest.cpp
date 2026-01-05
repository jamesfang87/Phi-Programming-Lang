//===----------------------------------------------------------------------===//
//
// Lexer tests for the Phi programming language
//
//===----------------------------------------------------------------------===//

#include "Lexer/Lexer.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "SrcManager/SrcManager.hpp"

#include "llvm/Support/raw_ostream.h"
#include <gtest/gtest.h>

using namespace phi;

namespace {

class LexerTest : public ::testing::Test {
protected:
  LexerTest() {
    SrcMgr = std::make_shared<SrcManager>();
    DiagMgr = std::make_shared<DiagnosticManager>(SrcMgr);
  }

  std::vector<Token> lex(const std::string &Source,
                         const std::string &Path = "!none") {
    Lexer L(Source, Path, DiagMgr);
    return L.scan();
  }

  std::string dumpTokens(const std::vector<Token> &Tokens) {
    std::string Out;
    llvm::raw_string_ostream OS(Out);
    for (const auto &Tok : Tokens)
      OS << Tok.toString() << "\n";
    return OS.str();
  }

  std::shared_ptr<SrcManager> SrcMgr;
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

} // namespace
