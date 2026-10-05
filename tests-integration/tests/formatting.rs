use std::collections::BTreeMap;
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use formatting::{Config, FormatError};
use syntax::ast::{AstNode, support};
use syntax::{SyntaxKind, WalkEvent, cst};

fn formatting(path: &Path) -> datatest_stable::Result<()> {
    let input = fs::read_to_string(path)?;
    let mut source = input.as_str();
    let mut configurations = Vec::new();
    let mut incomplete = None;
    while let Some(header) = source.strip_prefix("-- @format ") {
        let (header, remaining) = header.split_once('\n').unwrap_or((header, ""));
        source = remaining;
        let mut config = Config::default();
        let mut crlf = false;
        for parameter in header.split_whitespace() {
            let (name, value) = parameter
                .split_once('=')
                .ok_or_else(|| format!("expected name=value in formatting header: {parameter}"))?;
            match name {
                "width" => config.line_width = value.parse()?,
                "indent" => config.indent_width = value.parse()?,
                "unicode" => config.unicode = value.parse()?,
                "crlf" => crlf = value.parse()?,
                "incomplete" => incomplete = Some(value.parse()?),
                _ => return Err(format!("unknown formatting parameter: {name}").into()),
            }
        }
        configurations.push((config, crlf));
    }
    if configurations.is_empty() {
        configurations.push((Config::default(), false));
    }
    let lexed = lexing::lex(source);
    let layout = lexing::layout(&lexed);
    let (parsed, errors) = parsing::parse(&lexed, &layout);
    if incomplete.is_some() {
        assert!(errors.is_empty(), "the fixture must exercise syntax without parser errors");
    }
    let expected = syntax_structure(parsed.syntax_node());
    let config = Config::default();
    let assert_semantics = if errors.is_empty()
        && !matches!(
            formatting::format_with_config(source, &config),
            Err(FormatError::InvalidSource(_))
        ) {
        let assert_semantics = semantic_preservation(path)?;
        comment_boundaries(source, &lexed, &layout, &expected, &assert_semantics)?;
        Some(assert_semantics)
    } else {
        None
    };
    let multiple = configurations.len() > 1;
    let mut report = String::new();
    for (config, crlf) in configurations {
        let input = if crlf {
            source.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            source.to_owned()
        };
        let formatted = match formatting::format_with_config(&input, &config) {
            Ok(formatted) => {
                assert_ne!(incomplete, Some(true), "incomplete syntax must not be formatted");
                assert_semantics.as_ref().unwrap()(&input, &formatted)?;
                let candidate = lexing::lex(&formatted);
                assert_tokens(&lexing::lex(&input), &candidate, &config);
                assert_spacing(&candidate);
                let (candidate, errors) = parsing::parse(&candidate, &lexing::layout(&candidate));
                assert!(errors.is_empty());
                assert_eq!(expected, syntax_structure(candidate.syntax_node()));
                assert_eq!(
                    formatting::format_with_config(&formatted, &config)?,
                    formatted,
                    "formatter must be idempotent"
                );
                assert_eq!(
                    formatting::format_with_config(input.trim_end_matches(['\r', '\n']), &config)?,
                    formatted,
                    "a missing final newline must not change comment indentation"
                );
                if crlf {
                    assert_eq!(
                        formatting::format_with_config(source, &config)?,
                        formatted,
                        "CRLF whitespace must normalize to LF"
                    );
                }
                for line_width in [10, 20, 40, 80, 120] {
                    let other =
                        formatting::format_with_config(&input, &Config { line_width, ..config })?;
                    assert_semantics.as_ref().unwrap()(&input, &other)?;
                    assert_eq!(formatting::format_with_config(&other, &config)?, formatted);
                }
                formatted
            }
            Err(error) => {
                assert!(
                    !matches!(error, FormatError::ChangedSyntax),
                    "valid source must retain its syntax after formatting"
                );
                if let Some(incomplete) = incomplete {
                    assert!(incomplete, "complete syntax must be formatted: {error}");
                    assert!(matches!(error, FormatError::InvalidSource(_)));
                }
                error.to_string()
            }
        };
        if multiple {
            let Config { line_width, indent_width, unicode } = config;
            let spelling = if unicode { ", unicode" } else { "" };
            report.push_str(&format!(
                "=== width {line_width}, indent {indent_width}{spelling} ===\n{formatted}\n"
            ));
        } else {
            report.push_str(&formatted);
        }
    }
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path(Path::new(env!("CARGO_MANIFEST_DIR")).join(path.parent().unwrap()));
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.bind(|| insta::assert_snapshot!(path.file_stem().unwrap().to_str().unwrap(), report));
    Ok(())
}

fn semantic_preservation(
    path: &Path,
) -> datatest_stable::Result<impl Fn(&str, &str) -> datatest_stable::Result<()>> {
    let (engine, files) = tests_integration::load_compiler(path.parent().unwrap())?;
    let absolute_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    let uri = url::Url::from_file_path(absolute_path).unwrap();
    let id = files.id(uri.as_str()).unwrap();

    Ok(move |source: &str, formatted: &str| {
        engine.set_content(id, source);
        let indexed = engine.indexed(id)?;
        let resolved = engine.resolved(id)?;
        let lowered = engine.lowered(id)?;
        // Import IDs follow CST order. A witness in the observed output order lets
        // us check semantics across reordering, then retain exact equality and cache
        // reuse checks for the printer's whitespace changes.
        let (_, reordered) = import_permutation(&lexing::lex(source), &lexing::lex(formatted));
        let (indexed, resolved, lowered) = if let Some(reordered) = reordered {
            engine.set_content(id, reordered.as_str());
            let reordered_indexed = engine.indexed(id)?;
            let reordered_resolved = engine.resolved(id)?;
            let reordered_lowered = engine.lowered(id)?;
            assert_eq!(indexed.kind, reordered_indexed.kind);
            assert_eq!(indexed.names, reordered_indexed.names);
            assert_eq!(indexed.exports, reordered_indexed.exports);
            assert_eq!(indexed.items, reordered_indexed.items);
            assert_eq!(indexed.pairs, reordered_indexed.pairs);
            assert_eq!(indexed.errors.len(), reordered_indexed.errors.len());
            assert_eq!(resolved.locals, reordered_resolved.locals);
            assert_eq!(resolved.class, reordered_resolved.class);
            assert_eq!(resolved.errors.len(), reordered_resolved.errors.len());
            assert_eq!(
                exported_items(resolved.exports.iter_terms()),
                exported_items(reordered_resolved.exports.iter_terms()),
            );
            assert_eq!(
                exported_items(resolved.exports.iter_types()),
                exported_items(reordered_resolved.exports.iter_types()),
            );
            assert_eq!(
                exported_items(resolved.exports.iter_classes()),
                exported_items(reordered_resolved.exports.iter_classes()),
            );
            assert_eq!(lowered, reordered_lowered, "import sorting must preserve source semantics");
            assert!(
                Arc::ptr_eq(&lowered, &reordered_lowered),
                "import sorting must reuse the semantic cache"
            );
            (reordered_indexed, reordered_resolved, reordered_lowered)
        } else {
            (indexed, resolved, lowered)
        };
        engine.set_content(id, formatted);
        let candidate_indexed = engine.indexed(id)?;
        let candidate_resolved = engine.resolved(id)?;
        let candidate_lowered = engine.lowered(id)?;
        assert_eq!(indexed, candidate_indexed, "formatting must preserve declarations");
        assert_eq!(resolved, candidate_resolved, "formatting must preserve name resolution");
        assert_eq!(lowered, candidate_lowered, "formatting must preserve source semantics");
        assert!(Arc::ptr_eq(&indexed, &candidate_indexed), "the declaration cache must be reused");
        assert!(Arc::ptr_eq(&resolved, &candidate_resolved), "the resolution cache must be reused");
        assert!(Arc::ptr_eq(&lowered, &candidate_lowered), "the semantic cache must be reused");
        Ok(())
    })
}

fn exported_items<Name: Ord, Identifier>(
    items: impl Iterator<Item = (Name, files::FileId, Identifier)>,
) -> BTreeMap<Name, (files::FileId, Identifier)> {
    items.map(|(name, file, item)| (name, (file, item))).collect()
}

fn syntax_structure(root: syntax::SyntaxNode) -> Vec<(bool, SyntaxKind)> {
    let events = root.preorder().filter_map(|event| match event {
        WalkEvent::Enter(node) if node.kind() != SyntaxKind::Annotation => {
            Some((true, node.kind()))
        }
        WalkEvent::Leave(node) if node.kind() != SyntaxKind::Annotation => {
            Some((false, node.kind()))
        }
        _ => None,
    });
    let mut events = events.collect::<Vec<_>>();
    if let Some(imports) = support::child::<cst::ModuleImports>(&root) {
        let mut children = imports
            .children()
            .map(|import| syntax_structure(import.syntax().clone()))
            .collect::<Vec<_>>();
        children.sort();
        let start =
            events.iter().position(|event| *event == (true, SyntaxKind::ModuleImports)).unwrap();
        let end =
            events.iter().position(|event| *event == (false, SyntaxKind::ModuleImports)).unwrap();
        events.splice(start + 1..end, children.into_iter().flatten());
    }
    events
}

fn import_ranges(lexed: &lexing::Lexed<'_>) -> Vec<Range<usize>> {
    let (parsed, errors) = parsing::parse(lexed, &lexing::layout(lexed));
    assert!(errors.is_empty());
    let module = cst::Module::cast(parsed.syntax_node()).unwrap();
    let Some(imports) = module.imports() else { return Vec::new() };
    imports
        .children()
        .map(|import| {
            let range = import.syntax().text_range();
            lexed.first_text_start_from(u32::from(range.start()))
                ..lexed.first_text_start_from(u32::from(range.end()))
        })
        .collect()
}

fn import_permutation(
    original: &lexing::Lexed<'_>,
    candidate: &lexing::Lexed<'_>,
) -> (Vec<usize>, Option<String>) {
    let imports = import_ranges(original);
    let candidate_imports = import_ranges(candidate);
    assert_eq!(imports.len(), candidate_imports.len());
    let Some(first) = imports.first() else { return ((0..original.len()).collect(), None) };
    let mut remaining = imports.clone();
    let mut selected = Vec::new();
    for candidate_import in candidate_imports {
        let position = remaining
            .iter()
            .position(|import| {
                import.len() == candidate_import.len()
                    && import.clone().zip(candidate_import.clone()).all(
                        |(original_index, candidate_index)| {
                            original.kind(original_index) == candidate.kind(candidate_index)
                                && original.qualifier(original_index)
                                    == candidate.qualifier(candidate_index)
                                && original.text(original_index) == candidate.text(candidate_index)
                        },
                    )
            })
            .expect("formatting must preserve every import, including duplicates");
        selected.push(remaining.remove(position));
    }
    assert!(remaining.is_empty());
    let mut order = (0..first.start).collect::<Vec<_>>();
    order.extend(selected.iter().flat_map(|range| range.clone()));
    order.extend(imports.last().unwrap().end..original.len());
    let reordered = order.iter().copied().ne(0..original.len()).then(|| {
        let span = |range: &Range<usize>| {
            let last = range.clone().next_back().unwrap();
            original.info(range.start).annotation as usize..original.info(last).token as usize
        };
        let mut source = String::new();
        let mut offset = 0;
        for (slot, import) in imports.iter().zip(selected.iter()) {
            let slot = span(slot);
            source.push_str(&original.source[offset..slot.start]);
            source.push_str(&original.source[span(import)]);
            offset = slot.end;
        }
        source.push_str(&original.source[offset..]);
        source
    });
    (order, reordered)
}

fn comment_anchors(
    lexed: &lexing::Lexed<'_>,
    order: Option<&[usize]>,
) -> Vec<(usize, bool, String)> {
    let mut comments = Vec::new();
    for (index, annotation) in lexed.annotations().enumerate() {
        let mut remaining = annotation.unwrap_or_default();
        while !remaining.trim_start().is_empty() {
            let text = remaining.trim_start();
            let whitespace = &remaining[..remaining.len() - text.len()];
            let end = if text.starts_with("--") {
                text.find('\n').unwrap_or(text.len())
            } else {
                assert!(text.starts_with("{-"));
                let mut depth = 1;
                let mut end = 2;
                while depth > 0 {
                    match text.as_bytes().get(end..end + 2) {
                        Some(b"{-") => {
                            depth += 1;
                            end += 2;
                        }
                        Some(b"-}") => {
                            depth -= 1;
                            end += 2;
                        }
                        _ => {
                            end += 1;
                            assert!(end <= text.len());
                        }
                    }
                }
                end
            };
            let standalone = index == 0 || whitespace.contains('\n');
            let anchor = if standalone { index } else { index - 1 };
            let anchor = order.map_or(anchor, |order| order[anchor]);
            comments.push((anchor, standalone, text[..end].split_whitespace().collect()));
            remaining = &text[end..];
        }
    }
    comments.sort_by_key(|(anchor, standalone, _)| (*anchor, *standalone));
    comments
}

fn assert_tokens(original: &lexing::Lexed<'_>, candidate: &lexing::Lexed<'_>, config: &Config) {
    assert_eq!(original.len(), candidate.len());
    let (order, _) = import_permutation(original, candidate);
    assert_eq!(
        comment_anchors(original, None),
        comment_anchors(candidate, Some(&order)),
        "comments must remain attached to their lexical anchor"
    );
    for (index, &original_index) in order.iter().enumerate() {
        let kind = original.kind(original_index);
        assert_eq!(kind, candidate.kind(index));
        assert_eq!(original.qualifier(original_index), candidate.qualifier(index));
        if !config.unicode
            || !matches!(
                kind,
                SyntaxKind::DOUBLE_COLON
                    | SyntaxKind::LEFT_ARROW
                    | SyntaxKind::RIGHT_ARROW
                    | SyntaxKind::LEFT_THICK_ARROW
                    | SyntaxKind::RIGHT_THICK_ARROW
                    | SyntaxKind::FORALL
            )
        {
            assert_eq!(original.text(original_index), candidate.text(index));
        }
    }
}

fn assert_spacing(lexed: &lexing::Lexed<'_>) {
    for index in 0..lexed.len() {
        let annotation = lexed.annotation(index).unwrap_or_default();
        if annotation.trim().is_empty() && !annotation.contains('\n') {
            assert!(
                matches!(annotation, "" | " "),
                "inline tokens must not have indentation padding: {annotation:?}"
            );
        }
        if !annotation.contains("{-")
            && let Some((lines, _)) = annotation.rsplit_once('\n')
        {
            for line in lines.split('\n') {
                if line.trim().is_empty() {
                    assert!(line.is_empty(), "blank lines must not contain padding: {line:?}");
                }
            }
        }
    }
}

fn comment_boundaries(
    source: &str,
    lexed: &lexing::Lexed<'_>,
    layout: &[SyntaxKind],
    expected: &[(bool, SyntaxKind)],
    assert_semantics: &impl Fn(&str, &str) -> datatest_stable::Result<()>,
) -> datatest_stable::Result<()> {
    let mut tested = 0;
    for index in 1..lexed.len() {
        let (before, after) = source.split_at(lexed.info(index).annotation as usize);
        let indent = " ".repeat(lexed.position(index).column as usize);
        for comment in [
            " {- boundary -} ".to_owned(),
            format!(" -- boundary\n{indent}"),
            format!(" {{- boundary\n-}}\n{indent}"),
        ] {
            let input = format!("{before}{comment}{after}");
            let candidate = lexing::lex(&input);
            let candidate_layout = lexing::layout(&candidate);
            let (candidate, errors) = parsing::parse(&candidate, &candidate_layout);
            if !errors.is_empty()
                || candidate_layout != layout
                || syntax_structure(candidate.syntax_node()) != expected
            {
                continue;
            }
            for config in [
                Config { line_width: 20, indent_width: 1, unicode: false },
                Config { line_width: 40, indent_width: 4, unicode: true },
            ] {
                let formatted =
                    formatting::format_with_config(&input, &config).map_err(|error| {
                        format!("comment before token {index}, {config:?}: {error}\n{input}")
                    })?;
                assert_semantics(&input, &formatted)?;
                let candidate = lexing::lex(&formatted);
                assert_tokens(&lexing::lex(&input), &candidate, &config);
                assert_spacing(&candidate);
                let (candidate, errors) = parsing::parse(&candidate, &lexing::layout(&candidate));
                assert!(errors.is_empty());
                assert_eq!(expected, syntax_structure(candidate.syntax_node()));
                assert_eq!(formatting::format_with_config(&formatted, &config)?, formatted);
                tested += 1;
            }
        }
    }
    assert!(tested > 0, "comment-boundary coverage must exercise valid syntax");
    Ok(())
}

fn has_missing_expression(root: syntax::SyntaxNode) -> bool {
    // The corpus includes intentional recovery inputs. Establish their missing
    // expression from the original CST, independently of the formatter's result.
    root.preorder().any(|event| {
        let WalkEvent::Enter(node) = event else { return false };
        match node.kind() {
            SyntaxKind::Unconditional | SyntaxKind::LetBindingPattern => {
                support::child::<cst::WhereExpression>(&node).is_none()
            }
            SyntaxKind::ExpressionLambda
            | SyntaxKind::ExpressionIf
            | SyntaxKind::ExpressionThen
            | SyntaxKind::ExpressionElse
            | SyntaxKind::ExpressionOperatorPair
            | SyntaxKind::RecordUpdateLeaf
            | SyntaxKind::WhereExpression => support::child::<cst::Expression>(&node).is_none(),
            SyntaxKind::RecordField => {
                node.parent().is_some_and(|parent| parent.kind() == SyntaxKind::ExpressionRecord)
                    && cst::RecordField::cast(node).unwrap().expression().is_none()
            }
            _ => false,
        }
    })
}

fn corpus(path: &Path) -> datatest_stable::Result<()> {
    let source = fs::read_to_string(path)?;
    let lexed = lexing::lex(&source);
    let layout = lexing::layout(&lexed);
    let (parsed, errors) = parsing::parse(&lexed, &layout);
    if !errors.is_empty() || has_missing_expression(parsed.syntax_node()) {
        let config = Config::default();
        assert!(matches!(
            formatting::format_with_config(&source, &config),
            Err(FormatError::InvalidSource(_))
        ));
        return Ok(());
    }
    let expected = syntax_structure(parsed.syntax_node());
    let wide = formatting::format_with_config(
        &source,
        &Config { line_width: 120, indent_width: 4, ..Config::default() },
    )?;
    let narrow = formatting::format_with_config(
        &source,
        &Config { line_width: 10, indent_width: 4, ..Config::default() },
    )?;
    for line_width in [20, 40, 80, 120] {
        for indent_width in [1, 2, 4] {
            for unicode in [false, true] {
                let config = Config { line_width, indent_width, unicode };
                let formatted =
                    formatting::format_with_config(&source, &config).map_err(|error| {
                        format!(
                            "width {line_width}, indent {indent_width}, unicode {unicode}: {error}"
                        )
                    })?;
                assert_eq!(
                    formatting::format_with_config(&formatted, &config)?,
                    formatted,
                    "formatter must be idempotent"
                );
                assert_eq!(
                    formatting::format_with_config(&wide, &config)?,
                    formatted,
                    "wrapping must not depend on the previous width"
                );
                assert_eq!(
                    formatting::format_with_config(&narrow, &config)?,
                    formatted,
                    "wrapping must recover after a narrower width"
                );
                let candidate = lexing::lex(&formatted);
                assert_tokens(&lexed, &candidate, &config);
                assert_spacing(&candidate);
                let (candidate, errors) = parsing::parse(&candidate, &lexing::layout(&candidate));
                assert!(errors.is_empty());
                assert_eq!(expected, syntax_structure(candidate.syntax_node()));
            }
        }
    }
    Ok(())
}

datatest_stable::harness! {
    { test = formatting, root = "fixtures/formatting", pattern = r".*/Main\.purs$" },
    { test = corpus, root = "../compiler-frontend", pattern = r"^(parsing/tests/parser|lexing/tests/layout)/.*\.purs$" },
    { test = corpus, root = "fixtures/compiler", pattern = r".*\.purs$" },
}
