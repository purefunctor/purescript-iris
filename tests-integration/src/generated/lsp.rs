pub mod render;

use std::fmt::Write;

use building::QueryEngine;
use files::{FileId, Files};
use iris_analysis as analyzer;
use iris_analysis::completion::SuggestionsCache;
use iris_analysis::position::PositionEncoding;
use iris_analysis::{AnalyzerCapabilities, AnalyzerHost};
use itertools::Itertools;
use line_index::{LineIndex, TextSize};
use lsp_types::{
    CodeActionContext, CodeActionKind, CodeActionOrCommand, CodeActionResponse,
    CodeActionTriggerKind, CompletionItemKind, CompletionList, CompletionResponse, DocumentChanges,
    DocumentHighlight, DocumentSymbolResponse, GotoDefinitionResponse, HoverContents,
    LanguageString, Location, MarkedString, NumberOrString, OneOf, Position, PrepareRenameResponse,
    Range, SemanticTokens, SymbolInformation, TextEdit, Url, WorkspaceEdit,
    WorkspaceSymbolResponse,
};
use render::{TabledCompletionItem, TabledDetailedCompletionItem};
use similar::TextDiff;
use syntax::ast::AstNode;
use syntax::{SyntaxKind, TokenAtOffset, cst};
use tabled::Table;
use tabled::settings::{Padding, Style};

struct IntegrationAnalyzerHost<'a> {
    queries: &'a QueryEngine,
    files: &'a Files,
}

impl AnalyzerHost for IntegrationAnalyzerHost<'_> {
    type Queries = QueryEngine;

    fn queries(&self) -> &QueryEngine {
        self.queries
    }

    fn file_id(&self, uri: &str) -> Option<FileId> {
        self.files.id(uri)
    }

    fn file_uri(&self, file_id: FileId) -> Result<Option<Url>, url::ParseError> {
        let uri = self.files.path(file_id);
        Url::parse(&uri).map(Some)
    }

    fn active_files(&self) -> impl Iterator<Item = FileId> + '_ {
        self.files.iter_id()
    }

    fn is_editable(&self, _file_id: FileId) -> bool {
        true
    }
}

#[derive(Debug, Clone, Copy)]
enum CursorKind {
    GotoDefinition,
    Hover,
    Completion,
    CompletionCached,
    References,
    Rename,
    PrepareRename,
    DocumentHighlight,
    DocumentSymbols,
    CodeAction,
}

impl CursorKind {
    const CHARACTERS: &[char] = &['@', '$', '^', '~', '%', '/', '?', '!', '&', '.'];

    fn parse(text: &str) -> Option<CursorKind> {
        match text {
            "@" => Some(CursorKind::GotoDefinition),
            "$" => Some(CursorKind::Hover),
            "^" => Some(CursorKind::Completion),
            "~" => Some(CursorKind::CompletionCached),
            "%" => Some(CursorKind::References),
            "/" => Some(CursorKind::Rename),
            "?" => Some(CursorKind::PrepareRename),
            "&" => Some(CursorKind::DocumentHighlight),
            "!" => Some(CursorKind::DocumentSymbols),
            "." => Some(CursorKind::CodeAction),
            _ => None,
        }
    }

    fn valid(c: char) -> bool {
        CursorKind::CHARACTERS.contains(&c)
    }
}

fn cursor_marker_line(line: &str) -> bool {
    let Some(markers) = line.strip_prefix("--") else { return false };
    markers.chars().all(|character| character.is_whitespace() || CursorKind::valid(character))
}

enum Request {
    Cursor(Position, CursorKind),
    Diagnostics,
    SemanticTokens,
    WorkspaceSymbols(String),
}

const DIAGNOSTICS_DIRECTIVE: &str = "-- diagnostics";
const SEMANTIC_TOKENS_DIRECTIVE: &str = "-- semantic tokens";
const WORKSPACE_SYMBOLS_DIRECTIVE: &str = "-- #";

fn extract_cursors(content: &str) -> Vec<(usize, Request)> {
    let line_index = LineIndex::new(content);
    let mut cursors = vec![];

    for (index, text) in content.match_indices(CursorKind::valid) {
        let line_col = line_index.line_col(TextSize::new(index as u32));
        let line_range = line_index.line(line_col.line).unwrap();
        if !cursor_marker_line(&content[line_range]) {
            continue;
        }

        let line = line_col.line - 1;
        let character = line_col.col;
        let position = Position::new(line, character);
        let Some(kind) = CursorKind::parse(text) else { continue };

        cursors.push((index, Request::Cursor(position, kind)));
    }

    cursors
}

fn extract_workspace_symbol_queries(content: &str) -> Vec<(usize, Request)> {
    let line_index = LineIndex::new(content);
    let mut queries = vec![];

    for (index, _) in content.match_indices(WORKSPACE_SYMBOLS_DIRECTIVE) {
        let line_col = line_index.line_col(TextSize::new(index as u32));
        let line_range = line_index.line(line_col.line).unwrap();
        let line = &content[line_range];
        if !line.starts_with(WORKSPACE_SYMBOLS_DIRECTIVE) {
            continue;
        }

        let query = line
            .strip_prefix(WORKSPACE_SYMBOLS_DIRECTIVE)
            .expect("line starts with workspace symbols directive")
            .trim()
            .to_string();

        queries.push((index, Request::WorkspaceSymbols(query)));
    }

    queries
}

fn extract_semantic_tokens_requests(content: &str) -> Vec<(usize, Request)> {
    content
        .match_indices(SEMANTIC_TOKENS_DIRECTIVE)
        .map(|(index, _)| (index, Request::SemanticTokens))
        .collect()
}

fn extract_diagnostics_requests(content: &str) -> Vec<(usize, Request)> {
    content
        .match_indices(DIAGNOSTICS_DIRECTIVE)
        .map(|(index, _)| (index, Request::Diagnostics))
        .collect()
}

fn extract_requests(content: &str) -> Vec<Request> {
    let mut requests = extract_cursors(content);
    requests.extend(extract_diagnostics_requests(content));
    requests.extend(extract_semantic_tokens_requests(content));
    requests.extend(extract_workspace_symbol_queries(content));
    requests.sort_by_key(|(index, _)| *index);
    requests.into_iter().map(|(_, request)| request).collect()
}

pub fn report(engine: &QueryEngine, files: &Files, id: FileId) -> String {
    let uri = {
        let path = files.path(id);
        Url::parse(&path).unwrap()
    };

    let content = engine.content(id).unwrap();
    let line_index = LineIndex::new(&content);
    let requests = extract_requests(&content);

    let mut suggestions_cache = SuggestionsCache::default();
    let mut symbols_cache = analyzer::symbols::WorkspaceSymbolsCache::default();
    let mut result = String::new();
    for (index, request) in requests.iter().enumerate() {
        let uri = uri.clone();

        if index > 0 {
            writeln!(result, "\n").unwrap();
        }

        match request {
            Request::Cursor(position, cursor) => {
                writeln!(result, "{cursor:#?} at {position:?}\n").unwrap();

                let line_0 = line_index.line(position.line);
                let line_1 = line_index.line(position.line + 1);
                if let Some((line_0, line_1)) = line_0.zip(line_1) {
                    let line_0 = &content[line_0];
                    let line_1 = &content[line_1];
                    writeln!(result, "```").unwrap();
                    write!(result, "{line_0}").unwrap();
                    write!(result, "{line_1}").unwrap();
                    writeln!(result, "```").unwrap();
                }
                writeln!(result).unwrap();

                if matches!(cursor, CursorKind::Completion) {
                    suggestions_cache = SuggestionsCache::default();
                }

                dispatch_cursor(
                    &mut result,
                    engine,
                    files,
                    &mut suggestions_cache,
                    *position,
                    *cursor,
                    uri,
                );
            }
            Request::WorkspaceSymbols(query) => {
                writeln!(result, "WorkspaceSymbols query {query:?}\n").unwrap();
                dispatch_workspace_symbols(&mut result, engine, files, &mut symbols_cache, query);
            }
            Request::Diagnostics => {
                writeln!(result, "Diagnostics\n").unwrap();
                dispatch_diagnostics(&mut result, engine, files, id);
            }
            Request::SemanticTokens => {
                writeln!(result, "SemanticTokens\n").unwrap();
                dispatch_semantic_tokens(&mut result, engine, files, uri, &content);
            }
        }
    }

    redact_paths(result)
}

fn dispatch_diagnostics(result: &mut String, engine: &QueryEngine, files: &Files, file_id: FileId) {
    let encoding = PositionEncoding::Utf16;
    let host = IntegrationAnalyzerHost { queries: engine, files };
    let capabilities = AnalyzerCapabilities::default();
    let context = analyzer::AnalyzerContext::new(&host, encoding, capabilities);
    let Ok(collected) = analyzer::diagnostics::implementation(&context, file_id) else {
        writeln!(result, "<error>").unwrap();
        return;
    };
    if collected.diagnostics.is_empty() {
        writeln!(result, "<empty>").unwrap();
        return;
    }

    for diagnostic in collected.diagnostics {
        let code = match diagnostic.code {
            Some(NumberOrString::Number(code)) => code.to_string(),
            Some(NumberOrString::String(code)) => code,
            None => "<no code>".to_owned(),
        };
        let source = diagnostic.source.as_deref().unwrap_or("<no source>");
        writeln!(
            result,
            "{} {source} [{code}] {}",
            render_range(diagnostic.range),
            diagnostic.message,
        )
        .unwrap();
    }
}

fn dispatch_semantic_tokens(
    result: &mut String,
    engine: &QueryEngine,
    files: &Files,
    uri: Url,
    content: &str,
) {
    let encoding = PositionEncoding::Utf16;
    let host = IntegrationAnalyzerHost { queries: engine, files };
    let capabilities = AnalyzerCapabilities::default().with_change_annotations();
    let context = analyzer::AnalyzerContext::new(&host, encoding, capabilities);
    let Ok(Some(SemanticTokens { data, .. })) =
        analyzer::semantic_tokens::implementation(&context, uri)
    else {
        writeln!(result, "<empty>").unwrap();
        return;
    };

    let mut line = 0;
    let mut start = 0;
    let positions = analyzer::position::PositionConverter::new(content, encoding);
    for token in data {
        line += token.delta_line;
        start = if token.delta_line == 0 { start + token.delta_start } else { token.delta_start };

        let start_position = Position::new(line, start);
        let end_position = Position::new(line, start + token.length);
        let token_text = positions
            .protocol_position_to_utf8(start_position)
            .zip(positions.protocol_position_to_utf8(end_position))
            .and_then(|(start, end)| {
                let start = positions.utf8_position_to_offset(start)?;
                let end = positions.utf8_position_to_offset(end)?;
                content.get(usize::from(start)..usize::from(end))
            })
            .unwrap_or("<invalid range>");

        let token_type = &analyzer::semantic_tokens::TOKEN_TYPES[token.token_type as usize];
        let modifiers = analyzer::semantic_tokens::TOKEN_MODIFIERS
            .iter()
            .enumerate()
            .filter_map(|(index, modifier)| {
                let bit = 1 << index;
                (token.token_modifiers_bitset & bit != 0).then(|| modifier.as_str())
            })
            .join(", ");
        let modifiers =
            if modifiers.is_empty() { String::new() } else { format!(" [{modifiers}]") };

        writeln!(
            result,
            "{line}:{start}..{} {}{modifiers} {token_text:?}",
            start + token.length,
            token_type.as_str(),
        )
        .unwrap();
    }
}

fn render_location(location: Location) -> String {
    format!(
        "{} @ {}:{}..{}:{}",
        location.uri,
        location.range.start.line,
        location.range.start.character,
        location.range.end.line,
        location.range.end.character,
    )
}

fn render_range(range: Range) -> String {
    format!(
        "{}:{}..{}:{}",
        range.start.line, range.start.character, range.end.line, range.end.character,
    )
}

fn render_text_edit(edit: TextEdit) -> String {
    format!(
        "{}:{}..{}:{} => {:?}",
        edit.range.start.line,
        edit.range.start.character,
        edit.range.end.line,
        edit.range.end.character,
        edit.new_text,
    )
}

fn render_workspace_edit(edit: WorkspaceEdit) -> Vec<String> {
    let mut result = vec![];

    if let Some(changes) = edit.changes {
        for edits in changes.into_values() {
            result.extend(edits.into_iter().map(render_text_edit));
        }
    }

    result.sort();
    result
}

fn assert_rename_annotations(edit: &WorkspaceEdit) {
    let Some(annotations) = edit.change_annotations.as_ref() else { return };
    assert!(edit.changes.is_none(), "annotated rename edits must not also use changes");
    assert!(!annotations.is_empty(), "annotated rename edits must define an annotation");
    assert!(
        annotations.values().all(|annotation| annotation.needs_confirmation == Some(true)),
        "rename change annotations must require confirmation"
    );

    let Some(DocumentChanges::Edits(documents)) = edit.document_changes.as_ref() else {
        panic!("annotated rename edits must use text document edits");
    };
    assert!(!documents.is_empty(), "annotated rename edits must edit a document");
    for document in documents {
        assert!(!document.edits.is_empty(), "annotated rename documents must contain an edit");
        for edit in &document.edits {
            let OneOf::Right(edit) = edit else {
                panic!("every edit in an annotated rename must reference an annotation");
            };
            assert!(
                annotations.contains_key(&edit.annotation_id),
                "annotated rename edit references an unknown annotation"
            );
        }
    }
}

fn render_rename_edit(edit: WorkspaceEdit, files: &Files, encoding: PositionEncoding) -> String {
    assert_rename_annotations(&edit);
    let annotation = edit.change_annotations.and_then(|annotations| {
        annotations.into_values().next().map(|annotation| {
            let confirmation = if annotation.needs_confirmation == Some(true) {
                " (confirmation required)"
            } else {
                ""
            };
            format!("{}{}", annotation.label, confirmation)
        })
    });

    let changes = if let Some(changes) = edit.changes {
        changes
    } else if let Some(DocumentChanges::Edits(documents)) = edit.document_changes {
        let documents = documents.into_iter().map(|document| {
            let edits = document.edits.into_iter().map(|edit| match edit {
                OneOf::Left(edit) => edit,
                OneOf::Right(edit) => edit.text_edit,
            });
            let edits = edits.collect();
            (document.text_document.uri, edits)
        });
        documents.collect()
    } else {
        return String::new();
    };

    let mut changes = changes.into_iter().collect::<Vec<_>>();
    changes.sort_by(|(left, _), (right, _)| left.as_str().cmp(right.as_str()));

    let mut rendered = changes.into_iter().map(|(uri, edits)| {
        let file_id = files.id(uri.as_str()).expect("rename edit references a loaded file");
        let content = files.content(file_id);
        let changed = apply_text_edits(&content, edits, encoding);
        let file_name = uri
            .to_file_path()
            .ok()
            .and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
            .unwrap_or_else(|| uri.to_string());

        let diff = TextDiff::from_lines(content.as_ref(), changed.as_str());
        let diff = diff.unified_diff().header(&file_name, &file_name).to_string();
        let mut diff = diff.lines().map(|line| if line == " " { "" } else { line });
        let diff = diff.join("\n");

        format!("```diff\n{diff}\n```")
    });

    let rendered = rendered.join("\n\n");
    if let Some(annotation) = annotation { format!("{annotation}\n\n{rendered}") } else { rendered }
}

fn apply_text_edits(content: &str, edits: Vec<TextEdit>, encoding: PositionEncoding) -> String {
    let positions = analyzer::position::PositionConverter::new(content, encoding);
    let edits = edits.into_iter().map(|edit| {
        let start = positions
            .protocol_position_to_utf8(edit.range.start)
            .and_then(|position| positions.utf8_position_to_offset(position))
            .expect("rename edit starts at a valid source position");
        let end = positions
            .protocol_position_to_utf8(edit.range.end)
            .and_then(|position| positions.utf8_position_to_offset(position))
            .expect("rename edit ends at a valid source position");

        (usize::from(start), usize::from(end), edit.new_text)
    });
    let mut edits = edits.collect::<Vec<_>>();
    edits.sort_by_key(|(start, end, _)| (*start, *end));

    let mut result = String::with_capacity(content.len());
    let mut cursor = 0;

    for (start, end, new_text) in edits {
        assert!(cursor <= start, "rename edits must not overlap");
        result.push_str(&content[cursor..start]);
        result.push_str(&new_text);
        cursor = end;
    }

    result.push_str(&content[cursor..]);
    result
}

fn render_code_action_response(response: CodeActionResponse) -> String {
    let mut result = vec![];

    for action in response {
        match action {
            CodeActionOrCommand::CodeAction(action) => {
                let kind = action.kind.as_ref().map(CodeActionKind::as_str).unwrap_or("<none>");

                if let Some(edit) = action.edit {
                    let edits = render_workspace_edit(edit);
                    if edits.is_empty() {
                        result.push(format!("{} [{kind}] <no edit>", action.title));
                    } else {
                        for edit in edits {
                            result.push(format!("{} [{kind}] {edit}", action.title));
                        }
                    }
                } else {
                    result.push(format!("{} [{kind}] <no edit>", action.title));
                }
            }
            CodeActionOrCommand::Command(command) => {
                result.push(format!("{} [command:{}]", command.title, command.command));
            }
        }
    }

    if result.is_empty() { "<empty>".to_string() } else { result.join("\n") }
}

fn dispatch_cursor(
    result: &mut String,
    engine: &QueryEngine,
    files: &Files,
    cache: &mut SuggestionsCache,
    position: Position,
    cursor: CursorKind,
    uri: Url,
) {
    let encoding = PositionEncoding::Utf16;
    let host = IntegrationAnalyzerHost { queries: engine, files };
    let capabilities = AnalyzerCapabilities::default().with_change_annotations();
    let context = analyzer::AnalyzerContext::new(&host, encoding, capabilities);

    match cursor {
        CursorKind::GotoDefinition => {
            if let Ok(Some(response)) =
                analyzer::definition::implementation(&context, uri, position)
            {
                match response {
                    GotoDefinitionResponse::Scalar(location) => {
                        let location = render_location(location);
                        writeln!(result, "{location}").unwrap();
                    }
                    GotoDefinitionResponse::Array(location) => {
                        let location = location.into_iter().map(render_location).join("\n");
                        writeln!(result, "{location}").unwrap();
                    }
                    GotoDefinitionResponse::Link(_) => (),
                }
            } else {
                writeln!(result, "<empty>").unwrap();
            }
        }
        CursorKind::Hover => {
            let file_id = host.file_id(uri.as_str()).expect("hover URI references a loaded file");
            let content = engine.content(file_id).unwrap();
            let positions = analyzer::position::PositionConverter::new(&content, encoding);
            if let Ok(Some(response)) = analyzer::hover::implementation(&context, uri, position) {
                let convert = |marked: MarkedString| -> String {
                    match marked {
                        MarkedString::String(string) => string,
                        MarkedString::LanguageString(LanguageString {
                            language, value, ..
                        }) => format!("```{language}\n{value}\n```"),
                    }
                };

                let range = response.range.and_then(|range| {
                    positions
                        .protocol_position_to_utf8(range.start)
                        .zip(positions.protocol_position_to_utf8(range.end))
                        .and_then(|(start, end)| {
                            let start = positions.utf8_position_to_offset(start)?;
                            let end = positions.utf8_position_to_offset(end)?;
                            content.get(usize::from(start)..usize::from(end))
                        })
                });
                if let Some(range) = range {
                    writeln!(result, "Range: {range:?}\n").unwrap();
                } else {
                    writeln!(result, "Range: <none>\n").unwrap();
                }

                match response.contents {
                    HoverContents::Scalar(marked) => {
                        let marked = convert(marked);
                        if marked.is_empty() {
                            writeln!(result, "<empty>").unwrap();
                        } else {
                            writeln!(result, "{marked}").unwrap();
                        }
                    }
                    HoverContents::Array(marked) => {
                        let marked = marked.into_iter().map(convert).join("\n");
                        if marked.is_empty() {
                            writeln!(result, "<empty>").unwrap();
                        } else {
                            writeln!(result, "{marked}").unwrap();
                        }
                    }
                    HoverContents::Markup(markup) => {
                        if markup.value.is_empty() {
                            writeln!(result, "<empty>").unwrap();
                        } else {
                            writeln!(result, "{}", markup.value).unwrap();
                        }
                    }
                }
            } else {
                writeln!(result, "<empty>").unwrap();
            }
        }
        CursorKind::CodeAction => {
            let range = Range::new(position, position);
            let action_context = CodeActionContext {
                diagnostics: vec![],
                only: Some(vec![CodeActionKind::QUICKFIX]),
                trigger_kind: Some(CodeActionTriggerKind::INVOKED),
            };

            if let Ok(Some(response)) =
                analyzer::code_action::implementation(&context, uri, range, action_context)
            {
                writeln!(result, "{}", render_code_action_response(response)).unwrap();
            } else {
                writeln!(result, "<empty>").unwrap();
            }
        }
        CursorKind::Completion | CursorKind::CompletionCached => {
            if let Ok(Some(response)) =
                analyzer::completion::implementation(&context, cache, uri, position)
            {
                match response {
                    CompletionResponse::Array(items)
                    | CompletionResponse::List(CompletionList { items, .. }) => {
                        let items: Vec<_> = items
                            .into_iter()
                            .filter_map(|item| {
                                analyzer::completion::resolve::implementation(engine, item).ok()
                            })
                            .collect();

                        let has_values =
                            items.iter().any(|item| item.kind == Some(CompletionItemKind::VALUE));

                        let mut table = if has_values {
                            let items: Vec<TabledDetailedCompletionItem> =
                                items.into_iter().map(TabledDetailedCompletionItem::from).collect();
                            Table::new(items)
                        } else {
                            let items: Vec<TabledCompletionItem> =
                                items.into_iter().map(TabledCompletionItem::from).collect();
                            Table::new(items)
                        };
                        table.with(Style::modern_rounded());
                        table.with(Padding::new(2, 2, 0, 0));

                        writeln!(result, "{table}").unwrap();
                    }
                }
            } else {
                writeln!(result, "<empty>").unwrap();
            }
        }
        CursorKind::DocumentSymbols => {
            if let Ok(Some(response)) = analyzer::symbols::document(&context, uri) {
                writeln!(result, "{}", render_document_symbols_response(response)).unwrap();
            } else {
                writeln!(result, "<empty>").unwrap();
            }
        }
        CursorKind::References => {
            if let Ok(Some(location)) =
                analyzer::references::implementation(&context, uri, position)
            {
                let location = location.into_iter().map(render_location).join("\n");
                writeln!(result, "{location}").unwrap();
            } else {
                writeln!(result, "<empty>").unwrap();
            }
        }
        CursorKind::Rename => {
            let Some(new_name) = rename_target_name(engine, files, uri.clone(), position, encoding)
            else {
                writeln!(result, "<empty>").unwrap();
                return;
            };
            let host = IntegrationAnalyzerHost { queries: engine, files };
            let capabilities = AnalyzerCapabilities::default().with_change_annotations();
            let context = analyzer::AnalyzerContext::new(&host, encoding, capabilities);
            let response =
                analyzer::rename::implementation(&context, uri.clone(), position, new_name.clone());
            if let Ok(Some(edit)) = response {
                let annotated = edit.change_annotations.is_some();
                let edit = render_rename_edit(edit, files, encoding);
                writeln!(result, "{edit}").unwrap();
                if annotated {
                    let context = analyzer::AnalyzerContext::new(
                        &host,
                        encoding,
                        AnalyzerCapabilities::default(),
                    );
                    let response =
                        analyzer::rename::implementation(&context, uri, position, new_name);
                    if let Err(error) = response {
                        writeln!(result, "\nWithout change annotations: {error}").unwrap();
                    }
                }
            } else {
                writeln!(result, "<empty>").unwrap();
            }
        }
        CursorKind::PrepareRename => match analyzer::rename::prepare(&context, uri, position) {
            Ok(Some(PrepareRenameResponse::Range(range))) => {
                writeln!(result, "{}", render_range(range)).unwrap();
            }
            Ok(Some(PrepareRenameResponse::RangeWithPlaceholder { range, placeholder })) => {
                writeln!(result, "{} {placeholder}", render_range(range)).unwrap();
            }
            Ok(Some(PrepareRenameResponse::DefaultBehavior { default_behavior })) => {
                writeln!(result, "default behavior: {default_behavior}").unwrap();
            }
            Ok(None) | Err(_) => {
                writeln!(result, "<empty>").unwrap();
            }
        },
        CursorKind::DocumentHighlight => {
            let render_highlight = |h: DocumentHighlight| -> String {
                format!(
                    "{}:{}..{}:{}",
                    h.range.start.line,
                    h.range.start.character,
                    h.range.end.line,
                    h.range.end.character
                )
            };

            if let Ok(Some(highlights)) =
                analyzer::document_highlight::implementation(&context, uri, position)
            {
                let highlights = highlights.into_iter().map(render_highlight).join("\n");
                writeln!(result, "{highlights}").unwrap();
            } else {
                writeln!(result, "<empty>").unwrap();
            }
        }
    }
}

fn rename_target_name(
    engine: &QueryEngine,
    files: &Files,
    uri: Url,
    position: Position,
    encoding: PositionEncoding,
) -> Option<String> {
    let file_id = files.id(uri.as_str())?;
    let content = engine.content(file_id).ok()?;
    let positions = analyzer::position::PositionConverter::new(&content, encoding);
    let position = positions.protocol_position_to_utf8(position)?;
    let offset = positions.utf8_position_to_offset(position)?;
    let (parsed, _) = engine.parsed(file_id).ok()?;
    let root = parsed.syntax_node();
    let token = match root.token_at_offset(offset) {
        TokenAtOffset::None => return None,
        TokenAtOffset::Single(token) => token,
        TokenAtOffset::Between(_, right) => right,
    };

    if token.parent().kind() == SyntaxKind::Qualifier {
        return Some("Renamed".to_string());
    }
    if token.parent_ancestors().any(|node| cst::RecordPun::can_cast(node.kind())) {
        return Some("renamed".to_string());
    }

    match token.kind() {
        SyntaxKind::LOWER => Some("renamed".to_string()),
        SyntaxKind::UPPER => Some("Renamed".to_string()),
        SyntaxKind::OPERATOR
        | SyntaxKind::OPERATOR_NAME
        | SyntaxKind::COLON
        | SyntaxKind::DOUBLE_PERIOD
        | SyntaxKind::DOUBLE_PERIOD_OPERATOR_NAME
        | SyntaxKind::MINUS
        | SyntaxKind::LEFT_THICK_ARROW => Some("<~>".to_string()),
        _ => None,
    }
}

fn dispatch_workspace_symbols(
    result: &mut String,
    engine: &QueryEngine,
    files: &Files,
    cache: &mut analyzer::symbols::WorkspaceSymbolsCache,
    query: &str,
) {
    let encoding = PositionEncoding::Utf16;
    let host = IntegrationAnalyzerHost { queries: engine, files };
    let capabilities = AnalyzerCapabilities::default().with_change_annotations();
    let context = analyzer::AnalyzerContext::new(&host, encoding, capabilities);

    match analyzer::symbols::workspace(&context, cache, query) {
        Ok(Some(WorkspaceSymbolResponse::Flat(symbols))) => {
            let mut lines = symbols
                .into_iter()
                .map(|symbol| {
                    let location = render_location(symbol.location);
                    format!("{} {:?} {location}", symbol.name, symbol.kind)
                })
                .collect_vec();

            if lines.is_empty() {
                writeln!(result, "<empty>").unwrap();
            } else {
                lines.sort();
                writeln!(result, "{}", lines.join("\n")).unwrap();
            }
        }
        Ok(Some(_)) => {
            writeln!(result, "<unsupported>").unwrap();
        }
        Ok(None) => {
            writeln!(result, "<none>").unwrap();
        }
        Err(_) => {
            writeln!(result, "<empty>").unwrap();
        }
    }
}

fn render_document_symbols_response(response: DocumentSymbolResponse) -> String {
    match response {
        DocumentSymbolResponse::Flat(symbols) => {
            if symbols.is_empty() {
                "<empty>".into()
            } else {
                symbols.into_iter().map(render_symbol_information).join("\n")
            }
        }
        DocumentSymbolResponse::Nested(_) => "<nested>".into(),
    }
}

fn render_symbol_information(symbol: SymbolInformation) -> String {
    let SymbolInformation { name, kind, location, .. } = symbol;
    format!(
        "{name} :: {kind:?} @ {}:{}..{}:{}",
        location.range.start.line,
        location.range.start.character,
        location.range.end.line,
        location.range.end.character,
    )
}

fn redact_paths(mut result: String) -> String {
    let manifest_directory = env!("CARGO_MANIFEST_DIR");
    let temporary_directory = crate::PRIM_DIRECTORY.path();

    let manifest_directory_url = url::Url::from_file_path(manifest_directory).unwrap();
    let temporary_directory_url = url::Url::from_file_path(temporary_directory).unwrap();

    for (url, redacted) in [
        (manifest_directory_url, "file:///tests-integration"),
        (temporary_directory_url, "file:///temporary-directory"),
    ] {
        let uri = url.to_string();
        result = result.replace(&uri, redacted);
    }

    result
}
