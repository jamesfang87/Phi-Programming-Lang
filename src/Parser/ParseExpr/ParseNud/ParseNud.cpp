#include "Parser/Parser.hpp"

#include "AST/Nodes/Expr.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"
#include "Parser/PrecedenceTable.hpp"
#include "SrcManager/SrcLocation.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <vector>

namespace phi {

std::unique_ptr<Expr> Parser::parseNud(const Token &Tok) {
  if (Tok.getKind() != TokenKind::OpenBrace) {
    advanceToken();
  }

  switch (Tok.getKind()) {
  // Prefix operators: -a, !b, ++c
  case TokenKind::Minus:       // -
  case TokenKind::Bang:        // !
  case TokenKind::DoublePlus:  // ++
  case TokenKind::DoubleMinus: // --
  case TokenKind::Amp:
  case TokenKind::Star:
    return parsePrefixUnaryOp(Tok);

  // Intrinsics
  case TokenKind::Panic: {
    if (!expectToken(TokenKind::OpenParen, "'(' after panic"))
      return nullptr;

    auto Msg = parseExpr();
    if (!Msg)
      return nullptr;

    if (!expectToken(TokenKind::CloseParen, "')' after panic argument"))
      return nullptr;

    return IntrinsicCall::CreatePanic(Tok.getStart(), std::move(Msg));
  }
  case TokenKind::Assert: {
    if (!expectToken(TokenKind::OpenParen, "'(' after assert"))
      return nullptr;

    // Required condition
    auto Cond = parseExpr();
    if (!Cond)
      return nullptr;

    std::unique_ptr<Expr> Msg = nullptr;

    // Optional message
    if (matchToken(TokenKind::Comma)) {
      Msg = parseExpr();
      if (!Msg)
        return nullptr;
    }

    if (!expectToken(TokenKind::CloseParen, "')' after assert"))
      return nullptr;

    return IntrinsicCall::CreateAssert(Tok.getStart(), std::move(Cond),
                                       std::move(Msg));
  }
  case TokenKind::Unreachable: {
    return IntrinsicCall::CreateUnreachable(Tok.getStart());
  }
  case TokenKind::TypeOf: {
    if (!expectToken(TokenKind::OpenParen, "'(' after typeof"))
      return nullptr;

    auto Operand = parseExpr();
    if (!Operand)
      return nullptr;

    if (!expectToken(TokenKind::CloseParen, "')' after typeof operand"))
      return nullptr;

    return IntrinsicCall::CreateTypeOf(Tok.getStart(), std::move(Operand));
  }

  // Identifiers and Kws
  case TokenKind::Identifier: {
    SrcLocation Location = Tok.getStart();
    std::string QualId = Tok.getLexeme();
    while (peekKind() == TokenKind::DoubleColon) {
      // beginning of turbofish operator; we cannot advance the token
      if (peekToken(1).getKind() == TokenKind::OpenCaret) {
        break;
      }

      // it is fine to advance the token
      advanceToken();
      QualId += "::";

      // otherwise, we expect an Identifier
      if (!expectToken(TokenKind(TokenKind::Identifier), "Module path",
                       false)) {
        return nullptr;
      }
      QualId += advanceToken().getLexeme();
    }

    return std::make_unique<DeclRefExpr>(Location, QualId);
  }
  case TokenKind::ThisKw:
    return std::make_unique<DeclRefExpr>(Tok.getStart(), "this");
  case TokenKind::MatchKw:
    return parseMatchExpr();

  // Grouping: ( expr )
  case TokenKind::OpenParen:
    return parseGroupingOrTupleLiteral();
  case TokenKind::OpenBrace: {
    // Case of anonymous AdtInit
    auto Inits = parseList<MemberInit>(
        TokenKind::OpenBrace, TokenKind::CloseBrace, &Parser::parseMemberInit);

    return (!Inits) ? nullptr
                    : std::make_unique<AdtInit>(Tok.getStart(), std::nullopt,
                                                std::nullopt,
                                                std::move(Inits.value()));
  }
  // Literals
  default:
    return parsePrimitiveLiteral(Tok);
  }
  return nullptr;
}

std::unique_ptr<Expr> Parser::parseGroupingOrTupleLiteral() {
  std::unique_ptr<Expr> Lhs;
  std::vector<TokenKind::Kind> Terminators = {
      TokenKind::Eof, TokenKind::Semicolon, TokenKind::Comma,
      TokenKind::CloseParen, TokenKind::CloseBracket};
  if (!NoAdtInit) {
    Terminators.push_back(TokenKind::OpenBrace);
  }

  auto Res = pratt(0, Terminators);
  if (!Res) {
    return nullptr;
  }
  Lhs = std::move(Res);

  switch (peekToken().getKind()) {
  case TokenKind::CloseParen:
    advanceToken(); // treat as grouping expr
    return Lhs;
  case TokenKind::Comma: {
    advanceToken();
    std::vector<std::unique_ptr<Expr>> Elements;
    Elements.push_back(std::move(Lhs));
    do {
      auto Element = pratt(0, Terminators);
      if (!Element) {
        return nullptr;
      }
      Elements.push_back(std::move(Element));

      // Check for closing delimiter before comma
      if (matchToken(TokenKind::CloseParen)) {
        break;
      }

      // Handle comma separator
      if (!expectToken(TokenKind::Comma, "tuple list")) {
        return nullptr;
      }
    } while (!matchToken(TokenKind::CloseParen));
    return std::make_unique<TupleLiteral>(Elements[0]->getLocation(),
                                          std::move(Elements));
  }
  default:
    emitUnexpectedTokenError(peekToken());
    break;
  }

  if (!matchToken(TokenKind::CloseParen)) {
    error("missing closing parenthesis")
        .with_primary_label(peekToken().getSpan(), "expected `)` here")
        .with_help("parentheses must be properly matched")
        .emit(*Diags);
    return nullptr;
  }

  return Lhs;
}

std::unique_ptr<Expr> Parser::parsePrefixUnaryOp(const Token &Tok) {
  int RBp = prefixBP(Tok.getKind()).value();
  std::vector<TokenKind::Kind> Terminators = {
      TokenKind::Eof, TokenKind::Semicolon, TokenKind::Comma,
      TokenKind::CloseParen, TokenKind::CloseBracket};
  if (!NoAdtInit) {
    Terminators.push_back(TokenKind::OpenBrace);
  }

  auto Rhs = pratt(RBp, Terminators);
  if (!Rhs)
    return nullptr;

  return std::make_unique<UnaryOp>(std::move(Rhs), Tok, true); // prefix
}

std::unique_ptr<Expr> Parser::parsePrimitiveLiteral(const Token &Tok) {
  switch (Tok.getKind()) {
  case TokenKind::IntLiteral:
    return std::make_unique<IntLiteral>(Tok.getStart(),
                                        std::stoll(Tok.getLexeme()));
  case TokenKind::FloatLiteral:
    return std::make_unique<FloatLiteral>(Tok.getStart(),
                                          std::stod(Tok.getLexeme()));
  case TokenKind::StrLiteral:
    return std::make_unique<StrLiteral>(Tok.getStart(), Tok.getLexeme());
  case TokenKind::CharLiteral:
    return std::make_unique<CharLiteral>(Tok.getStart(), Tok.getLexeme()[0]);
  case TokenKind::TrueKw:
    return std::make_unique<BoolLiteral>(Tok.getStart(), true);
  case TokenKind::FalseKw:
    return std::make_unique<BoolLiteral>(Tok.getStart(), false);
  default:
    error("unexpected token; could not match this token to a literal")
        .with_primary_label(Tok.getSpan())
        .emit(*Diags);
    return nullptr;
  }
}

} // namespace phi
