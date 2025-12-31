#pragma once

#include "AST/Nodes/Stmt.hpp"
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

} // namespace PatternAtomics

using Pattern = std::variant<PatternAtomics::Wildcard, PatternAtomics::Literal,
                             PatternAtomics::Variant>;
