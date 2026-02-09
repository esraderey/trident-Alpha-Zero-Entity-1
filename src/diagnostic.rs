use crate::span::Span;

/// A compiler diagnostic (error, warning, or hint).
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub span: Span,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

impl Diagnostic {
    pub fn error(message: String, span: Span) -> Self {
        Self {
            severity: Severity::Error,
            message,
            span,
            notes: Vec::new(),
            help: None,
        }
    }

    pub fn warning(message: String, span: Span) -> Self {
        Self {
            severity: Severity::Warning,
            message,
            span,
            notes: Vec::new(),
            help: None,
        }
    }

    pub fn with_note(mut self, note: String) -> Self {
        self.notes.push(note);
        self
    }

    pub fn with_help(mut self, help: String) -> Self {
        self.help = Some(help);
        self
    }

    /// Render the diagnostic to stderr using ariadne.
    pub fn render(&self, filename: &str, source: &str) {
        use ariadne::{Color, Label, Report, ReportKind, Source};

        let kind = match self.severity {
            Severity::Error => ReportKind::Error,
            Severity::Warning => ReportKind::Warning,
        };

        let color = match self.severity {
            Severity::Error => Color::Red,
            Severity::Warning => Color::Yellow,
        };

        let mut report = Report::build(kind, filename, self.span.start as usize)
            .with_message(&self.message)
            .with_label(
                Label::new((filename, self.span.start as usize..self.span.end as usize))
                    .with_message(&self.message)
                    .with_color(color),
            );

        for note in &self.notes {
            report = report.with_note(note);
        }

        if let Some(help) = &self.help {
            report = report.with_help(help);
        }

        report
            .finish()
            .eprint((filename, Source::from(source)))
            .unwrap();
    }
}

/// Render a list of diagnostics.
pub fn render_diagnostics(diagnostics: &[Diagnostic], filename: &str, source: &str) {
    for diag in diagnostics {
        diag.render(filename, source);
    }
}
