//! Shared diagnostics for the Artezia toolchain, rendered with `ariadne`.
//!
//! Every stage (lexer, parser, and eventually type-checking) reports problems as `Diagnostic`
//! values rather than printing directly, so a caller can collect diagnostics from multiple
//! stages, sort/dedupe them, and render them together against one source file.
mod render;
mod sourcemap;
pub use render::*;
pub use sourcemap::*;

use std::ops::Range;
use ariadne::{Color, ReportKind};

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

/// One reportable problem, anchored to a span in a single source file
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    /// Text shown under the underlined span; defaults to `message` if unset
    pub label: Option<String>,
    pub notes: Vec<String>,
    pub code: Option<&'static str>,
    pub secondary: Vec<(Span, String)> // Extra underlined regions (span, label)
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
            secondary: Vec::new()
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

    pub fn with_secondary(mut self, span: Span, label: impl Into<String>) -> Self {
        self.secondary.push((span, label.into()));
        self
    }
}