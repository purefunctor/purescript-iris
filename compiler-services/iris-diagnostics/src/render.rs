use itertools::Itertools;
use line_index::{LineCol, LineIndex, WideEncoding};
use lsp_types::{
    Code, DiagnosticRelatedInformation, DiagnosticSeverity, Location, Message, Position, Range,
};
use syntax::TextSize;
use unicode_width::UnicodeWidthChar;

use crate::{Diagnostic, DiagnosticSection, Severity, Span};

const DIAGNOSTIC_WIDTH: usize = 100;
const RICH_BRANCH: &str = "  •";
const RICH_STEM: &str = "  │";
const RICH_SEPARATOR: &str = "·";
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD_RED: &str = "\x1b[1;31m";
const ANSI_BOLD_YELLOW: &str = "\x1b[1;33m";
const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_DIM: &str = "\x1b[2m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DisplayPosition {
    line: u32,
    column: u32,
}

fn line_text<'a>(line_index: &LineIndex, content: &'a str, line: u32) -> Option<&'a str> {
    let range = line_index.line(line)?;
    let text = &content[range];
    Some(text.trim_end_matches(['\n', '\r']))
}

fn span_location(
    line_index: &LineIndex,
    content: &str,
    span: Span,
) -> Option<((u32, u32), (u32, u32))> {
    let start = offset_to_display_position(line_index, content, TextSize::from(span.start))?;
    let end = offset_to_display_position(line_index, content, TextSize::from(span.end))?;
    Some(((start.line, start.column), (end.line, end.column)))
}

pub fn format_rich_with_path(
    diagnostics: &[Diagnostic],
    content: &str,
    line_index: &LineIndex,
    path: &str,
    color: bool,
) -> String {
    let mut output = String::new();

    for diagnostic in diagnostics {
        let severity_text = match diagnostic.severity {
            Severity::Error => "Error!",
            Severity::Warning => "Warning!",
        };
        let severity = painted(severity_text, severity_color(diagnostic.severity), color);
        let separator = painted(RICH_SEPARATOR, ANSI_DIM, color);
        let code = painted(diagnostic.code.to_string(), ANSI_BOLD_YELLOW, color);
        let location = span_location(line_index, content, diagnostic.span).map_or_else(
            || path.to_string(),
            |((line, column), _)| format!("{path}:{}:{}", line + 1, column + 1),
        );
        let location = painted(location, ANSI_BOLD_YELLOW, color);
        output.push_str(&format!("{severity} {separator} [{code}] {separator} {location}\n"));

        let branch = painted(RICH_BRANCH, ANSI_DIM, color);
        let stem = painted(RICH_STEM, ANSI_DIM, color);
        output.push_str(&format!("{branch}\n{stem}\n"));

        let message_width = DIAGNOSTIC_WIDTH.saturating_sub(RICH_BRANCH.chars().count() + 1);
        render_bullet_text(&mut output, &diagnostic.message, message_width, &branch, &stem, color);
        output.push_str(&format!("{stem}\n"));
        render_rich_source(
            &mut output,
            line_index,
            content,
            diagnostic.span,
            diagnostic.severity,
            color,
        );

        for related in &diagnostic.related {
            output.push_str(&format!("{stem}\n"));
            render_bullet_text(&mut output, &related.message, message_width, &branch, &stem, color);
            output.push_str(&format!("{stem}\n"));
            render_rich_source(
                &mut output,
                line_index,
                content,
                related.span,
                diagnostic.severity,
                color,
            );
        }

        for section in &diagnostic.sections {
            output.push_str(&format!("{stem}\n"));
            match section {
                DiagnosticSection::Context(text) => {
                    render_bullet_text(&mut output, text, message_width, &branch, &stem, color);
                }
                DiagnosticSection::List { heading, items } => {
                    render_bullet_text(&mut output, heading, message_width, &branch, &stem, color);
                    for item in items {
                        let wrapped = textwrap::wrap(item, message_width.saturating_sub(2));
                        for line in wrapped {
                            let line = paint_quoted_text(&line, color);
                            output.push_str(&format!("{stem}   {line}\n"));
                        }
                    }
                }
                DiagnosticSection::Hint(text) => {
                    let full = format!("Hint: {text}");
                    let wrapped = textwrap::wrap(&full, message_width);
                    for (index, line) in wrapped.iter().enumerate() {
                        let gutter = if index == 0 { &branch } else { &stem };
                        if line.is_empty() {
                            output.push_str(&format!("{gutter}\n"));
                        } else if index == 0
                            && let Some(rest) = line.strip_prefix("Hint:")
                        {
                            let prefix = painted("Hint:", ANSI_CYAN, color);
                            let rest = paint_quoted_text(rest, color);
                            output.push_str(&format!("{gutter} {prefix}{rest}\n"));
                        } else {
                            let line = paint_quoted_text(line, color);
                            output.push_str(&format!("{gutter} {line}\n"));
                        }
                    }
                }
            }
        }
        output.push_str(&format!("{branch}\n\n"));
    }

    output
}

fn render_bullet_text(
    output: &mut String,
    text: &str,
    width: usize,
    branch: &str,
    stem: &str,
    color: bool,
) {
    let wrapped = textwrap::wrap(text, width);
    for (index, line) in wrapped.iter().enumerate() {
        let gutter = if index == 0 { branch } else { stem };
        if line.is_empty() {
            output.push_str(&format!("{gutter}\n"));
        } else {
            let line = paint_quoted_text(line, color);
            output.push_str(&format!("{gutter} {line}\n"));
        }
    }
}

fn render_rich_source(
    output: &mut String,
    line_index: &LineIndex,
    content: &str,
    span: Span,
    severity: Severity,
    color: bool,
) {
    let Some(((start_line, start_column), (end_line, end_column))) =
        span_location(line_index, content, span)
    else {
        return;
    };
    let Some(original_source_line) = line_text(line_index, content, start_line) else { return };
    let source_line = expand_tabs(original_source_line);
    let start_column = visual_column(original_source_line, start_column);
    let end_column = if start_line == end_line {
        visual_column(original_source_line, end_column)
    } else {
        display_width(original_source_line)
    };
    let marker = visual_highlight_marker(start_column, end_column, display_width(&source_line));
    let marker = painted(marker, severity_color(severity), color);
    let stem = painted(RICH_STEM, ANSI_DIM, color);
    if source_line.is_empty() {
        output.push_str(&format!("{stem}\n"));
    } else {
        output.push_str(&format!("{stem}   {source_line}\n"));
    }
    output.push_str(&format!("{stem}   {marker}\n"));
}

fn painted(text: impl AsRef<str>, ansi: &str, color: bool) -> String {
    if color { format!("{ansi}{}{ANSI_RESET}", text.as_ref()) } else { text.as_ref().to_string() }
}

fn severity_color(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => ANSI_BOLD_RED,
        Severity::Warning => ANSI_BOLD_YELLOW,
    }
}

fn expand_tabs(line: &str) -> String {
    let mut expanded = String::new();
    let mut column = 0;
    for character in line.chars() {
        if character == '\t' {
            let spaces = 4 - column % 4;
            expanded.push_str(&" ".repeat(spaces));
            column += spaces;
        } else {
            expanded.push(character);
            column += character.width().unwrap_or(0);
        }
    }
    expanded
}

fn visual_column(line: &str, character_column: u32) -> u32 {
    let mut column = 0;
    for character in line.chars().take(character_column as usize) {
        if character == '\t' {
            column += 4 - column % 4;
        } else {
            column += character.width().unwrap_or(0);
        }
    }
    column as u32
}

fn display_width(line: &str) -> u32 {
    visual_column(line, line.chars().count() as u32)
}

fn visual_highlight_marker(start_column: u32, end_column: u32, line_width: u32) -> String {
    let start = start_column.min(line_width);
    let end = end_column.min(line_width);
    if end <= start {
        format!("{}╰", " ".repeat(start as usize))
    } else {
        let line_count = end.saturating_sub(start).saturating_sub(1) as usize;
        format!("{}╰{}", " ".repeat(start as usize), "─".repeat(line_count))
    }
}

fn paint_quoted_text(text: &str, color: bool) -> String {
    if !color {
        return text.to_string();
    }

    let mut output = String::new();
    let mut quoted = false;
    let mut characters = text.chars().peekable();
    let mut previous: Option<char> = None;
    while let Some(character) = characters.next() {
        let next = characters.peek().copied();
        let is_apostrophe = !quoted
            && character == '\''
            && previous.is_some_and(char::is_alphanumeric)
            && next.is_some_and(char::is_alphanumeric);
        let is_inner_prime = quoted
            && character == '\''
            && next.is_some_and(|next| next.is_alphanumeric() || next == '\'');
        if character == '\'' && !is_apostrophe && !is_inner_prime {
            if quoted {
                output.push(character);
                output.push_str(ANSI_RESET);
            } else {
                output.push_str(ANSI_CYAN);
                output.push(character);
            }
            quoted = !quoted;
        } else {
            output.push(character);
        }
        previous = Some(character);
    }
    if quoted {
        output.push_str(ANSI_RESET);
    }
    output
}

fn offset_to_display_position(
    line_index: &LineIndex,
    content: &str,
    offset: TextSize,
) -> Option<DisplayPosition> {
    let LineCol { line, col } = line_index.line_col(offset);

    let line_text_range = line_index.line(line)?;
    let line_content = &content[line_text_range];

    let until_col = &line_content[..col as usize];
    let column = until_col.chars().count() as u32;

    Some(DisplayPosition { line, column })
}

fn offset_to_lsp_position(
    line_index: &LineIndex,
    offset: TextSize,
    encoding: &lsp_types::PositionEncodingKind,
) -> Option<Position> {
    let line_col = line_index.try_line_col(offset)?;
    let character = if encoding == &lsp_types::PositionEncodingKind::UTF8 {
        line_col.col
    } else if encoding == &lsp_types::PositionEncodingKind::UTF32 {
        line_index.to_wide(WideEncoding::Utf32, line_col)?.col
    } else {
        line_index.to_wide(WideEncoding::Utf16, line_col)?.col
    };

    Some(Position { line: line_col.line, character })
}

fn format_lsp_message(diagnostic: &Diagnostic) -> String {
    if diagnostic.sections.is_empty() {
        return diagnostic.message.clone();
    }

    let mut message = diagnostic.message.clone();
    for section in &diagnostic.sections {
        message.push_str("\n\n");
        match section {
            DiagnosticSection::Context(text) => message.push_str(text),
            DiagnosticSection::List { heading, items } => {
                message.push_str(heading);
                for item in items {
                    message.push_str("\n  ");
                    message.push_str(item);
                }
            }
            DiagnosticSection::Hint(text) => {
                message.push_str("Hint: ");
                message.push_str(text);
            }
        }
    }
    message
}

pub fn to_lsp_diagnostic(
    diagnostic: &Diagnostic,
    line_index: &LineIndex,
    uri: &lsp_types::Uri,
    encoding: &lsp_types::PositionEncodingKind,
) -> Option<lsp_types::Diagnostic> {
    let to_position =
        |offset: u32| offset_to_lsp_position(line_index, TextSize::from(offset), encoding);

    let start = to_position(diagnostic.span.start)?;
    let end = to_position(diagnostic.span.end)?;
    let range = Range { start, end };

    let severity = match diagnostic.severity {
        Severity::Error => DiagnosticSeverity::Error,
        Severity::Warning => DiagnosticSeverity::Warning,
    };

    let related_information = diagnostic.related.iter().filter_map(|related| {
        let start = to_position(related.span.start)?;
        let end = to_position(related.span.end)?;
        Some(DiagnosticRelatedInformation {
            location: Location { uri: uri.clone(), range: Range { start, end } },
            message: related.message.clone(),
        })
    });

    let related_information = related_information.collect_vec();

    Some(lsp_types::Diagnostic {
        range,
        severity: Some(severity),
        code: Some(Code::String(diagnostic.code.to_string())),
        code_description: None,
        source: Some(format!("analyzer/{}", diagnostic.source)),
        message: Message::String(format_lsp_message(diagnostic)),
        related_information: if related_information.is_empty() {
            None
        } else {
            Some(related_information)
        },
        tags: None,
        data: None,
    })
}

#[cfg(test)]
mod tests {
    use line_index::LineIndex;
    use lsp_types::{Message, PositionEncodingKind, Uri};

    use super::{
        ANSI_CYAN, ANSI_RESET, format_rich_with_path, paint_quoted_text, to_lsp_diagnostic,
    };
    use crate::{Diagnostic, Severity, Span};

    #[test]
    fn paint_quoted_text_preserves_apostrophes_and_balances_quotes() {
        let painted = paint_quoted_text("Module's export 'foo'' cannot be used", true);
        assert_eq!(
            painted,
            format!("Module's export {ANSI_CYAN}'foo''{ANSI_RESET} cannot be used")
        );
    }

    #[test]
    fn format_rich_renders_context_list_and_hint_sections() {
        let content = "module Main where\nvalue = 1\n";
        let line_index = LineIndex::new(content);
        let diagnostic = Diagnostic::new(
            Severity::Error,
            "SampleCode",
            "Primary headline",
            Span::new(18, 23),
            "checking",
        )
        .with_context("While matching 'Int' with 'String'")
        .with_list("Matching instances:", vec!["Eq Int".to_owned(), "Eq a".to_owned()])
        .with_hint("Add an explicit type signature.");

        let rendered =
            format_rich_with_path(&[diagnostic], content, &line_index, "Main.purs", false);
        assert_eq!(
            rendered,
            "\
Error! · [SampleCode] · Main.purs:2:1
  •
  │
  • Primary headline
  │
  │   value = 1
  │   ╰────
  │
  • While matching 'Int' with 'String'
  │
  • Matching instances:
  │   Eq Int
  │   Eq a
  │
  • Hint: Add an explicit type signature.
  •

"
        );
    }

    #[test]
    fn lsp_diagnostic_appends_structured_sections() {
        let content = "module Main where\nvalue = 1\n";
        let line_index = LineIndex::new(content);
        let uri: Uri = "file:///Main.purs".parse().unwrap();
        let diagnostic = Diagnostic::new(
            Severity::Error,
            "SampleCode",
            "Primary headline",
            Span::new(18, 23),
            "checking",
        )
        .with_context("Inferred type: 'Int'")
        .with_list("In scope:", vec!["a :: Int".to_owned()])
        .with_hint("Add an explicit type signature.");

        let lsp = to_lsp_diagnostic(&diagnostic, &line_index, &uri, &PositionEncodingKind::UTF16)
            .unwrap();
        assert_eq!(
            lsp.message,
            Message::String(
                "Primary headline\n\nInferred type: 'Int'\n\nIn scope:\n  a :: Int\n\nHint: Add an explicit type signature."
                    .to_owned()
            )
        );
    }
}
