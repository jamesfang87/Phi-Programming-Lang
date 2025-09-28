#include "Parser/Parser.hpp"

#include "AST/Expr.hpp"
#include "Parser/PrecedenceTable.hpp"

namespace phi {

std::unique_ptr<Expr> Parser::parseNud(const Token &Tok) {
  switch (Tok.getKind()) {
  // Prefix operators: -a, !b, ++c
  case TokenKind::Minus:       // -
  case TokenKind::Bang:        // !
  case TokenKind::DoublePlus:  // ++
  case TokenKind::DoubleMinus: // --
  case TokenKind::Amp:
  case TokenKind::Star:
    return parsePrefixUnaryOp(Tok);

  // Identifiers
  case TokenKind::Identifier:
    return std::make_unique<DeclRefExpr>(Tok.getStart(), Tok.getLexeme());
  case TokenKind::ThisKw:
    return std::make_unique<DeclRefExpr>(Tok.getStart(), "this");

  // Grouping: ( expr )
  case TokenKind::OpenParen:
    return parseGroupingExpr();

  // Literals
  default:
    return parseLiteralExpr(Tok);
  }
  return nullptr;
}

std::unique_ptr<Expr> Parser::parseGroupingExpr() {
  std::unique_ptr<Expr> Lhs;
  std::vector<TokenKind> Terminators = {TokenKind::Eof, TokenKind::Semicolon,
                                        TokenKind::Comma, TokenKind::CloseParen,
                                        TokenKind::CloseBracket};
  if (!NoStructInit) {
    Terminators.push_back(TokenKind::OpenBrace);
  }
  auto Res = pratt(0, Terminators);
  if (!Res)
    return nullptr;
  Lhs = std::move(Res);

  if (peekToken().getKind() != TokenKind::CloseParen) {
    error("missing closing parenthesis")
        .with_primary_label(spanFromToken(peekToken()), "expected `)` here")
        .with_help("parentheses must be properly matched")
        .emit(*DiagnosticsMan);
    return nullptr;
  }

  advanceToken(); // consume ')'
  return Lhs;
}

std::unique_ptr<Expr> Parser::parsePrefixUnaryOp(const Token &Tok) {
  int R = prefixBP(Tok.getKind()).value();
  std::vector<TokenKind> Terminators = {TokenKind::Eof, TokenKind::Semicolon,
                                        TokenKind::Comma, TokenKind::CloseParen,
                                        TokenKind::CloseBracket};
  if (!NoStructInit) {
    Terminators.push_back(TokenKind::OpenBrace);
  }

  auto rhs = pratt(R, Terminators);
  if (!rhs)
    return nullptr;

  return std::make_unique<UnaryOp>(std::move(rhs), Tok, true); // prefix
}

std::unique_ptr<Expr> Parser::parseLiteralExpr(const Token &Tok) {
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
    return nullptr;
  }
}

} // namespace phi
