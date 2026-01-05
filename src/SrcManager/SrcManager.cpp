#include "SrcManager/SrcManager.hpp"

namespace phi {

/**
 * Adds a source file to the source manager.
 *
 * @param path File path (used as unique identifier)
 * @param content Source code content
 *
 * Preprocesses the source by splitting it into lines for efficient access.
 */
void SrcManager::addSrcFile(const std::string &Path,
                            const std::string_view Content) {
  std::vector<std::string_view> Lines;
  auto It = Content.begin();
  while (It < Content.end()) {
    auto LineStart = It;
    while (It < Content.end() && *It != '\n') {
      It++;
    }
    Lines.emplace_back(LineStart, It);
    if (It < Content.end()) {
      It++; // skip '\n'
    }
  }
  SrcFiles[Path] = std::move(Lines);
}

/**
 * Retrieves a single line from a source file.
 *
 * @param path File path
 * @param line_num Line number (1-indexed)
 * @return Line content if available, nullopt otherwise
 *
 * Performs bounds checking to ensure valid line numbers.
 */
std::optional<std::string_view> SrcManager::getLine(const std::string &Path,
                                                    const int LineNum) const {
  const auto FileIt = SrcFiles.find(Path);
  if (FileIt == SrcFiles.end()) {
    return std::nullopt;
  }

  const auto &Lines = FileIt->second;
  if (LineNum < 1 || LineNum > static_cast<int>(Lines.size())) {
    return std::nullopt;
  }

  return Lines[LineNum - 1];
}

/**
 * Retrieves a range of lines from a source file.
 *
 * @param path File path
 * @param start_line Starting line (inclusive)
 * @param end_line Ending line (inclusive)
 * @return Vector of line contents
 *
 * Automatically handles invalid ranges by returning available lines.
 */
std::vector<std::string_view> SrcManager::getLines(const std::string &Path,
                                                   const int startLine,
                                                   const int EndLine) const {
  std::vector<std::string_view> Result;
  for (int i = startLine; i <= EndLine; ++i) {
    if (auto Line = getLine(Path, i)) {
      Result.push_back(*Line);
    }
  }
  return Result;
}

/**
 * Gets the total number of lines in a source file.
 *
 * @param path File path
 * @return Line count, or 0 if file not found
 */
int SrcManager::getLineCount(const std::string &Path) const {
  const auto FileIt = SrcFiles.find(Path);
  if (FileIt == SrcFiles.end()) {
    return 0;
  }
  return static_cast<int>(FileIt->second.size());
}

} // namespace phi
