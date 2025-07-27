#include <map>
#include <optional>
#include <string>
#include <vector>

namespace phi {
/**
 * @brief Source code manager for diagnostics
 *
 * Caches and provides access to source file contents for
 * error reporting and source context display.
 */
class SrcManager {
public:
    /**
     * @brief Registers source file content
     * @param path File path identifier
     * @param content Source code content
     */
    void add_source_file(const std::string& path, std::string_view content);

    /**
     * @brief Retrieves specific source line
     * @param path File path
     * @param line_num Line number (1-indexed)
     * @return Line content or nullopt if invalid
     */
    [[nodiscard]] std::optional<std::string_view> get_line(const std::string& path,
                                                           int line_num) const;

    /**
     * @brief Retrieves source line range
     * @param path File path
     * @param start_line Start line (inclusive)
     * @param end_line End line (inclusive)
     * @return Vector of line contents
     */
    [[nodiscard]] std::vector<std::string_view>
    get_lines(const std::string& path, int start_line, int end_line) const;

    /**
     * @brief Gets total lines in file
     * @param path File path
     * @return Line count or 0 if unknown
     */
    [[nodiscard]] int get_line_count(const std::string& path) const;

private:
    std::map<std::string, std::vector<std::string_view>> source_files; ///< Source file cache
};
} // namespace phi
