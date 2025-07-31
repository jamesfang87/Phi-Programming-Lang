#pragma once

#include <string>

namespace phi {

/**
 * @brief Represents a source code location
 *
 * Contains file path and line/column position information for
 * error reporting and debugging.
 */
struct SrcLocation {
    std::string path; ///< Source file path
    int line, col;    ///< Line and column numbers (1-indexed)
};

} // namespace phi
