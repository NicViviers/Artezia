use ariadne::{Color, Config, Label, Report, ReportKind};
use std::io::Write;
use crate::{Diagnostic, Severity, sourcemap::{SourceMap, localize}};

fn kind_of(sev: Severity) -> ReportKind<'static> {
    match sev {
        Severity::Error => ReportKind::Error,
        Severity::Warning => ReportKind::Warning,
        Severity::Advice => ReportKind::Advice
    }
}

fn color_of(sev: Severity) -> Color {
    match sev {
        Severity::Error => Color::Red,
        Severity::Warning => Color::Yellow,
        Severity::Advice => Color::Cyan
    }
}

/// Render one diagnostic into `out`
pub fn render_one<W: std::io::Write>(d: &Diagnostic, map: &SourceMap, out: &mut W, disable_colors: bool) -> std::io::Result<()> {
    let (file, local) = localize(map, &d.span);

    let mut report = Report::build(
        kind_of(d.severity),
        file.clone(),
        local.start
    ).with_message(&d.message).with_config(Config::default().with_color(!disable_colors));

    if let Some(code) = d.code {
        report = report.with_code(code);
    }

    let label_text = d.label.as_deref().unwrap_or(&d.message);
    report = report.with_label(
        Label::new((file.clone(), local))
            .with_message(label_text)
            .with_color(color_of(d.severity))
    );

    for (span, text) in &d.secondary {
        let (sfile, slocal) = localize(map, span);
        report = report.with_label(
            Label::new((sfile, slocal))
                .with_message(text)
                .with_color(Color::Blue)
        );
    }

    for note in &d.notes {
        report = report.with_note(note);
    }

    let cache = ariadne::sources(map.sources().map(|(n, t)| (n, t.to_string())).collect::<Vec<_>>());

    report.finish().write(cache, out)
}

/// Render all diagnostics to stderr. Returns the number of errors
pub fn report_all(diags: &[Diagnostic], map: &SourceMap, disable_colors: bool) -> usize {
    let mut errors = 0;
    let stderr = std::io::stderr();
    let mut lock = stderr.lock();

    for d in diags {
        if matches!(d.severity, Severity::Error) {
            errors += 1;
        }

        let _ = render_one(d, map, &mut lock, disable_colors);
    }

    if errors > 0 {
        let _ = writeln!(lock, "\n{} error(s) emitted", errors);
    }

    errors
}

/// Render all diagnostics into a String
pub fn render_to_string(diags: &[Diagnostic], map: &SourceMap, disable_colors: bool) -> String {
    let mut buf = Vec::new();

    for d in diags {
        let _ = render_one(d, map, &mut buf, disable_colors);
    }

    String::from_utf8_lossy(&buf).into_owned()
}