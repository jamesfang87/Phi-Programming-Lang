//! Defines [`SrcSpan`], a half-open range of character offsets into the compiler's source map.
//!
//! Every compiler stage that points at source code (the lexer, the parser, diagnostics) uses
//! this type instead of file-local positions. The offsets are global, so a span survives being
//! passed between stages without carrying a file ID alongside it.

/// [`crate::driver::src_map::SrcMap`] assigns each file a slice of the offset space, so a span
/// always resolves back to one file and one position within it.
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
