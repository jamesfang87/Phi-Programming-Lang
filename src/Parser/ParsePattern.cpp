#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <string>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "AST/Pattern.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {
using std::optional;

optional<std::vector<Pattern>> Parser::parsePattern() {
  std::vector<Pattern> Patterns;

  // Parse the first singular pattern
  auto First = parseSingularPattern();
  if (!First)
    return std::nullopt;

  Patterns.push_back(std::move(*First));

  // Handle alternation: p1 | p2 | p3 ...
  while (peekKind() == TokenKind::Pipe) {
    advanceToken(); // consume '|'

    auto Next = parseSingularPattern();
    if (!Next)
      return std::nullopt;

    Patterns.push_back(std::move(*Next));
  }

  return Patterns;
}

optional<Pattern> Parser::parseSingularPattern() {
  switch (peekKind()) {
  case TokenKind::Wildcard:
    return parseWildcardPattern();
  case TokenKind::Period:
    return parseVariantPattern();
  default:
    return parseLiteralPattern();
  }
}

optional<Pattern> Parser::parseWildcardPattern() {
  assert(peekKind() == TokenKind::Wildcard);
  advanceToken();

  std::optional<Pattern> Result;
  Result.emplace(std::in_place_type<PatternAtomics::Wildcard>);
  return Result;
}

optional<Pattern> Parser::parseLiteralPattern() {
  auto Tok = advanceToken();

  std::optional<Pattern> Result;

  switch (Tok.getKind()) {
  case TokenKind::IntLiteral:
    Result.emplace(std::in_place_type<PatternAtomics::Literal>,
                   std::make_unique<IntLiteral>(Tok.getStart(),
                                                std::stoll(Tok.getLexeme())));
    break;
  case TokenKind::FloatLiteral:
    Result.emplace(std::in_place_type<PatternAtomics::Literal>,
                   std::make_unique<FloatLiteral>(Tok.getStart(),
                                                  std::stod(Tok.getLexeme())));
    break;
  case TokenKind::StrLiteral:
    Result.emplace(
        std::in_place_type<PatternAtomics::Literal>,
        std::make_unique<StrLiteral>(Tok.getStart(), Tok.getLexeme()));
    break;
  case TokenKind::CharLiteral:
    Result.emplace(
        std::in_place_type<PatternAtomics::Literal>,
        std::make_unique<CharLiteral>(Tok.getStart(), Tok.getLexeme()[0]));
    break;
  case TokenKind::TrueKw:
    Result.emplace(std::in_place_type<PatternAtomics::Literal>,
                   std::make_unique<BoolLiteral>(Tok.getStart(), true));
    break;
  case TokenKind::FalseKw:
    Result.emplace(std::in_place_type<PatternAtomics::Literal>,
                   std::make_unique<BoolLiteral>(Tok.getStart(), false));
    break;
  default:
    return std::nullopt;
  }

  return Result;
}

optional<Pattern> Parser::parseVariantPattern() {
  assert(advanceToken().getKind() == TokenKind::Period);

  if (!expectToken(TokenKind::Identifier, "variant pattern", false)) {
    return std::nullopt;
  }
  const SrcLocation Loc = peekToken().getStart();
  const std::string Name = advanceToken().getLexeme();

  std::optional<std::vector<std::unique_ptr<VarDecl>>> Vars;
  if (peekKind() != TokenKind::OpenParen) {
    return PatternAtomics::Variant(Name, {}, Loc);
  }

  Vars = parseList<VarDecl>(
      TokenKind::OpenParen, TokenKind::CloseParen,
      [&] -> std::unique_ptr<VarDecl> {
        if (expectToken(TokenKind::Identifier, "", false)) {
          return std::make_unique<VarDecl>(
              peekToken().getSpan(), Mutability::Var,
              advanceToken().getLexeme(), std::nullopt, nullptr);
        }
        return nullptr;
      });

  if (!Vars)
    return std::nullopt;

  return PatternAtomics::Variant(Name, std::move(*Vars), Loc);
}

} // namespace phi
