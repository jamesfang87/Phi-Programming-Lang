#include "Lexer/TokenKind.hpp"
#include "Parser/Parser.hpp"

#include "AST/Nodes/Expr.hpp"
#include "Parser/PrecedenceTable.hpp"
#include <cassert>
#include <memory>
#include <optional>
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

  // Identifiers and Kws
  case TokenKind::Identifier:
    return std::make_unique<DeclRefExpr>(Tok.getStart(), Tok.getLexeme());
  case TokenKind::ThisKw:
    return std::make_unique<DeclRefExpr>(Tok.getStart(), "this");
  case TokenKind::MatchKw:
    return parseMatchExpr();

  // Grouping: ( expr )
  case TokenKind::OpenParen:
    return parseGroupingOrTupleLiteral();
  case TokenKind::OpenBrace: {
    auto Inits = parseList<MemberInitExpr>(
        TokenKind::OpenBrace, TokenKind::CloseBrace, &Parser::parseMemberInit);

    return (!Inits)
               ? nullptr
               : std::make_unique<CustomTypeCtor>(Tok.getStart(), std::nullopt,
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
  std::vector<TokenKind> Terminators = {TokenKind::Eof, TokenKind::Semicolon,
                                        TokenKind::Comma, TokenKind::CloseParen,
                                        TokenKind::CloseBracket};
  if (!NoStructInit) {
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
      if (peekToken().getKind() == TokenKind::CloseParen) {
        break;
      }

      // Handle comma separator
      if (peekToken().getKind() == TokenKind::Comma) {
        advanceToken();
      } else {
        error("missing comma in tuple list")
            .with_primary_label(peekToken().getSpan(), "expected `,` here")
            .with_help("separate tuple elements with commas")
            .emit(*DiagnosticsMan);
        return nullptr;
      }
    } while (peekToken().getKind() != TokenKind::CloseParen);
    assert(peekToken().getKind() == TokenKind::CloseParen);
    advanceToken(); // Consume the closing parenthesis
    return std::make_unique<TupleLiteral>(Elements[0]->getLocation(),
                                          std::move(Elements));
  }
  default:
    emitUnexpectedTokenError(peekToken());
    break;
  }

  if (peekToken().getKind() != TokenKind::CloseParen) {
    error("missing closing parenthesis")
        .with_primary_label(peekToken().getSpan(), "expected `)` here")
        .with_help("parentheses must be properly matched")
        .emit(*DiagnosticsMan);
    return nullptr;
  }

  advanceToken(); // consume ')'
  return Lhs;
}

std::unique_ptr<Expr> Parser::parsePrefixUnaryOp(const Token &Tok) {
  int RBp = prefixBP(Tok.getKind()).value();
  std::vector<TokenKind> Terminators = {TokenKind::Eof, TokenKind::Semicolon,
                                        TokenKind::Comma, TokenKind::CloseParen,
                                        TokenKind::CloseBracket};
  if (!NoStructInit) {
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
    return nullptr;
  }
}

} // namespace phi
