/**
 * @file Lexer.hpp
 * @brief Lexical analyzer (scanner) for the Phi programming language
 *
 * Contains the Lexer class responsible for converting Phi source code
 * from character streams into tokens. Handles lexical elements including:
 * - Keywords and identifiers
 * - Operators and punctuation
 * - String/character literals with escape sequences
 * - Numeric literals (integers and floats)
 * - Comments and whitespace
 */
#pragma once

#include <memory>
#include <string>
#include <vector>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Token.hpp"

namespace phi {

/**
 * @brief Lexical analyzer for the Phi programming language
 *
 * Converts source code into tokens while maintaining precise source location
 * information. Handles all lexical analysis aspects including:
 * - Comprehensive token recognition
 * - Escape sequence processing in literals
 * - Numeric literal parsing with validation
 * - Comment skipping (both line and block styles)
 * - Detailed error reporting with source positions
 *
 * Maintains line/column tracking throughout scanning for accurate error locations.
 */
class Lexer {
public:
    /**
     * @brief Constructs a Lexer for given source code
     * @param src Source code string to scan
     * @param path File path for error reporting
     * @param diagnostic_manager Diagnostic system for error reporting
     */
    Lexer(std::string src, std::string path, std::shared_ptr<DiagnosticManager> diagnostic_manager)
        : src(std::move(src)),
          path(std::move(path)),
          diagnostic_manager(std::move(diagnostic_manager)) {
        cur_char = this->src.begin();
        cur_lexeme = this->src.begin();
        cur_line = this->src.begin();
        lexeme_line = this->src.begin();
    }

    /**
     * @brief Scans source code to generate tokens
     *
     * Processes the entire source string character-by-character to produce tokens.
     * Handles comments and whitespace appropriately during scanning.
     *
     * @return Pair containing:
     *         - Vector of tokens from source
     *         - Boolean indicating scanning success (false if errors occurred)
     */
    std::pair<std::vector<Token>, bool> scan();

    /**
     * @brief Retrieves source code being scanned
     * @return Copy of source code string
     */
    [[nodiscard]] std::string get_src() const { return src; }

    /**
     * @brief Retrieves source file path
     * @return Copy of file path string
     */
    [[nodiscard]] std::string get_path() const { return path; }

private:
    std::string src;                                       ///< Source code being scanned
    std::string path;                                      ///< File path for error reporting
    std::shared_ptr<DiagnosticManager> diagnostic_manager; ///< Diagnostic system

    int line_num = 1;                  ///< Current line number (1-indexed)
    std::string::iterator cur_char;    ///< Current character position
    std::string::iterator cur_lexeme;  ///< Start of current lexeme
    std::string::iterator cur_line;    ///< Start of current line
    std::string::iterator lexeme_line; ///< Start of current lexeme's line

    bool inside_str = false; ///< Inside string literal state

    // MAIN SCANNING LOGIC

    /**
     * @brief Scans next token from current position
     * @return Next token in source stream
     */
    Token scan_token();

    // TOKEN PARSING METHODS

    /**
     * @brief Parses numeric literal (integer/float)
     * @return Token representing parsed number
     */
    Token parse_number();

    /**
     * @brief Parses string literal with escape sequences
     * @return Token representing parsed string
     */
    Token parse_string();

    /**
     * @brief Parses character literal with escape sequences
     * @return Token representing parsed character
     */
    Token parse_char();

    /**
     * @brief Parses identifier/keyword
     * @return Token for identifier or recognized keyword
     */
    Token parse_identifier_or_kw();

    // UTILITY FUNCTIONS

    /**
     * @brief Skips comment content
     *
     * Handles both single-line and block comments.
     * Updates line tracking when encountering newlines in comments.
     */
    void skip_comment();

    /**
     * @brief Peeks current character without advancing
     * @return Current character, or '\0' at EOF
     */
    [[nodiscard]] char peek_char() const { return reached_eof() ? '\0' : *cur_char; }

    /**
     * @brief Peeks next character without advancing
     * @return Next character, or '\0' at/past EOF
     */
    [[nodiscard]] char peek_next() const {
        if (reached_eof() || cur_char + 1 >= src.end()) {
            return '\0';
        }
        return *(cur_char + 1);
    }

    /**
     * @brief Advances to next character
     * @return Character advanced past, or '\0' at EOF
     */
    char advance_char() {
        if (reached_eof()) return '\0';
        return *cur_char++;
    }

    /**
     * @brief Conditionally advances if current character matches
     * @param next Character to match
     * @return true if matched and advanced, false otherwise
     */
    bool match_next(const char next) {
        if (reached_eof() || peek_char() != next) {
            return false;
        }
        ++cur_char;
        return true;
    }

    /**
     * @brief Peeks next n characters
     * @param n Number of characters to peek
     * @return String of next n characters (empty if insufficient)
     */
    std::string peek_next_n(const int n) const {
        auto temp = cur_char;
        for (int i = 0; i < n; ++i) {
            if (reached_eof()) {
                return "";
            }
            ++temp;
        }
        return std::string{cur_char, temp};
    }

    /**
     * @brief Matches specific character sequence
     * @param next Sequence to match
     * @return true if sequence matched and advanced, false otherwise
     */
    bool match_next_n(const std::string_view next) {
        auto temp = cur_char;
        for (const char& c : next) {
            if (reached_eof() || *temp != c) {
                return false;
            }
            ++temp;
        }
        cur_char = temp;
        return true;
    }

    /**
     * @brief Checks for end of source
     * @return true at EOF, false otherwise
     */
    [[nodiscard]] bool reached_eof() const { return cur_char >= src.end(); }

    /**
     * @brief Creates token from current lexeme
     * @param type Token type to create
     * @return Token with specified type and current lexeme
     */
    Token make_token(TokenType type) const {
        int start_col = static_cast<int>(cur_lexeme - lexeme_line) + 1;
        int end_col = std::distance(cur_char, cur_line) + 1;

        // TODO: correctly use start line
        int start_line = line_num;
        int end_line = line_num;

        return {SrcLocation{.path = this->path, .line = start_line, .col = start_col},
                SrcLocation{.path = this->path, .line = end_line, .col = end_col},
                type,
                std::string(cur_lexeme, cur_char)};
    }

    /**
     * @brief Parses escape sequence in literals
     * @return Actual character value of escape sequence
     */
    char parse_escape_sequence();

    /**
     * @brief Parses hexadecimal escape sequence (\xNN)
     * @return Character value from hex digits
     */
    char parse_hex_escape();

    // ERROR HANDLING

    /**
     * @brief Reports scanning error using diagnostic system
     * @param message Primary error description
     * @param help_message Optional help message for resolution
     */
    void emit_lexer_error(std::string_view message, std::string_view help_message = "");

    /**
     * @brief Reports unterminated string literal error with ASCII art
     */
    void emit_unterminated_string_error(std::string::iterator string_start_pos,
                                        std::string::iterator string_start_line,
                                        int string_start_line_num);

    /**
     * @brief Reports unterminated character literal error with ASCII art
     */
    void emit_unterminated_char_error();

    /**
     * @brief Reports unclosed block comment error with ASCII art
     */
    void emit_unclosed_block_comment_error(std::string::iterator comment_start_pos,
                                           std::string::iterator comment_start_line,
                                           int comment_start_line_num);

    /**
     * @brief Gets current source location for error reporting
     * @return SrcLocation representing current lexeme position
     */
    SrcLocation get_current_location() const;

    /**
     * @brief Gets current source span for error reporting
     * @return SrcSpan representing current lexeme range
     */
    SrcSpan get_current_span() const;
};

} // namespace phi
