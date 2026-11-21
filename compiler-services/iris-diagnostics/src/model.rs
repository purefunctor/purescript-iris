use std::fmt;

use building_types::Severity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Span {
        Span { start, end }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedSpan {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiagnosticCode(&'static str);

impl DiagnosticCode {
    pub fn new(code: &'static str) -> DiagnosticCode {
        DiagnosticCode(code)
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A diagnostic produced by the compiler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The severity of the diagnostic.
    pub severity: Severity,
    /// The stable code for the diagnostic.
    pub code: DiagnosticCode,
    /// The message attached to the diagnostic.
    pub message: String,
    /// The span this diagnostic is attached to.
    pub span: Span,
    /// Related information attached to other spans.
    pub related: Vec<RelatedSpan>,
    /// Additional information for context.
    pub trivia: Vec<String>,
    /// The crate this diagnostic is attached to.
    pub source: &'static str,
}

impl Diagnostic {
    pub fn new(
        severity: Severity,
        code: &'static str,
        message: impl Into<String>,
        span: Span,
        source: &'static str,
    ) -> Diagnostic {
        let message = message.into();
        let related = Vec::new();
        let trivia = Vec::new();
        Diagnostic {
            severity,
            code: DiagnosticCode::new(code),
            message,
            span,
            related,
            trivia,
            source,
        }
    }

    /// Creates a [`Severity::Error`]-level [`Diagnostic`].
    pub fn error(
        code: &'static str,
        message: impl Into<String>,
        span: Span,
        source: &'static str,
    ) -> Diagnostic {
        Diagnostic::new(Severity::Error, code, message, span, source)
    }

    /// Attaches a [`RelatedSpan`] to a [`Diagnostic`].
    pub fn with_related(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
        let message = message.into();
        self.related.push(RelatedSpan { span, message });
        self
    }

    /// Attaches triva to a [`Diagnostic`].
    pub fn with_trivia(mut self, trivia: impl Into<String>) -> Diagnostic {
        let trivia = trivia.into();
        self.trivia.push(trivia);
        self
    }
}
