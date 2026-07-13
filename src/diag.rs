use std::cell::RefCell;

use ariadne::{Color, Label, Report, ReportKind, Source};

use crate::driver::src_map::SrcMap;
use crate::lexer::src_span::SrcSpan;

/// How serious a diagnostic is; controls both the `ariadne` report kind and its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Severity {
    fn report_kind(self) -> ReportKind<'static> {
        match self {
            Severity::Error => ReportKind::Error,
            Severity::Warning => ReportKind::Warning,
        }
    }

    fn color(self) -> Color {
        match self {
            Severity::Error => Color::Red,
            Severity::Warning => Color::Yellow,
        }
    }
}

/// A single diagnostic produced by any pipeline stage (lexer, parser, ...), ready to be
/// rendered with `ariadne`. `span` holds *global* char offsets into the `SrcMap`'s combined
/// address space, so a diagnostic doesn't need to know which file it came from until it's
/// actually rendered.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: SrcSpan,
    pub label: Option<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: SrcSpan) -> Self {
        Diagnostic {
            severity: Severity::Error,
            message: message.into(),
            span,
            label: None,
            help: None,
        }
    }

    pub fn warning(message: impl Into<String>, span: SrcSpan) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            message: message.into(),
            span,
            label: None,
            help: None,
        }
    }

    /// Text shown right under the highlighted span, instead of just repeating the message.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// A trailing "help:" note.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Render this diagnostic to stderr via `ariadne`, using `src_map` to figure out which
    /// file `self.span` belongs to and to translate its global char offsets into the
    /// byte-indexed spans `ariadne` expects.
    fn eprint(&self) {
        let file = SrcMap::file_containing(self.span.get_begin())
            .expect("diagnostic span does not belong to any file in the SrcMap");

        let local_begin = self.span.get_begin() - file.global_offset;
        let local_end = self.span.get_end() - file.global_offset;
        let (text, byte_offsets) = byte_source(&file.content);
        let start = byte_offsets[local_begin];
        let end = byte_offsets[local_end];

        let mut report = Report::build(
            self.severity.report_kind(),
            (file.name.as_str(), start..end),
        )
        .with_message(&self.message);

        report = report.with_label(
            Label::new((file.name.as_str(), start..end))
                .with_message(self.label.as_deref().unwrap_or(&self.message))
                .with_color(self.severity.color()),
        );

        if let Some(help) = &self.help {
            report = report.with_help(help);
        }

        report
            .finish()
            .eprint((file.name.as_str(), Source::from(text)))
            .unwrap();
    }
}

/// Builds the UTF-8 text of a `char` source together with a lookup table from `char` index to
/// byte offset, so a `char`-indexed [`SrcSpan`] can be translated into the byte-indexed spans
/// `ariadne` expects. `byte_offsets` has `src.len() + 1` entries so that both a span's start and
/// its (exclusive) end can always be looked up.
fn byte_source(src: &[char]) -> (String, Vec<usize>) {
    let mut text = String::with_capacity(src.len());
    let mut byte_offsets = Vec::with_capacity(src.len() + 1);
    byte_offsets.push(0);
    for &c in src {
        text.push(c);
        byte_offsets.push(text.len());
    }
    (text, byte_offsets)
}

thread_local! {
    /// The actual diagnostic storage backing the [`DiagCtx`] singleton. Thread-local rather than
    /// a single process-wide global so that compiling on different threads (and running tests,
    /// which execute on a pool of worker threads) never lets diagnostics from one compilation
    /// bleed into another.
    static DIAGNOSTICS: RefCell<Vec<Diagnostic>> = const { RefCell::new(Vec::new()) };
}

/// Collects and renders every diagnostic raised while compiling, regardless of which pipeline
/// stage (lexer, parser, ...) raised it. There is exactly one of these per thread — pipeline
/// stages call its associated functions directly instead of threading a `&mut DiagCtx` through
/// every constructor.
pub struct DiagCtx;

impl DiagCtx {
    pub fn emit(diagnostic: Diagnostic) {
        DIAGNOSTICS.with(|d| d.borrow_mut().push(diagnostic));
    }

    pub fn error(message: impl Into<String>, span: SrcSpan) {
        Self::emit(Diagnostic::error(message, span));
    }

    pub fn warning(message: impl Into<String>, span: SrcSpan) {
        Self::emit(Diagnostic::warning(message, span));
    }

    pub fn diagnostics() -> Vec<Diagnostic> {
        DIAGNOSTICS.with(|d| d.borrow().clone())
    }

    pub fn has_errors() -> bool {
        DIAGNOSTICS.with(|d| {
            d.borrow()
                .iter()
                .any(|diag| diag.severity == Severity::Error)
        })
    }

    /// Discards every diagnostic collected so far on this thread.
    pub fn clear() {
        DIAGNOSTICS.with(|d| d.borrow_mut().clear());
    }

    /// Render every diagnostic collected so far to stderr, in the order they were recorded.
    pub fn report() {
        DIAGNOSTICS.with(|d| {
            for diag in d.borrow().iter() {
                diag.eprint();
            }
        });
    }
}
