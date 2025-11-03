#pragma once

#include <format>
#include <string>

namespace phi {

/**
 * @brief Represents a source code location
 *
 * Contains file path and line/column position information for
 * error reporting and debugging.
 */
struct SrcLocation {
  std::string Path; ///< Source file path
  int Line, Col;    ///< Line and column numbers (1-indexed)
                    ///
  std::string toString() const { return std::format("{}:{}", Line, Col); }
};

} // namespace phi
