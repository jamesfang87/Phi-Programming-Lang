#include "AST/Decl.hpp"
#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <string>

#include "AST/Pattern.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {
using std::optional;

optional<Pattern> Parser::parsePattern() {
  std::vector<PatternAtomics::SingularPattern> Patterns;

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

  if (Patterns.size() == 1) {
    // Extract the contained type from SingularPattern and construct Pattern
    return std::visit(
        [](auto &&Arg) -> Pattern {
          return Pattern(std::forward<decltype(Arg)>(Arg));
        },
        std::move(Patterns[0]));
  }
  return Pattern(PatternAtomics::Alternation{std::move(Patterns)});
}

optional<PatternAtomics::SingularPattern> Parser::parseSingularPattern() {
  switch (peekKind()) {
  case TokenKind::Wildcard:
    return parseWildcardPattern();
  case TokenKind::Period:
    return parseVariantPattern();
  default:
    return parseLiteralPattern();
  }
}

optional<PatternAtomics::Wildcard> Parser::parseWildcardPattern() {
  assert(peekKind() == TokenKind::Wildcard);
  advanceToken();

  return PatternAtomics::Wildcard();
}

optional<PatternAtomics::Literal> Parser::parseLiteralPattern() {
  auto Tok = advanceToken();
  switch (Tok.getKind()) {
  case TokenKind::IntLiteral:
    return PatternAtomics::Literal(std::make_unique<IntLiteral>(
        Tok.getStart(), std::stoll(Tok.getLexeme())));
  case TokenKind::FloatLiteral:
    return PatternAtomics::Literal(std::make_unique<FloatLiteral>(
        Tok.getStart(), std::stod(Tok.getLexeme())));
  case TokenKind::StrLiteral:
    return PatternAtomics::Literal(
        std::make_unique<StrLiteral>(Tok.getStart(), Tok.getLexeme()));
  case TokenKind::CharLiteral:
    return PatternAtomics::Literal(
        std::make_unique<CharLiteral>(Tok.getStart(), Tok.getLexeme()[0]));
  case TokenKind::TrueKw:
    return PatternAtomics::Literal(
        std::make_unique<BoolLiteral>(Tok.getStart(), true));
  case TokenKind::FalseKw:
    return PatternAtomics::Literal(
        std::make_unique<BoolLiteral>(Tok.getStart(), false));
  default:
    return std::nullopt;
  }
}

optional<PatternAtomics::Variant> Parser::parseVariantPattern() {
  assert(peekKind() == TokenKind::Period);
  advanceToken();

  if (peekKind() != TokenKind::Identifier) {
    emitUnexpectedTokenError(peekToken());
    return std::nullopt;
  }
  const std::string Name = peekToken().getLexeme();
  const SrcLocation Loc = advanceToken().getStart();

  // parse destructing variables
  if (peekKind() == TokenKind::OpenParen) {
    bool ErrorHappened = false;
    auto Vars = parseList<VarDecl>(
        TokenKind::OpenParen, TokenKind::CloseParen,
        [&](Parser *P) -> std::unique_ptr<VarDecl> {
          if (P->peekKind() == TokenKind::Identifier) {
            return std::make_unique<VarDecl>(P->peekToken().getStart(),
                                             P->advanceToken().getLexeme(),
                                             std::nullopt, false, nullptr);
          }
          P->emitUnexpectedTokenError(P->peekToken());
          ErrorHappened = true;
          return nullptr;
        });

    if (ErrorHappened || !Vars)
      return std::nullopt;

    return PatternAtomics::Variant(Name, std::move(*Vars), Loc);
  }

  return PatternAtomics::Variant(Name, {}, Loc);
}

} // namespace phi
