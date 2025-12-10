#pragma once

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "SrcManager/SrcLocation.hpp"

#include <memory>
#include <variant>
#include <vector>

namespace PatternAtomics {

struct Wildcard {};

struct Literal {
  std::unique_ptr<phi::Expr> Value;
};

struct Variant {
  std::string VariantName;
  std::vector<std::unique_ptr<phi::VarDecl>> Vars;
  phi::SrcLocation Location;
};

using SingularPattern =
    std::variant<PatternAtomics::Wildcard, PatternAtomics::Literal,
                 PatternAtomics::Variant>;

struct Alternation {
  std::vector<SingularPattern> Patterns;
};

} // namespace PatternAtomics

using Pattern =
    std::variant<PatternAtomics::Wildcard, PatternAtomics::Literal,
                 PatternAtomics::Variant, PatternAtomics::Alternation>;
