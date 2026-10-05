use std::fs;
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
    events.collect()
}

fn assert_tokens(original: &lexing::Lexed<'_>, candidate: &lexing::Lexed<'_>, config: &Config) {
    assert_eq!(original.len(), candidate.len());
    let comment_content = |annotation: Option<&str>| {
        annotation.unwrap_or_default().split_whitespace().collect::<String>()
    };
    for index in 0..original.len() {
        let kind = original.kind(index);
        assert_eq!(kind, candidate.kind(index));
        assert_eq!(original.qualifier(index), candidate.qualifier(index));
        assert_eq!(
            comment_content(original.annotation(index)),
            comment_content(candidate.annotation(index)),
            "comments must remain at their token boundary"
        );
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
            assert_eq!(original.text(index), candidate.text(index));
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
