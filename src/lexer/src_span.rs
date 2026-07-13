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

    pub fn merge(self, other: SrcSpan) -> SrcSpan {
        SrcSpan::new(
            self.begin.min(other.get_begin()),
            self.end.max(other.get_end()),
        )
    }
}
