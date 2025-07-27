#include <map>
#include <optional>
#include <string>
#include <vector>

namespace phi {
/// Manages source files and their contents for diagnostic rendering
class SrcManager {
public:
    /// Register a source file with its content
    void add_source_file(const std::string& path, std::string_view content);

    /// Get a specific line from a source file
    [[nodiscard]] std::optional<std::string_view> get_line(const std::string& path,
                                                           int line_num) const;

    /// Get multiple lines from a source file
    [[nodiscard]] std::vector<std::string_view>
    get_lines(const std::string& path, int start_line, int end_line) const;

    /// Get the total number of lines in a file
    [[nodiscard]] int get_line_count(const std::string& path) const;

private:
    std::map<std::string, std::vector<std::string_view>> source_files_;
};
} // namespace phi
