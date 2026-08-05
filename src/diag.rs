//! This file defines the compiler's diagnostic system: `Diagnostic`, `Severity`, and the
//! `DiagCtx` singleton that collects and renders them.
//!
//! Diagnostics are addressed by global char offset into the `SrcMap`, so a pipeline stage can
//! raise one without knowing which file it came from, or worrying about the byte-versus-char
//! offset scheme it will eventually be rendered against.

use std::cell::RefCell;
use std::io::IsTerminal;
use std::ops::Range;

use ariadne::{Color, Config, Fmt, Label, Report, ReportKind, sources};

use crate::driver::src_map::SrcMap;
use crate::lexer::src_span::SrcSpan;

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

/// A second place a diagnostic points at, besides the one its `span` names.
///
/// Plenty of errors are about a *relationship* between two pieces of source rather than about
/// one piece on its own: an implementation that doesn't match the declaration it implements, an
/// `extend` block that conflicts with an earlier one, a call whose argument disagrees with the
/// parameter it was passed to. Describing the second place in prose -- "`Show` is already
/// implemented elsewhere" -- leaves the reader to go find it. A secondary label puts the
/// compiler's finger on it instead.
///
/// The span may lie in a different file from the primary one; see [`Diagnostic::eprint`] for
/// how that is rendered.
#[derive(Debug, Clone)]
pub struct SecondaryLabel {
    pub span: SrcSpan,
    pub message: String,
}

/// A single diagnostic produced by any pipeline stage, such as the lexer or parser, ready to
/// be rendered with `ariadne`.
///
/// `span` holds *global* char offsets into the `SrcMap`'s combined address space, so a
/// diagnostic doesn't need to know which file it came from until it's actually rendered.
///
/// It is `None` for a diagnostic that names no source location at all -- see
/// [`Diagnostic::error_global`]. That is a genuinely different thing from a zero-length span
/// at offset 0, which points at the first character of whichever file happens to have been
/// registered first.
///
/// `span` is where the mistake *is* -- the place whose text has to change to fix it.
/// [`secondary`](SecondaryLabel) labels are the places that explain why it's a mistake, and a
/// diagnostic should stay comprehensible with all of them stripped off: they run in a fixed
/// order after the primary, but a reader skimming only the first underline should still get the
/// point.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
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
            message: message.into(),
            span,
            label: None,
            help: None,
            secondary: Vec::new(),
        }
    }

    /// An error about the compilation as a whole rather than about a place in the source.
    ///
    /// Some failures have no honest span to point at. A missing lang item (see
    /// [`crate::langitems`]) is the motivating case: the core library is embedded in the
    /// compiler binary, so the fault is the compiler's own and there is no user source text
    /// that caused it. Pointing such an error at offset 0 -- which is what a
    /// `SrcSpan::new(0, 0)` null span does -- renders it as an underline under the first
    /// character of the user's first file, blaming code that is entirely innocent, and panics
    /// outright when no file has been registered at all.
    ///
    /// A diagnostic built this way renders as a plain message with no source snippet.
    pub fn error_global(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message, None)
    }

    /// Sets the text shown right under the highlighted span, so it doesn't just repeat the
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

    /// Adds a second place the diagnostic points at, underlined and labelled beneath the primary
    /// one. See [`SecondaryLabel`] for what belongs in one.
    ///
    /// Can be called more than once; the labels are shown in the order they were added, within
    /// each file. A label whose span belongs to no registered file is dropped when the
    /// diagnostic is rendered rather than reported as a second failure -- see
    /// [`Diagnostic::eprint`].
    pub fn with_secondary(mut self, span: SrcSpan, message: impl Into<String>) -> Self {
        self.secondary.push(SecondaryLabel {
            span,
            message: message.into(),
        });
        self
    }

    /// Renders this diagnostic to stderr.
    ///
    /// A diagnostic with a span is rendered by `ariadne` against the source it points at; one
    /// without a span, or whose span belongs to no registered file, is rendered as a bare
    /// message by [`Diagnostic::eprint_bare`].
    ///
    /// The no-covering-file case falls back rather than panicking. It should not happen for a
    /// span a stage actually built out of source text, but a diagnostic that cannot find its
    /// file is still a diagnostic worth showing, and losing it -- along with every diagnostic
    /// queued behind it -- to a panic is strictly worse than showing it without its snippet.
    /// A secondary label that can't be located is dropped on the same reasoning taken one step
    /// further: it is the elaboration, so showing the error without it beats showing neither.
    ///
    /// Secondary labels may point into other files -- a conflicting `extend` block or a trait
    /// declaration is routinely a file away from the code that violates it -- so every file any
    /// label lands in is handed to `ariadne` together, and it quotes each one in its own
    /// snippet.
    fn eprint(&self) {
        let Some(span) = self.span else {
            return self.eprint_bare();
        };
        let Some(primary) = Located::of(span) else {
            return self.eprint_bare();
        };

        let mut report = Report::build(self.severity.report_kind(), primary.id())
            .with_config(Self::config())
            .with_message(&self.message);

        report = report.with_label(
            Label::new(primary.id())
                .with_message(self.label.as_deref().unwrap_or(&self.message))
                .with_color(self.severity.color()),
        );

        // Every file a label lands in, deduplicated, since `ariadne` wants one entry per source
        // rather than one per label. The primary's file goes in whether or not a secondary
        // shares it.
        let mut located = vec![primary];
        for secondary in &self.secondary {
            let Some(at) = Located::of(secondary.span) else {
                continue;
            };
            report = report.with_label(
                Label::new(at.id())
                    .with_message(&secondary.message)
                    .with_color(SECONDARY_COLOR),
            );
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

        report.finish().eprint(sources(cache)).unwrap();
    }

    /// Renders this diagnostic to stderr with no source snippet.
    ///
    /// `ariadne` is built around quoting source, and asking it for a label-less report still
    /// draws the empty snippet frame, which reads as a rendering failure rather than as a
    /// deliberate choice. So this writes the header and help lines directly, matching the shape
    /// `ariadne` gives them -- `Error: <message>`, then an indented `Help: <help>` -- so the two
    /// kinds of diagnostic sit together in one report without looking mismatched.
    ///
    /// `self.label` and `self.secondary` are deliberately ignored: a label is text placed under
    /// an underline, and there is no underline here. Nor is one reachable -- a diagnostic gets
    /// here because it has no locatable span of its own, which is exactly the case where the
    /// stage that raised it had nothing to point a secondary label at either.
    fn eprint_bare(&self) {
        // `Fmt::fg` takes an `Option<Color>` and leaves the text unpainted for `None`, which is
        // how the no-color case stays a plain string rather than an escape-code sandwich.
        let color = Self::config_colors().then(|| self.severity.color());

        let kind = match self.severity {
            Severity::Error => "Error",
            Severity::Warning => "Warning",
        };
        eprintln!("{}: {}", kind.fg(color), self.message);
        if let Some(help) = &self.help {
            eprintln!("  {}: {help}", "Help".fg(color));
        }
    }

    /// Whether rendered diagnostics should carry ANSI color.
    ///
    /// Colored escape codes are only useful, and only correctly interpreted, by an actual
    /// terminal. Emit plain text instead when stderr is redirected to a file, a pipe, or, as in
    /// the golden tests under `tests/`, captured from a child process.
    fn config_colors() -> bool {
        std::io::stderr().is_terminal()
    }

    fn config() -> Config {
        Config::new().with_color(Self::config_colors())
    }
}

/// The color secondary labels are drawn in.
///
/// Deliberately not the severity's own color: an underline in error red reads as a second error,
/// and a secondary label is the opposite -- the context that explains the one error above it.
const SECONDARY_COLOR: Color = Color::Blue;

/// A [`SrcSpan`] resolved against the [`SrcMap`]: the file it lies in, that file's UTF-8 text,
/// and the byte range the span covers there.
///
/// This is the translation `ariadne` needs. Spans are global and char-indexed, because that is
/// what lets a stage raise a diagnostic without knowing which file it is in; `ariadne` addresses
/// source per file and byte-indexed. Nothing else in the compiler needs the second form, so the
/// conversion lives here, at the point of rendering.
struct Located {
    name: &'static str,
    text: String,
    range: Range<usize>,
}

impl Located {
    /// Resolves `span`, or `None` if no registered file covers it.
    fn of(span: SrcSpan) -> Option<Self> {
        let file = SrcMap::file_containing(span.get_begin())?;
        let (text, byte_offsets) = byte_source(&file.content);

        // A span is half-open and `byte_offsets` has one entry per char plus a final one for
        // the end, so both ends are in range for any span that lies within this file. Clamp
        // anyway: a span running past the file's end is a bug in whichever stage built it, and
        // it should surface as a slightly wide underline rather than an index-out-of-bounds
        // panic that takes the whole diagnostic report down with it.
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

/// Builds the UTF-8 text of a `char` source together with a lookup table from `char` index to
/// byte offset.
///
/// This lets a `char`-indexed [`SrcSpan`] be translated into the byte-indexed spans `ariadne`
/// expects. `byte_offsets` has `src.len() + 1` entries, so that both a span's start and its
/// (exclusive) end can always be looked up.
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
    /// The actual diagnostic storage for the [`DiagCtx`] singleton.
    ///
    /// It's thread-local rather than a single process-wide global, so that compiling on
    /// different threads, and running tests, which execute on a pool of worker threads, never
    /// lets diagnostics from one compilation bleed into another.
    static DIAGNOSTICS: RefCell<Vec<Diagnostic>> = const { RefCell::new(Vec::new()) };
}

/// Collects and renders every diagnostic raised while compiling, regardless of which pipeline
/// stage, such as the lexer or parser, raised it.
///
/// There is exactly one of these per thread. Pipeline stages call its associated functions
/// directly instead of threading a `&mut DiagCtx` through every constructor.
pub struct DiagCtx;

impl DiagCtx {
    /// Records `diagnostic` on the current thread. It isn't rendered until [`DiagCtx::report`]
    /// is called.
    pub fn emit(diagnostic: Diagnostic) {
        DIAGNOSTICS.with(|d| d.borrow_mut().push(diagnostic));
    }

    /// Records an error-severity diagnostic. See [`DiagCtx::emit`].
    pub fn error(message: impl Into<String>, span: SrcSpan) {
        Self::emit(Diagnostic::error(message, span));
    }

    /// Records a warning-severity diagnostic. See [`DiagCtx::emit`].
    pub fn warning(message: impl Into<String>, span: SrcSpan) {
        Self::emit(Diagnostic::warning(message, span));
    }

    /// Returns every diagnostic recorded so far on this thread, in the order they were
    /// recorded.
    pub fn diagnostics() -> Vec<Diagnostic> {
        DIAGNOSTICS.with(|d| d.borrow().clone())
    }

    /// Returns whether any diagnostic recorded so far on this thread is error-severity.
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

    /// Renders every diagnostic collected so far to stderr, in source order.
    ///
    /// Diagnostics are *collected* in emission order, which is stage-major: every diagnostic the
    /// lexer raised across all files, then every one the parser raised, and so on. That is an
    /// artifact of how the compiler is structured and means nothing to someone reading the
    /// output, who is working through their file top to bottom. So they are ordered by span
    /// before being printed.
    ///
    /// Location-less diagnostics (see [`Diagnostic::error_global`]) sort first. They describe the
    /// build as a whole rather than a place in it -- a missing lang item means the compiler
    /// itself is broken -- so they belong at the top, where they frame everything after them,
    /// rather than buried under a screen of ordinary errors.
    ///
    /// The sort is stable, so two diagnostics about the same span stay in the order the stage
    /// that raised them meant: a note elaborating on an error keeps sitting next to it.
    ///
    /// Only the printing is ordered. [`DiagCtx::diagnostics`] still hands back emission order,
    /// which is what its callers -- tests asserting on what a single pass raised -- are asking
    /// about.
    pub fn report() {
        for diag in Self::report_order(Self::diagnostics()) {
            diag.eprint();
        }
    }

    /// Puts `diagnostics` into the order [`DiagCtx::report`] prints them in.
    ///
    /// Split out from `report` so the ordering can be asserted on without capturing stderr.
    fn report_order(mut diagnostics: Vec<Diagnostic>) -> Vec<Diagnostic> {
        // `sort_by_key` is stable, which is what keeps equal spans in emission order.
        // `Option`'s own ordering puts `None` first, which is where location-less diagnostics
        // are documented to go.
        diagnostics.sort_by_key(|diag| diag.span.map(|span| (span.get_begin(), span.get_end())));
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::src_file::FileOrigin;

    fn messages(diagnostics: Vec<Diagnostic>) -> Vec<String> {
        diagnostics.into_iter().map(|d| d.message).collect()
    }

    /// An offset far past the end of anything the rest of the test suite could have registered.
    ///
    /// `SrcMap` is process-wide and shared by every test, so a span that is guaranteed to have
    /// no owning file has to be picked by being absurd rather than by emptying the map.
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
            .eprint();
    }

    /// A span that belongs to no registered file degrades to the same location-less rendering
    /// rather than taking the whole report down with it.
    #[test]
    fn rendering_an_unmapped_span_does_not_panic() {
        Diagnostic::error("span points nowhere", SrcSpan::new(UNMAPPED, UNMAPPED + 4)).eprint();
    }

    /// A span running past the end of its file is clamped rather than indexed out of bounds.
    #[test]
    fn rendering_an_overlong_span_does_not_panic() {
        let chars: Vec<char> = "fun main() {}\n".chars().collect();
        let offset = SrcMap::add_file("<overlong>".to_string(), chars.clone(), FileOrigin::User);
        Diagnostic::error(
            "span runs past the end of the file",
            SrcSpan::new(offset + 4, offset + chars.len() + 100),
        )
        .eprint();
    }

    /// A secondary label pointing into a *different* file than the primary one. Both files have
    /// to reach `ariadne`, or it panics looking up the source it was asked to quote.
    #[test]
    fn rendering_a_cross_file_secondary_does_not_panic() {
        let decl: Vec<char> = "trait Show { fun show(self); }\n".chars().collect();
        let decl_at = SrcMap::add_file("<decl>".to_string(), decl.clone(), FileOrigin::User);
        let use_: Vec<char> = "extend Foo with Show {}\n".chars().collect();
        let use_at = SrcMap::add_file("<use>".to_string(), use_.clone(), FileOrigin::User);

        Diagnostic::error("missing method `show`", SrcSpan::new(use_at, use_at + use_.len() - 1))
            .with_label("`show` not implemented")
            .with_secondary(
                SrcSpan::new(decl_at + 13, decl_at + 28),
                "declared here, with no default body",
            )
            .eprint();
    }

    /// A secondary label that resolves to no file is dropped, not escalated: the error it
    /// elaborates on still gets rendered.
    #[test]
    fn an_unmapped_secondary_is_dropped_not_fatal() {
        let chars: Vec<char> = "fun main() {}\n".chars().collect();
        let offset = SrcMap::add_file("<dropped-secondary>".to_string(), chars, FileOrigin::User);
        Diagnostic::error("something is wrong here", SrcSpan::new(offset, offset + 3))
            .with_secondary(SrcSpan::new(UNMAPPED, UNMAPPED + 4), "and because of this")
            .eprint();
    }

    /// Two labels in one file give `ariadne` one source, not the same one twice.
    #[test]
    fn rendering_two_labels_in_one_file_does_not_panic() {
        let chars: Vec<char> = "fun main() { let x = 1; let x = 2; }\n".chars().collect();
        let offset = SrcMap::add_file("<same-file>".to_string(), chars, FileOrigin::User);
        Diagnostic::error("`x` is bound twice", SrcSpan::new(offset + 28, offset + 29))
            .with_label("second binding")
            .with_secondary(SrcSpan::new(offset + 17, offset + 18), "first binding")
            .eprint();
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

    /// The point of sorting: diagnostics are emitted stage-major, but read source-major.
    #[test]
    fn report_orders_by_span_not_emission() {
        let ordered = DiagCtx::report_order(vec![
            // As the pipeline would emit them: the lexer's diagnostic about the end of the
            // file, then the parser's about the start of it.
            Diagnostic::error("late", SrcSpan::new(90, 95)),
            Diagnostic::error("early", SrcSpan::new(10, 15)),
            Diagnostic::error("middle", SrcSpan::new(50, 55)),
        ]);
        assert_eq!(messages(ordered), ["early", "middle", "late"]);
    }

    #[test]
    fn location_less_diagnostics_sort_first() {
        let ordered = DiagCtx::report_order(vec![
            Diagnostic::error("in the source", SrcSpan::new(10, 15)),
            Diagnostic::error_global("about the build as a whole"),
        ]);
        assert_eq!(
            messages(ordered),
            ["about the build as a whole", "in the source"]
        );
    }

    /// Two diagnostics about the same place keep the order the stage that raised them chose, so
    /// a note elaborating on an error stays attached to it.
    #[test]
    fn equal_spans_keep_emission_order() {
        let span = SrcSpan::new(10, 15);
        let ordered = DiagCtx::report_order(vec![
            Diagnostic::error("first note", span),
            Diagnostic::error("second note", span),
            Diagnostic::error("earlier", SrcSpan::new(1, 2)),
        ]);
        assert_eq!(messages(ordered), ["earlier", "first note", "second note"]);
    }

    /// Sorting happens on the way to stderr only, so tests that assert on what a single pass
    /// raised still see emission order.
    #[test]
    fn diagnostics_are_stored_in_emission_order() {
        DiagCtx::clear();
        DiagCtx::error("late", SrcSpan::new(90, 95));
        DiagCtx::error("early", SrcSpan::new(10, 15));
        assert_eq!(messages(DiagCtx::diagnostics()), ["late", "early"]);
        DiagCtx::clear();
    }
}
