#pragma once

#include <cstdint>
#include <initializer_list>
#include <memory>
#include <optional>
#include <stack>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "AST/Pattern.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Parser - Recursive descent parser for the Phi programming language
//===----------------------------------------------------------------------===//

class Parser {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  Parser(std::vector<Token> Tokens, DiagnosticManager *DiagnosticManager);

  //===--------------------------------------------------------------------===//
  // Main Entry Point
  //===--------------------------------------------------------------------===//

  std::unique_ptr<ModuleDecl> parse();

private:
  //===--------------------------------------------------------------------===//
  // Parser State
  //===--------------------------------------------------------------------===//
  std::vector<Token> Tokens;
  std::vector<Token>::iterator TokenIt;
  std::vector<std::unique_ptr<ItemDecl>> Ast;
  DiagnosticManager *Diags;

  bool NoAdtInit = false;
  bool FileHasModule = false;
  std::vector<TypeArgDecl *> ValidGenerics;
  std::unordered_map<std::string, Type *> BuiltinTyAliases;

  //===--------------------------------------------------------------------===//
  // Token Navigation Utilities
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool atEOF() const;
  [[nodiscard]] Token peekToken() const;
  [[nodiscard]] Token peekToken(int Offset) const;
  [[nodiscard]] TokenKind peekKind() const;

  std::optional<Token> unconsume();
  Token advanceToken();
  bool matchToken(TokenKind::Kind Kind);
  bool expectToken(TokenKind::Kind Expected, const std::string &Context = "",
                   bool Advance = true);

  //===--------------------------------------------------------------------===//
  // Diagnostic Reporting
  //===--------------------------------------------------------------------===//

  void emitError(Diagnostic &&Diag) { Diags->emit(Diag); }
  void emitWarning(Diagnostic &&Diag) const { Diags->emit(Diag); }
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

  bool syncToTopLvl();
  bool syncToStmt();
  bool syncTo(const TokenKind::Kind TargetToken);
  bool syncTo(const std::initializer_list<TokenKind::Kind> TargetTokens);

  //===--------------------------------------------------------------------===//
  // ADT Parsing
  //===--------------------------------------------------------------------===//

  struct ModulePathInfo {
    std::string PathStr;
    std::vector<std::string> Path;
    SrcSpan Span;
  };
  std::optional<ModulePathInfo> parseModulePath();
  std::optional<Visibility> parseItemVisibility();
  std::optional<Mutability> parseMutability();
  std::optional<Visibility> parseAdtMemberVisibility();

  std::unique_ptr<EnumDecl> parseEnumDecl(Visibility Vis);
  std::unique_ptr<StructDecl> parseStructDecl(Visibility Vis);

  std::unique_ptr<StructDecl> parseAnonymousStruct();
  std::unique_ptr<VariantDecl> parseVariantDecl();
  std::unique_ptr<FieldDecl> parseFieldDecl(uint32_t FieldIndex,
                                            Visibility Vis);
  std::unique_ptr<MethodDecl> parseMethodDecl(std::string ParentName,
                                              Visibility Vis);
  std::unique_ptr<ParamDecl> parseMethodParam(std::string ParentName);
  void desugarStaticMethod(
      std::string ParentName, std::string MethodName,
      const std::vector<std::unique_ptr<TypeArgDecl>> &ParentTypeArgs,
      std::vector<std::unique_ptr<TypeArgDecl>> MethodTypeArgs,
      std::vector<std::unique_ptr<ParamDecl>> Params, TypeRef ReturnTy,
      std::unique_ptr<Block> Body, SrcSpan Span, Visibility Vis);
  std::optional<std::vector<std::unique_ptr<TypeArgDecl>>> parseTypeArgDecls();

  //===--------------------------------------------------------------------===//
  // Type System Parsing
  //===--------------------------------------------------------------------===//

  std::optional<TypeRef> parseType(bool AllowPlaceholder);

  enum class Indirection : uint8_t { Ptr, Ref, None };
  std::pair<Indirection, std::optional<SrcSpan>> parseIndirection();
  // parses stuff like tuples, adts, builtins; does not handle indirections or
  // trailing type args
  std::optional<TypeRef> parseTypeBase(bool AllowPlaceholder);
  std::optional<std::vector<TypeRef>> parseTypeArgList(bool AllowPlaceholder);

  //===--------------------------------------------------------------------===//
  // Function Declaration Parsing
  //===--------------------------------------------------------------------===//

  std::unique_ptr<FunDecl> parseFunDecl(Visibility Vis);
  std::unique_ptr<ParamDecl> parseParamDecl();
  std::optional<TypeRef> parseReturnTy(SrcSpan FunSpan);

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
  std::unique_ptr<ImportStmt> parseImportStmt();
  std::unique_ptr<UseStmt> parseUseStmt();
  std::unique_ptr<Block> parseBlock();

  //===--------------------------------------------------------------------===//
  // Pattern Parsing
  //===--------------------------------------------------------------------===//

  std::optional<std::vector<Pattern>> parsePattern();
  std::optional<Pattern> parseSingularPattern();
  std::optional<Pattern> parseWildcardPattern();
  std::optional<Pattern> parseVariantPattern();
  std::optional<Pattern> parseLiteralPattern();

  //===--------------------------------------------------------------------===//
  // Expression Parsing
  //===--------------------------------------------------------------------===//

  std::unique_ptr<Expr> parseExpr();
  std::unique_ptr<Expr> pratt(int MinBp,
                              const std::vector<TokenKind::Kind> &Terminators);

  std::unique_ptr<Expr> parseNud(const Token &Tok);
  std::unique_ptr<Expr> parsePrefixUnaryOp(const Token &Tok);
  std::unique_ptr<Expr> parsePrimitiveLiteral(const Token &Tok);
  std::unique_ptr<Expr> parseGroupingOrTupleLiteral();

  std::unique_ptr<Expr> parsePostfix(const Token &Op,
                                     std::unique_ptr<Expr> Expr);
  std::unique_ptr<Expr> parseInfix(const Token &Op, std::unique_ptr<Expr> Expr,
                                   int RBp);
  std::unique_ptr<FunCallExpr> parseFunCall(std::unique_ptr<Expr> Callee,
                                            std::vector<TypeRef> TypeArgs);
  std::unique_ptr<AdtInit> parseAdtInit(std::unique_ptr<Expr> Expr,
                                        std::vector<TypeRef> TypeArgs);
  std::unique_ptr<MemberInit> parseMemberInit();
  std::unique_ptr<MatchExpr> parseMatchExpr();

  //===--------------------------------------------------------------------===//
  // Parsing Utilities
  //===--------------------------------------------------------------------===//

  struct TypedBinding {
    SrcSpan Span;
    std::string Name;
    std::optional<TypeRef> Type;
    std::unique_ptr<Expr> Init;
  };

  enum Policy { Forbidden, Optional, Required };
  struct BindingPolicy {
    Policy Type = Parser::Policy::Optional;
    Policy Init = Parser::Policy::Optional;
    bool AllowPlaceholderForType = false;
  };

  std::optional<TypedBinding> parseBinding(const BindingPolicy &Policy);

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
  parseList(const TokenKind::Kind Open, const TokenKind::Kind Close, F Fun,
            const std::string &Context = "list") {
    // Verify opening delimiter
    if (!expectToken(Open)) {
      return std::nullopt;
    }

    // Parse list elements
    std::vector<std::unique_ptr<T>> Content;
    while (!atEOF() && peekKind() != Close) {
      std::unique_ptr<T> Result;
      if constexpr (requires { std::invoke(Fun, this); }) {
        Result = std::invoke(Fun, this);
      } else {
        Result = std::invoke(Fun);
      }

      if (Result) {
        Content.push_back(std::move(Result));
      } else {
        // Recover by syncing to comma or closing delimiter
        syncTo({Close, TokenKind::Comma});
      }

      // Check for closing delimiter before comma
      if (peekKind() == Close) {
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
    if (atEOF() || !expectToken(Close)) {
      return std::nullopt;
    }
    return Content;
  }

  template <typename T, typename F>
  std::optional<std::vector<T>>
  parseValueList(const TokenKind::Kind Open, const TokenKind::Kind Close, F Fun,
                 const std::string &Context = "list") {
    TokenKind Opening = TokenKind(Open);
    TokenKind Closing = TokenKind(Close);

    // Verify opening delimiter
    const Token OpeningToken = peekToken();
    if (OpeningToken.getKind() != Opening) {
      emitExpectedFoundError(Opening.toString(), OpeningToken);
      return std::nullopt;
    }
    advanceToken();

    // Parse list elements
    std::vector<T> Content;
    while (!atEOF() && peekToken().getKind() != Closing) {
      std::optional<T> Res;
      if constexpr (std::is_invocable_v<F, Parser *>) {
        Res = std::invoke(Fun, this);
      } else {
        Res = std::invoke(Fun);
      }

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
      emitUnclosedDelimiterError(OpeningToken, Closing.toString());
      return std::nullopt;
    }

    advanceToken(); // Consume closing delimiter
    return Content;
  }
};

} // namespace phi
