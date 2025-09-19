#pragma once

#include <map>
#include <optional>
#include <string>
#include <vector>

namespace phi {

//===----------------------------------------------------------------------===//
// SrcManager - Source code manager for diagnostics
//===----------------------------------------------------------------------===//

/**
 * @brief Source code manager for diagnostics
 *
 * Caches and provides access to source file contents for
 * error reporting and source context display.
 */
class SrcManager {
public:
  //===--------------------------------------------------------------------===//
  // Source File Management
  //===--------------------------------------------------------------------===//

  /**
   * @brief Registers source file content
   * @param path File path identifier
   * @param content Source code content
   */
  void addSrcFile(const std::string &Path, std::string_view Content);

  //===--------------------------------------------------------------------===//
  // Line Access Methods
  //===--------------------------------------------------------------------===//

  /**
   * @brief Retrieves specific source line
   * @param path File path
   * @param line_num Line number (1-indexed)
   * @return Line content or nullopt if invalid
   */
  [[nodiscard]] std::optional<std::string_view> getLine(const std::string &Path,
                                                        int LineNum) const;

  /**
   * @brief Retrieves source line range
   * @param path File path
   * @param start_line Start line (inclusive)
   * @param end_line End line (inclusive)
   * @return Vector of line contents
   */
  [[nodiscard]] std::vector<std::string_view>
  getLines(const std::string &Path, int StartLine, int EndLine) const;

  //===--------------------------------------------------------------------===//
  // File Statistics
  //===--------------------------------------------------------------------===//

  /**
   * @brief Gets total lines in file
   * @param path File path
   * @return Line count or 0 if unknown
   */
  [[nodiscard]] int getLineCount(const std::string &Path) const;

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  std::map<std::string, std::vector<std::string_view>> SrcFiles;
};

} // namespace phi
