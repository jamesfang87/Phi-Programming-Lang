#pragma once

#include <cstdint>
#include <initializer_list>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "AST/Pattern.hpp"
#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Parser - Recursive descent parser for the Phi programming language
//===----------------------------------------------------------------------===//

class Parser {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  Parser(const std::string_view Src, const std::string_view Path,
         std::vector<Token> Tokens,
         std::shared_ptr<DiagnosticManager> DiagnosticManager);

  //===--------------------------------------------------------------------===//
  // Main Entry Point
  //===--------------------------------------------------------------------===//

  std::vector<std::unique_ptr<Decl>> parse();

private:
  //===--------------------------------------------------------------------===//
  // Parser State
  //===--------------------------------------------------------------------===//

  std::string Path;
  std::vector<std::string_view> Lines;
  std::vector<Token> Tokens;
  std::vector<Token>::iterator TokenIt;
  std::vector<std::unique_ptr<Decl>> Ast;
  std::shared_ptr<DiagnosticManager> DiagnosticsMan;

  bool NoStructInit = false;

  //===--------------------------------------------------------------------===//
  // Token Navigation Utilities
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool atEOF() const;
  [[nodiscard]] Token peekToken() const;
  [[nodiscard]] Token peekToken(int Offset) const;
  [[nodiscard]] TokenKind peekKind() const;

  Token advanceToken();
  bool matchToken(TokenKind Kind);
  bool expectToken(TokenKind Expected, const std::string &Context = "");

  //===--------------------------------------------------------------------===//
  // Diagnostic Reporting
  //===--------------------------------------------------------------------===//

  void emitError(Diagnostic &&Diag) { DiagnosticsMan->emit(Diag); }
  void emitWarning(Diagnostic &&Diag) const { DiagnosticsMan->emit(Diag); }
  void emitExpectedFoundError(const std::string &Expected,
                              const Token &FoundToken);
  void
  emitUnexpectedTokenError(const Token &Token,
                           const std::vector<std::string> &ExpectedTokens = {});
  void emitUnclosedDelimiterError(const Token &OpeningToken,
                                  const std::string &ExpectedClosing);

  //===--------------------------------------------------------------------===//
  // Error Recovery
  //===--------------------------------------------------------------------===//

  bool SyncToTopLvl();
  bool SyncToStmt();
  bool syncTo(const std::initializer_list<TokenKind> TargetTokens);
  bool syncTo(const TokenKind TargetToken);

  //===--------------------------------------------------------------------===//
  // Enum Parsing
  //===--------------------------------------------------------------------===//

  std::optional<VariantDecl> parseVariantDecl();
  std::unique_ptr<EnumDecl> parseEnumDecl();

  //===--------------------------------------------------------------------===//
  // Struct Parsing
  //===--------------------------------------------------------------------===//

  std::unique_ptr<StructDecl> parseStructDecl();
  std::optional<FieldDecl> parseFieldDecl(uint32_t FieldIndex);
  std::optional<MethodDecl> parseMethodDecl(std::string ParentName);

  //===--------------------------------------------------------------------===//
  // Type System Parsing
  //===--------------------------------------------------------------------===//

  std::optional<TypeRef> parseType();

  //===--------------------------------------------------------------------===//
  // Function Declaration Parsing
  //===--------------------------------------------------------------------===//

  std::unique_ptr<FunDecl> parseFunDecl();
  std::unique_ptr<ParamDecl> parseParamDecl();
  std::unique_ptr<Block> parseBlock();

  //===--------------------------------------------------------------------===//
  // Statement Parsing
  //===--------------------------------------------------------------------===//

  std::unique_ptr<Stmt> parseStmt();
  std::unique_ptr<ReturnStmt> parseReturnStmt();
  std::unique_ptr<DeferStmt> parseDeferStmt();
  std::unique_ptr<IfStmt> parseIfStmt();
  std::unique_ptr<WhileStmt> parseWhileStmt();
  std::unique_ptr<ForStmt> parseForStmt();
  std::unique_ptr<DeclStmt> parseDeclStmt();
  std::unique_ptr<BreakStmt> parseBreakStmt();
  std::unique_ptr<ContinueStmt> parseContinueStmt();

  //===--------------------------------------------------------------------===//
  // Pattern Parsing
  //===--------------------------------------------------------------------===//

  std::optional<std::vector<Pattern>> parsePattern();
  std::optional<Pattern> parseSingularPattern();
  std::optional<PatternAtomics::Wildcard> parseWildcardPattern();
  std::optional<PatternAtomics::Variant> parseVariantPattern();
  std::optional<PatternAtomics::Literal> parseLiteralPattern();

  //===--------------------------------------------------------------------===//
  // Expression Parsing
  //===--------------------------------------------------------------------===//

  std::unique_ptr<Expr> parseExpr();
  std::unique_ptr<Expr> pratt(int MinBp,
                              const std::vector<TokenKind> &Terminators);

  std::unique_ptr<Expr> parseNud(const Token &Tok);
  std::unique_ptr<Expr> parsePrefixUnaryOp(const Token &Tok);
  std::unique_ptr<Expr> parsePrimitiveLiteral(const Token &Tok);
  std::unique_ptr<Expr> parseGroupingOrTupleLiteral();

  std::unique_ptr<Expr> parsePostfix(const Token &Op,
                                     std::unique_ptr<Expr> Expr);
  std::unique_ptr<Expr> parseInfix(const Token &Op, std::unique_ptr<Expr> Expr,
                                   int RBp);
  std::unique_ptr<FunCallExpr> parseFunCall(std::unique_ptr<Expr> Callee);
  std::unique_ptr<CustomTypeCtor> parseCustomInit(std::unique_ptr<Expr> Expr);
  std::unique_ptr<MemberInitExpr> parseMemberInit();
  std::unique_ptr<MatchExpr> parseMatchExpr();

  //===--------------------------------------------------------------------===//
  // Parsing Utilities
  //===--------------------------------------------------------------------===//

  struct TypedBinding {
    SrcLocation Loc;
    std::string Name;
    std::optional<TypeRef> Type;
  };
  std::optional<TypedBinding> parseTypedBinding();

  /**
   * @brief Generic list parsing template
   *
   * Parses comma-separated lists enclosed by delimiters (e.g., parameters,
   * arguments)
   *
   * @tparam T Type of elements in the list
   * @tparam F Parser method type for individual elements
   * @param opening Opening delimiter token type
   * @param closing Closing delimiter token type
   * @param fun Member function pointer to element parser
   * @param context Description of list context for error messages
   * @return std::vector<std::unique_ptr<T>>
   *         Vector of parsed elements (empty on failure)
   *         Errors are emitted to DiagnosticManager
   */
  template <typename T, typename F>
  std::optional<std::vector<std::unique_ptr<T>>>
  parseList(const TokenKind Opening, const TokenKind Closing, F Fun,
            const std::string &Context = "list") {
    // Verify opening delimiter
    const Token OpeningToken = peekToken();
    if (OpeningToken.getKind() != Opening) {
      emitExpectedFoundError(TokenKindToStr(Opening), OpeningToken);
      return std::nullopt;
    }
    advanceToken();

    // Parse list elements
    std::vector<std::unique_ptr<T>> Content;
    while (!atEOF() && peekToken().getKind() != Closing) {
      auto Result = std::invoke(Fun, this);
      if (Result) {
        Content.push_back(std::move(Result));
      } else {
        // Recover by syncing to comma or closing delimiter
        syncTo({Closing, TokenKind::Comma});
      }

      // Check for closing delimiter before comma
      if (peekToken().getKind() == Closing) {
        break;
      }

      // Handle comma separator
      if (peekToken().getKind() == TokenKind::Comma) {
        advanceToken();
      } else {
        emitError(
            error("missing comma in " + Context)
                .with_primary_label(peekToken().getSpan(), "expected `,` here")
                .with_help("separate " + Context + " elements with commas")
                .build());
        return std::nullopt;
      }
    }

    // Verify closing delimiter
    if (atEOF() || peekToken().getKind() != Closing) {
      emitUnclosedDelimiterError(OpeningToken, TokenKindToStr(Closing));
      return std::nullopt;
    }

    advanceToken(); // Consume closing delimiter
    return Content;
  }

  template <typename T, typename F>
  std::optional<std::vector<T>>
  parseValueList(const TokenKind Opening, const TokenKind Closing, F Fun,
                 const std::string &Context = "list") {
    // Verify opening delimiter
    const Token OpeningToken = peekToken();
    if (OpeningToken.getKind() != Opening) {
      emitExpectedFoundError(TokenKindToStr(Opening), OpeningToken);
      return std::nullopt;
    }
    advanceToken();

    // Parse list elements
    std::vector<T> Content;
    while (!atEOF() && peekToken().getKind() != Closing) {
      auto Res = std::invoke(Fun, this);
      if (Res) {
        Content.push_back(std::move(*Res));
      } else {
        // Recover by syncing to comma or closing delimiter
        syncTo({Closing, TokenKind::Comma});
      }

      // Check for closing delimiter before comma
      if (peekKind() == Closing) {
        break;
      }

      // Handle comma separator
      if (peekKind() == TokenKind::Comma) {
        advanceToken();
      } else {
        emitError(
            error("missing comma in " + Context)
                .with_primary_label(peekToken().getSpan(), "expected `,` here")
                .with_help("separate " + Context + " elements with commas")
                .build());
        return std::nullopt;
      }
    }

    // Verify closing delimiter
    if (atEOF() || peekKind() != Closing) {
      emitUnclosedDelimiterError(OpeningToken, TokenKindToStr(Closing));
      return std::nullopt;
    }

    advanceToken(); // Consume closing delimiter
    return Content;
  }

  template <typename T, typename F>
  std::optional<std::vector<T>>
  parsePtrList(const TokenKind Opening, const TokenKind Closing, F Fun,
               const std::string &Context = "list") {
    // Verify opening delimiter
    const Token OpeningToken = peekToken();
    if (OpeningToken.getKind() != Opening) {
      emitExpectedFoundError(TokenKindToStr(Opening), OpeningToken);
      return std::nullopt;
    }
    advanceToken();

    // Parse list elements
    std::vector<T> Content;
    while (!atEOF() && peekToken().getKind() != Closing) {
      auto Res = std::invoke(Fun, this);
      if (Res) {
        Content.push_back(Res);
      } else {
        // Recover by syncing to comma or closing delimiter
        syncTo({Closing, TokenKind::Comma});
      }

      // Check for closing delimiter before comma
      if (peekKind() == Closing) {
        break;
      }

      // Handle comma separator
      if (peekKind() == TokenKind::Comma) {
        advanceToken();
      } else {
        emitError(
            error("missing comma in " + Context)
                .with_primary_label(peekToken().getSpan(), "expected `,` here")
                .with_help("separate " + Context + " elements with commas")
                .build());
        return std::nullopt;
      }
    }

    // Verify closing delimiter
    if (atEOF() || peekKind() != Closing) {
      emitUnclosedDelimiterError(OpeningToken, TokenKindToStr(Closing));
      return std::nullopt;
    }

    advanceToken(); // Consume closing delimiter
    return Content;
  }
};

} // namespace phi
