//! [`SrcSpan`] is a half-open range of character (not byte) offsets into the
//! compiler's source map.
//!
//! Offsets stored in [`SrcSpan`] are global. Thus, the offsets of a span not
//! only record information about a position in a file, but also which file.
//! This removes the requirement to carry a separate file id, reducing the
//! memory footprint of the compiler.

#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrcSpan {
    begin: usize,
    end: usize,
}

impl SrcSpan {
    pub fn new(begin: usize, end: usize) -> SrcSpan {
        SrcSpan { begin, end }
    }

    pub fn get_begin(&self) -> usize {
        self.begin
    }

    pub fn get_end(&self) -> usize {
        self.end
    }

    pub fn as_tuple(&self) -> (usize, usize) {
        (self.begin, self.end)
    }

    /// Returns the smallest span that covers both `self` and `other`.
    ///
    /// Used to build a span for a larger syntax node out of its parts' spans, e.g. a whole
    /// binary expression from its left and right operand spans.
    pub fn merge(self, other: SrcSpan) -> SrcSpan {
        SrcSpan::new(
            self.begin.min(other.get_begin()),
            self.end.max(other.get_end()),
        )
    }
}
