use std::fmt::Write;

use building::QueryEngine;
use files::FileId;
use javascript::Position;
use line_index::{LineIndex, WideEncoding, WideLineCol};

use crate::fixtures::FixtureResult;

pub fn report(engine: &QueryEngine, id: FileId) -> FixtureResult<String> {
    let module = engine.javascript(id)??;
    let source_map = engine.source_map(id)??;
    let content = engine.content(id)?;
    let line_index = engine.line_index(id)?;
    let generated = module.files().index_source();
    let width = generated.lines().count().to_string().len();
    let mut mappings = source_map.mappings().iter().peekable();
    let mut report = String::new();
    for (line, text) in (0..).zip(generated.lines()) {
        let number = line + 1;
        writeln!(report, "{}", format!("{number:>width$} | {text}").trim_end())?;
        while let Some(mapping) = mappings.next_if(|mapping| mapping.generated.line == line) {
            let indent = " ".repeat(mapping.generated.column as usize);
            let name = mapping.name.as_ref().map(|name| format!("({name}) ")).unwrap_or_default();
            let source = source_excerpt(&content, &line_index, mapping.source);
            writeln!(report, "{:width$} | {indent}^ {name}{source}", "")?;
        }
    }
    Ok(report)
}

fn source_excerpt(content: &str, line_index: &LineIndex, source: Option<Position>) -> String {
    let Some(Position { line, column }) = source else {
        return "unmapped".to_string();
    };
    let position = WideLineCol { line, col: column };
    let offset = line_index.to_utf8(WideEncoding::Utf16, position);
    let offset = offset.and_then(|position| line_index.offset(position));
    let offset = offset.expect("invariant violated: source map position is outside its source");
    let text = content[usize::from(offset)..].lines().next().unwrap_or_default();
    format!("{}:{} {}", line + 1, column + 1, text.trim_end())
}
