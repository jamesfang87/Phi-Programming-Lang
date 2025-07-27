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
void SrcManager::add_source_file(const std::string& path, const std::string_view content) {
    std::vector<std::string_view> lines;
    auto it = content.begin();
    while (it < content.end()) {
        auto line_start = it;
        while (it < content.end() && *it != '\n') {
            it++;
        }
        lines.emplace_back(line_start, it);
        if (it < content.end()) {
            it++; // skip '\n'
        }
    }
    source_files[path] = std::move(lines);
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
std::optional<std::string_view> SrcManager::get_line(const std::string& path,
                                                     const int line_num) const {
    const auto file_it = source_files.find(path);
    if (file_it == source_files.end()) {
        return std::nullopt;
    }

    const auto& lines = file_it->second;
    if (line_num < 1 || line_num > static_cast<int>(lines.size())) {
        return std::nullopt;
    }

    return lines[line_num - 1];
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
std::vector<std::string_view>
SrcManager::get_lines(const std::string& path, const int start_line, const int end_line) const {
    std::vector<std::string_view> result;
    for (int i = start_line; i <= end_line; ++i) {
        if (auto line = get_line(path, i)) {
            result.push_back(*line);
        }
    }
    return result;
}

/**
 * Gets the total number of lines in a source file.
 *
 * @param path File path
 * @return Line count, or 0 if file not found
 */
int SrcManager::get_line_count(const std::string& path) const {
    const auto file_it = source_files.find(path);
    if (file_it == source_files.end()) {
        return 0;
    }
    return static_cast<int>(file_it->second.size());
}
} // namespace phi
