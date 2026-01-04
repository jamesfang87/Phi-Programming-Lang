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
   * @param Start Start location
   * @param End End location
   */
  SrcSpan(SrcLocation Start, SrcLocation End)
      : Start(std::move(Start)), End(std::move(End)) {}

  /**
   * @brief Constructs single-position span
   * @param SinglePos Location for single point
   */
  explicit SrcSpan(const SrcLocation &SinglePos)
      : Start(SinglePos), End(SinglePos) {}

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
