#include "Parser/Parser.hpp"

#include "AST/Expr.hpp"
#include "Parser/PrecedenceTable.hpp"

namespace phi {

std::unique_ptr<Expr> Parser::parseNud(const Token &Tok) {
  switch (Tok.getKind()) {
  // Prefix operators: -a, !b, ++c
  case TokenKind::MinusKind:       // -
  case TokenKind::BangKind:        // !
  case TokenKind::DoublePlusKind:  // ++
  case TokenKind::DoubleMinusKind: // --
  case TokenKind::AmpKind:
  case TokenKind::StarKind:
    return parsePrefixUnaryOp(Tok);

  // Identifiers
  case TokenKind::IdentifierKind:
    return std::make_unique<DeclRefExpr>(Tok.getStart(), Tok.getLexeme());

  // Grouping: ( expr )
  case TokenKind::OpenParenKind:
    return parseGroupingExpr();

  // Literals
  default:
    return parseLiteralExpr(Tok);
  }
  return nullptr;
}

std::unique_ptr<Expr> Parser::parseGroupingExpr() {
  std::unique_ptr<Expr> Lhs;
  std::vector<TokenKind> Terminators = {
      TokenKind::EOFKind, TokenKind::SemicolonKind, TokenKind::CommaKind,
      TokenKind::CloseParenKind, TokenKind::CloseBracketKind};
  if (!NoStructInit) {
    Terminators.push_back(TokenKind::OpenBraceKind);
  }
  auto Res = pratt(0, Terminators);
  if (!Res)
    return nullptr;
  Lhs = std::move(Res);

  if (peekToken().getKind() != TokenKind::CloseParenKind) {
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
  std::vector<TokenKind> Terminators = {
      TokenKind::EOFKind, TokenKind::SemicolonKind, TokenKind::CommaKind,
      TokenKind::CloseParenKind, TokenKind::CloseBracketKind};
  if (!NoStructInit) {
    Terminators.push_back(TokenKind::OpenBraceKind);
  }

  auto rhs = pratt(R, Terminators);
  if (!rhs)
    return nullptr;

  return std::make_unique<UnaryOp>(std::move(rhs), Tok, true); // prefix
}

std::unique_ptr<Expr> Parser::parseLiteralExpr(const Token &Tok) {
  switch (Tok.getKind()) {
  case TokenKind::IntLiteralKind:
    return std::make_unique<IntLiteral>(Tok.getStart(),
                                        std::stoll(Tok.getLexeme()));
  case TokenKind::FloatLiteralKind:
    return std::make_unique<FloatLiteral>(Tok.getStart(),
                                          std::stod(Tok.getLexeme()));
  case TokenKind::StrLiteralKind:
    return std::make_unique<StrLiteral>(Tok.getStart(), Tok.getLexeme());
  case TokenKind::CharLiteralKind:
    return std::make_unique<CharLiteral>(Tok.getStart(), Tok.getLexeme()[0]);
  case TokenKind::TrueKwKind:
    return std::make_unique<BoolLiteral>(Tok.getStart(), true);
  case TokenKind::FalseKwKind:
    return std::make_unique<BoolLiteral>(Tok.getStart(), false);
  default:
    return nullptr;
  }
}

} // namespace phi
