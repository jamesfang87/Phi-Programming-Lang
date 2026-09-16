use std::io::IsTerminal;
use std::ops::Range;

use ariadne::{Color, Config, Fmt, Label, Report, ReportKind};

use crate::driver::source::{SrcMap, SrcSpan};

pub mod checks;
pub mod codes;
pub mod display;
pub mod langitems;
pub mod mir;
pub mod nameres;
pub mod parser;
pub mod typeck;
pub(crate) mod wording;

/// How serious a diagnostic is.
///
/// Controls both the `ariadne` report kind used to render it and the color it's shown in.
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

#[derive(Debug, Clone)]
pub struct SecondaryLabel {
    pub span: SrcSpan,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    /// Stable identity for this kind of diagnostic, if one is assigned. Tests should target
    /// this rather than the [`Diagnostic::message`], which may be reworded.
    pub code: Option<&'static str>,
    pub message: String,
    pub span: Option<SrcSpan>,
    pub label: Option<String>,
    pub help: Option<String>,
    pub secondary: Vec<SecondaryLabel>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>, span: SrcSpan) -> Self {
        Self::new(Severity::Error, message, Some(span))
    }

    pub fn warning(message: impl Into<String>, span: SrcSpan) -> Self {
        Self::new(Severity::Warning, message, Some(span))
    }

    fn new(severity: Severity, message: impl Into<String>, span: Option<SrcSpan>) -> Self {
        Diagnostic {
            severity,
            code: None,
            message: message.into(),
            span,
            label: None,
            help: None,
            secondary: Vec::new(),
        }
    }

    /// An error about the compilation as a whole rather than about a place in the source.
    /// Typically used for missing lang items
    pub fn error_global(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message, None)
    }

    /// A warning about the program as a whole, with no source span to point at.
    pub fn warning_global(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message, None)
    }

    /// Attaches the stable code for this kind of diagnostic.
    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }

    /// Sets the text shown right under the highlighted span, avoiding repetition of the
    /// diagnostic message.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets a trailing "help:" note shown after the diagnostic.
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_secondary(mut self, span: SrcSpan, message: impl Into<String>) -> Self {
        self.secondary.push(SecondaryLabel {
            span,
            message: message.into(),
        });
        self
    }

    pub fn eprint(&self, sources: &SrcMap) {
        let Some(span) = self.span else {
            return self.eprint_bare();
        };
        let Some(primary) = Located::of(sources, span) else {
            return self.eprint_bare();
        };

        let mut report = Report::build(self.severity.report_kind(), primary.id())
            .with_config(Self::config())
            .with_message(&self.message);
        if let Some(code) = self.code {
            report = report.with_code(code);
        }

        // `ariadne` starts a new source group, with its own file header, whenever a label sits
        // above the one before it. Adding the labels in source order keeps them in one group.
        let mut labelled = vec![(
            span,
            primary.clone(),
            self.label.as_deref().unwrap_or(&self.message),
            self.severity.color(),
        )];
        for secondary in &self.secondary {
            let Some(at) = Located::of(sources, secondary.span) else {
                continue;
            };
            labelled.push((
                secondary.span,
                at,
                secondary.message.as_str(),
                SECONDARY_COLOR,
            ));
        }
        labelled.sort_by_key(|(span, ..)| (span.get_begin(), span.get_end()));

        let mut located = Vec::with_capacity(labelled.len());
        for (_, at, message, color) in labelled {
            report = report.with_label(Label::new(at.id()).with_message(message).with_color(color));
            located.push(at);
        }

        if let Some(help) = &self.help {
            report = report.with_help(help);
        }

        let mut cache: Vec<(&'static str, String)> = Vec::with_capacity(located.len());
        for at in located {
            if !cache.iter().any(|(name, _)| *name == at.name) {
                cache.push((at.name, at.text));
            }
        }

        report.finish().eprint(ariadne::sources(cache)).unwrap();
    }

    /// Renders this diagnostic to stderr with no source snippet.
    fn eprint_bare(&self) {
        let color = Self::config_colors().then(|| self.severity.color());

        let kind = match self.severity {
            Severity::Error => "Error",
            Severity::Warning => "Warning",
        };
        let header = match self.code {
            Some(code) => format!("{kind}[{code}]"),
            None => kind.to_string(),
        };
        eprintln!("{}: {}", header.as_str().fg(color), self.message);
        if let Some(help) = &self.help {
            eprintln!("  {}: {help}", "Help".fg(color));
        }
    }

    /// Whether rendered diagnostics should carry ANSI color.
    ///
    /// Colored escape codes work in terminal output. Emit plain text when stderr is redirected to
    /// a file, a pipe, or (as in tests under `tests/`) captured from a child process.
    fn config_colors() -> bool {
        std::io::stderr().is_terminal()
    }

    fn config() -> Config {
        Config::new().with_color(Self::config_colors())
    }
}

/// The color secondary labels are drawn in.
const SECONDARY_COLOR: Color = Color::Blue;

#[derive(Clone)]
struct Located {
    name: &'static str,
    text: String,
    range: Range<usize>,
}

impl Located {
    fn of(sources: &SrcMap, span: SrcSpan) -> Option<Self> {
        let file = sources.file_containing(span.get_begin())?;
        let (text, byte_offsets) = byte_source(&file.content);

        let last = byte_offsets.len() - 1;
        let local_begin = (span.get_begin() - file.global_offset).min(last);
        let local_end = (span.get_end() - file.global_offset).clamp(local_begin, last);

        Some(Located {
            name: file.name.as_str(),
            text,
            range: byte_offsets[local_begin]..byte_offsets[local_end],
        })
    }

    /// How `ariadne` addresses this location: which source, and where in it.
    fn id(&self) -> (&'static str, Range<usize>) {
        (self.name, self.range.clone())
    }
}

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

/// The diagnostics collected during one build.
///
/// Owned by [`Session`](crate::session::Session), which forwards its own `emit`/`report` methods
/// here. A build's diagnostics are ordinary state, not a process-wide singleton: a new build
/// starts from an empty collection and the previous build's diagnostics cannot leak into it.
#[derive(Default)]
pub struct Diagnostics {
    diagnostics: Vec<Diagnostic>,
}

impl Diagnostics {
    pub fn new() -> Self {
        Diagnostics::default()
    }

    /// Records `diagnostic`. It isn't rendered until [`Diagnostics::report`] is called.
    pub fn emit(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    /// Records an error-severity diagnostic. See [`Diagnostics::emit`].
    pub fn error(&mut self, message: impl Into<String>, span: SrcSpan) {
        self.emit(Diagnostic::error(message, span));
    }

    /// Records a warning-severity diagnostic. See [`Diagnostics::emit`].
    pub fn warning(&mut self, message: impl Into<String>, span: SrcSpan) {
        self.emit(Diagnostic::warning(message, span));
    }

    /// Returns every diagnostic recorded so far, in the order it was recorded.
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics.clone()
    }

    /// Returns just the message text of every diagnostic recorded so far, in the order they were
    /// recorded. The spans and labels are what [`Diagnostics::report`] renders; a caller
    /// comparing against expected output wants only the messages.
    pub fn messages(&self) -> Vec<String> {
        self.diagnostics
            .iter()
            .map(|diag| diag.message.clone())
            .collect()
    }

    /// Discards every diagnostic collected so far.
    pub fn clear(&mut self) {
        self.diagnostics.clear();
    }

    /// Renders every diagnostic collected so far to stderr in source order, takes them out of the
    /// collection, and returns whether any of them was error-severity.
    pub fn report(&mut self, sources: &SrcMap) -> bool {
        let pending = std::mem::take(&mut self.diagnostics);
        let had_error = pending.iter().any(|diag| diag.severity == Severity::Error);
        for diag in report_order(pending) {
            diag.eprint(sources);
        }
        had_error
    }
}

/// Sorts diagnostics into the order they are printed.
pub(crate) fn report_order(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diagnostics.sort_by_key(|diag| diag.span.map(|span| (span.get_begin(), span.get_end())));
    diagnostics
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::source::FileOrigin;

    fn messages(diagnostics: Vec<Diagnostic>) -> Vec<String> {
        diagnostics.into_iter().map(|d| d.message).collect()
    }

    #[test]
    fn a_diagnostic_can_carry_a_stable_code() {
        let coded = Diagnostic::error("boom", SrcSpan::new(0, 1)).with_code("E0301");
        assert_eq!(coded.code, Some("E0301"));

        let uncoded = Diagnostic::error("boom", SrcSpan::new(0, 1));
        assert_eq!(uncoded.code, None);
    }

    /// An offset far past the end of any file the tests below register.
    ///
    /// A local [`SrcMap`] is used rather than a session's, so an unmapped span is selected by an
    /// absurdly large offset instead of relying on the map being empty.
    const UNMAPPED: usize = usize::MAX / 2;

    #[test]
    fn a_global_error_has_no_span() {
        let diag = Diagnostic::error_global("missing lang item `core::ops::Add`");
        assert_eq!(diag.span, None);
        assert_eq!(diag.severity, Severity::Error);
    }

    /// The case that used to panic: a diagnostic with nowhere to point rendered against a
    /// `SrcMap` that has no file covering it.
    #[test]
    fn rendering_a_global_error_does_not_panic() {
        Diagnostic::error_global("missing lang item `core::ops::Add`")
            .with_help("the core library must declare this item")
            .eprint(&SrcMap::new());
    }

    /// A span belonging to no registered file renders as location-less output instead of
    /// crashing the entire report.
    #[test]
    fn rendering_an_unmapped_span_does_not_panic() {
        Diagnostic::error("span points nowhere", SrcSpan::new(UNMAPPED, UNMAPPED + 4))
            .eprint(&SrcMap::new());
    }

    /// A span exceeding its file's end is clamped instead of causing an out-of-bounds panic.
    #[test]
    fn rendering_an_overlong_span_does_not_panic() {
        let chars: Vec<char> = "fun main() {}\n".chars().collect();
        let mut sources = SrcMap::new();
        let offset = sources.add_file("<overlong>".to_string(), chars.clone(), FileOrigin::User);
        Diagnostic::error(
            "span runs past the end of the file",
            SrcSpan::new(offset + 4, offset + chars.len() + 100),
        )
        .eprint(&sources);
    }

    /// A secondary label pointing into a *different* file than the primary one. Both files have
    /// to reach `ariadne`, or it panics looking up the source it was asked to quote.
    #[test]
    fn rendering_a_cross_file_secondary_does_not_panic() {
        let decl: Vec<char> = "trait Show { fun show(self); }\n".chars().collect();
        let use_: Vec<char> = "extend Foo with Show {}\n".chars().collect();
        let mut sources = SrcMap::new();
        let decl_at = sources.add_file("<decl>".to_string(), decl.clone(), FileOrigin::User);
        let use_at = sources.add_file("<use>".to_string(), use_.clone(), FileOrigin::User);

        Diagnostic::error(
            "missing method `show`",
            SrcSpan::new(use_at, use_at + use_.len() - 1),
        )
        .with_label("`show` not implemented")
        .with_secondary(
            SrcSpan::new(decl_at + 13, decl_at + 28),
            "declared here, with no default body",
        )
        .eprint(&sources);
    }

    /// A secondary label that resolves to no file is dropped, not escalated: the error it
    /// elaborates on still gets rendered.
    #[test]
    fn an_unmapped_secondary_is_dropped_not_fatal() {
        let chars: Vec<char> = "fun main() {}\n".chars().collect();
        let mut sources = SrcMap::new();
        let offset = sources.add_file("<dropped-secondary>".to_string(), chars, FileOrigin::User);
        Diagnostic::error("something is wrong here", SrcSpan::new(offset, offset + 3))
            .with_secondary(SrcSpan::new(UNMAPPED, UNMAPPED + 4), "and because of this")
            .eprint(&sources);
    }

    /// Two labels in one file give `ariadne` one source, not the same one twice.
    #[test]
    fn rendering_two_labels_in_one_file_does_not_panic() {
        let chars: Vec<char> = "fun main() { let x = 1; let x = 2; }\n".chars().collect();
        let mut sources = SrcMap::new();
        let offset = sources.add_file("<same-file>".to_string(), chars, FileOrigin::User);
        Diagnostic::error("`x` is bound twice", SrcSpan::new(offset + 28, offset + 29))
            .with_label("second binding")
            .with_secondary(SrcSpan::new(offset + 17, offset + 18), "first binding")
            .eprint(&sources);
    }

    #[test]
    fn secondary_labels_keep_the_order_they_were_added() {
        let span = SrcSpan::new(10, 15);
        let diag = Diagnostic::error("conflict", span)
            .with_secondary(SrcSpan::new(20, 25), "first")
            .with_secondary(SrcSpan::new(30, 35), "second");
        let messages: Vec<&str> = diag.secondary.iter().map(|s| s.message.as_str()).collect();
        assert_eq!(messages, ["first", "second"]);
    }

    /// Sorting reconciles two orderings: diagnostics emit in stage-major order but readers need
    /// source-major order.
    #[test]
    fn report_orders_by_span_not_emission() {
        let ordered = report_order(vec![
            // Pipeline emission: lexer diagnostic at file end, then parser diagnostic at start.
            Diagnostic::error("late", SrcSpan::new(90, 95)),
            Diagnostic::error("early", SrcSpan::new(10, 15)),
            Diagnostic::error("middle", SrcSpan::new(50, 55)),
        ]);
        assert_eq!(messages(ordered), ["early", "middle", "late"]);
    }

    #[test]
    fn location_less_diagnostics_sort_first() {
        let ordered = report_order(vec![
            Diagnostic::error("in the source", SrcSpan::new(10, 15)),
            Diagnostic::error_global("about the build as a whole"),
        ]);
        assert_eq!(
            messages(ordered),
            ["about the build as a whole", "in the source"]
        );
    }

    /// Diagnostics at the same location preserve emission order, keeping elaborating notes
    /// attached to their error.
    #[test]
    fn equal_spans_keep_emission_order() {
        let span = SrcSpan::new(10, 15);
        let ordered = report_order(vec![
            Diagnostic::error("first note", span),
            Diagnostic::error("second note", span),
            Diagnostic::error("earlier", SrcSpan::new(1, 2)),
        ]);
        assert_eq!(messages(ordered), ["earlier", "first note", "second note"]);
    }

    /// Sorting happens only during rendering to stderr; tests asserting on single-pass output
    /// see emission order.
    #[test]
    fn diagnostics_are_stored_in_emission_order() {
        let mut diagnostics = Diagnostics::new();
        diagnostics.error("late", SrcSpan::new(90, 95));
        diagnostics.error("early", SrcSpan::new(10, 15));
        assert_eq!(messages(diagnostics.diagnostics()), ["late", "early"]);
        diagnostics.clear();
    }
}
