//! Shared diagnostics for the Artezia toolchain, rendered with `ariadne`.
//!
//! Every stage (lexer, parser, and eventually type-checking) reports problems as `Diagnostic`
//! values rather than printing directly, so a caller can collect diagnostics from multiple
//! stages, sort/dedupe them, and render them together against one source file.

use std::io::{self, Write};
use std::ops::Range;

use ariadne::{Color, Label, Report, ReportKind, Source};

/// Byte-offset span into a source file.
pub type Span = Range<usize>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Advice,
}

impl Severity {
    fn report_kind(self) -> ReportKind<'static> {
        match self {
            Severity::Error => ReportKind::Error,
            Severity::Warning => ReportKind::Warning,
            Severity::Advice => ReportKind::Advice,
        }
    }

    fn color(self) -> Color {
        match self {
            Severity::Error => Color::Red,
            Severity::Warning => Color::Yellow,
            Severity::Advice => Color::Fixed(147),
        }
    }
}

/// One reportable problem, anchored to a span in a single source file.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    /// Text shown under the underlined span; defaults to `message` if unset.
    pub label: Option<String>,
    pub notes: Vec<String>,
    pub code: Option<&'static str>,
}

impl Diagnostic {
    pub fn new(severity: Severity, span: Span, message: impl Into<String>) -> Self {
        Diagnostic {
            severity,
            span,
            message: message.into(),
            label: None,
            notes: Vec::new(),
            code: None,
        }
    }

    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, span, message)
    }

    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, span, message)
    }

    pub fn advice(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Advice, span, message)
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_code(mut self, code: &'static str) -> Self {
        self.code = Some(code);
        self
    }
}

/// Writes one `ariadne` report per diagnostic against `source`, in order.
pub fn write_reports(
    writer: &mut dyn Write,
    source: &str,
    diagnostics: &[Diagnostic],
) -> io::Result<()> {
    for diag in diagnostics {
        let color = diag.severity.color();
        let mut builder = Report::build(diag.severity.report_kind(), (), diag.span.start)
            .with_message(&diag.message);

        if let Some(code) = diag.code {
            builder = builder.with_code(code);
        }

        builder = builder.with_label(
            Label::new(diag.span.clone())
                .with_message(diag.label.clone().unwrap_or_else(|| diag.message.clone()))
                .with_color(color),
        );

        for note in &diag.notes {
            builder = builder.with_note(note);
        }

        builder.finish().write(Source::from(source), &mut *writer)?;
    }
    Ok(())
}

/// Convenience wrapper around [`write_reports`] for callers that just want a `String`
/// (tests, non-terminal consumers, etc).
pub fn render_reports(source: &str, diagnostics: &[Diagnostic]) -> String {
    let mut buf = Vec::new();
    // `ariadne` only fails to write if the underlying writer does; a `Vec<u8>` never does.
    write_reports(&mut buf, source, diagnostics).expect("writing to a Vec<u8> cannot fail");
    String::from_utf8_lossy(&buf).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_a_message_and_label() {
        let src = "let x = )";
        let diags = [Diagnostic::error(8..9, "unexpected token")
            .with_label("expected an expression here")
            .with_note("this is a test note")];
        let rendered = render_reports(src, &diags);
        assert!(rendered.contains("unexpected token"));
        assert!(rendered.contains("expected an expression here"));
        assert!(rendered.contains("this is a test note"));
    }
}
