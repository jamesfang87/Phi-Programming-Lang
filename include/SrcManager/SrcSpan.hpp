#pragma once

#include "SrcManager/SrcLocation.hpp"

namespace phi {

/**
 * @brief Represents a source code span
 *
 * Defines a contiguous region of source code with start and end positions.
 * Used for error highlighting and source context.
 */
struct SrcSpan {
  SrcLocation Start; ///< Start position (inclusive)
  SrcLocation End;   ///< End position (inclusive)

  /**
   * @brief Constructs span from positions
   * @param start Start location
   * @param end End location
   */
  SrcSpan(SrcLocation start, SrcLocation end)
      : Start(std::move(start)), End(std::move(end)) {}

  /**
   * @brief Constructs single-position span
   * @param single_pos Location for single point
   */
  explicit SrcSpan(const SrcLocation &single_pos)
      : Start(single_pos), End(single_pos) {}

  /**
   * @brief Checks for multi-line span
   * @return true if spans multiple lines
   */
  [[nodiscard]] bool isMultiline() const { return Start.Line != End.Line; }

  /**
   * @brief Calculates line count
   * @return Number of lines covered
   */
  [[nodiscard]] int lineCount() const { return End.Line - Start.Line + 1; }
};

} // namespace phi
