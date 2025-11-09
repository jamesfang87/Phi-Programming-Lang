/*
#include <gtest/gtest.h>
#include <memory>
#include <sstream>
#include <string>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "SrcManager/SrcManager.hpp"

using namespace phi;

class ParserTest : public ::testing::Test {
protected:
  void SetUp() override {
    SrcMan = std::make_shared<SrcManager>();
    DiagMan = std::make_shared<DiagnosticManager>(SrcMan);
    LastLexer.reset();
  }

  /// Helper to create a parser for a snippet of Phi code
  std::unique_ptr<Parser> makeParser(const std::string &Code) {
    std::string FilePath = "test.phi";
    SrcMan->addSrcFile(FilePath, Code);

    LastLexer = std::make_unique<Lexer>(Code, FilePath, DiagMan);
    auto Tokens = LastLexer->scan();
    return std::make_unique<Parser>(Code, FilePath, Tokens, DiagMan);
  }

  std::shared_ptr<SrcManager> SrcMan;
  std::shared_ptr<DiagnosticManager> DiagMan;
  std::unique_ptr<Lexer> LastLexer;
};

//===----------------------------------------------------------------------===//
// CustomTypeCtor parsing tests
//===----------------------------------------------------------------------===//

TEST_F(ParserTest, ParseSimpleStructCtor) {
  std::string Code = R"(
      const v = Vector2D { x = 1.0, y = 2.0 };
  )";

  auto Parser = makeParser(Code);
  auto ExprAST = Parser->parse(); // parse top-level decls/exprs

  ASSERT_NE(ExprAST, nullptr);
}

TEST_F(ParserTest, ParseNestedStructCtor) {
  std::string Code = R"(
      const rect = Rect { top_left = Point { x = 0, y = 0 },
                          bottom_right = Point { x = 5, y = 5 } };
  )";

  auto Parser = makeParser(Code);
  auto ExprAST = Parser->parse();

  ASSERT_NE(ExprAST, nullptr);
}

TEST_F(ParserTest, ParseEnumVariantCtorWithFields) {
  std::string Code = R"(
      const r = Shape { Rectangle { l = 4, w = 5 } };
  )";

  auto Parser = makeParser(Code);
  auto ExprAST = Parser->parse();

  ASSERT_NE(ExprAST, nullptr);
}

TEST_F(ParserTest, ParseEnumVariantCtorWithValue) {
  std::string Code = R"(
      const c = Shape { Circle = 4 };
  )";

  auto Parser = makeParser(Code);
  auto ExprAST = Parser->parse();

  ASSERT_NE(ExprAST, nullptr);
}

TEST_F(ParserTest, ParseEnumVariantCtorWithTupleLikeValue) {
  std::string Code = R"(
      const t = Shape { Triangle = (1, 2) };
  )";

  auto Parser = makeParser(Code);
  auto ExprAST = Parser->parse();

  ASSERT_NE(ExprAST, nullptr);
}

TEST_F(ParserTest, ParseCustomTypeCtorError_MissingClosingBrace) {
  std::string Code = R"(
      const bad = Shape { Circle = 10;
  )"; // missing '}' should produce a parser error

  auto Parser = makeParser(Code);
  auto ExprAST = Parser->parse();

  EXPECT_EQ(ExprAST, nullptr) << "Expected parse failure due to missing '}'";
}

TEST_F(ParserTest, ParseCustomTypeCtorError_UnexpectedToken) {
  std::string Code = R"(
      const bad = Vector2D { x == 10, y = 5 };
  )"; // '==' instead of '=' should fail

  auto Parser = makeParser(Code);
  auto ExprAST = Parser->parse();

  EXPECT_EQ(ExprAST, nullptr) << "Expected parse failure due to bad syntax";
}

TEST_F(ParserTest, ParseAmbiguousCtor_EnumVsStruct) {
  std::string Code = R"(
      const weird = Shape { x = 10 };
  )"; // neither a valid enum variant nor a struct field

  auto Parser = makeParser(Code);
  auto ExprAST = Parser->parse();

  EXPECT_EQ(ExprAST, nullptr) << "Expected parse failure due to ambiguity";
}
*/
