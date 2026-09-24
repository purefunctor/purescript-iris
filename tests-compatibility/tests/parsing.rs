use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use building::{QueryEngine, prim};
use files::Files;
use iris_diagnostics::{DiagnosticsContext, Severity, ToDiagnostics};
use tests_compatibility::all_source_files;

type ErrorPerFile<E> = HashMap<PathBuf, E>;

#[test]
fn test_parse_package_set() {
    let mut all_errors = ErrorPerFile::default();

    for file in all_source_files() {
        let Ok(source) = fs::read_to_string(&file) else {
            continue;
        };

        let lexed = lexing::lex(&source);
        let tokens = lexing::layout(&lexed);

        let (_, errors) = parsing::parse(&lexed, &tokens);
        if !errors.is_empty() {
            all_errors.insert(file, errors);
        }
    }

    assert!(all_errors.is_empty(), "{all_errors:#?}");
}

#[test]
fn test_index_package_set() {
    let mut all_errors = ErrorPerFile::default();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let mut sources = Vec::new();
    for file in all_source_files() {
        let Ok(source) = fs::read_to_string(&file) else {
            continue;
        };
        let url = url::Url::from_file_path(&file).unwrap();
        let id = files.insert(url.to_string(), source.clone());
        engine.set_content(id, source.clone());
        sources.push((file, id, source));
    }

    for (_, id, source) in &sources {
        let Ok((parsed, _)) = engine.parsed(*id) else { continue };
        if let Some(module_name) = parsed.module_name(source) {
            engine.set_module_file(module_name.as_ref(), *id);
        }
    }

    for (file, id, source) in sources {
        let Ok((parsed, _)) = engine.parsed(id) else {
            continue;
        };
        let Ok(stabilized) = engine.stabilized(id) else {
            continue;
        };
        let Ok(indexed) = engine.indexed(id) else {
            continue;
        };

        let root = parsed.syntax_node();
        let Ok(lowered) = engine.lowered(id) else {
            if !indexed.errors.is_empty() {
                all_errors.insert(
                    file,
                    indexed.errors.iter().map(|error| format!("{error:?}")).collect(),
                );
            }
            continue;
        };
        let Ok(checked) = engine.checked(id) else {
            continue;
        };

        let context = DiagnosticsContext::new(
            &engine,
            &source,
            &root,
            &stabilized,
            &indexed,
            &lowered,
            &checked,
        );
        let errors: Vec<_> = indexed
            .errors
            .iter()
            .filter_map(|error| {
                let diagnostics = error.to_diagnostics(&context);
                if diagnostics.is_empty()
                    || diagnostics.iter().any(|diagnostic| diagnostic.severity == Severity::Error)
                {
                    Some(format!("{error:?}"))
                } else {
                    None
                }
            })
            .collect();

        if !errors.is_empty() {
            all_errors.insert(file, errors);
        }
    }

    assert!(all_errors.is_empty(), "{all_errors:#?}");
}

#[test]
fn test_cst_id_package_set() {
    let mut results = vec![];

    for file in all_source_files() {
        let Ok(source) = fs::read_to_string(&file) else {
            continue;
        };

        let lexed = lexing::lex(&source);
        let tokens = lexing::layout(&lexed);
        let (parsed, _) = parsing::parse(&lexed, &tokens);
        let node = parsed.syntax_node();

        let start = Instant::now();
        let _cst_id = stabilizing::stabilize_module(&node);
        results.push((start.elapsed(), file));
    }

    results.sort_by_key(|result| result.0);

    for result in results {
        eprintln!("{result:?}");
    }
}

#[test]
fn test_parallel_parse_package_set() {
    use building::QueryEngine;
    use rayon::prelude::*;

    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let mut source = vec![];
    for path in all_source_files() {
        let content = std::fs::read_to_string(&path).unwrap();
        let url = url::Url::from_file_path(path).unwrap();
        let uri = url.to_string();

        let id = files.insert(uri, content);
        let content = files.content(id);

        engine.set_content(id, content);
        source.push(id);
    }

    let start = Instant::now();
    source.par_iter().for_each(|&id| {
        let engine = engine.snapshot();
        let parsed = engine.parsed(id);
        assert!(parsed.is_ok());
    });
    let parsing = start.elapsed();
    println!("Parsing {parsing:?}");

    let names = source.iter().filter_map(|&id| {
        let content = engine.content(id).ok()?;
        let (parsed, _) = engine.parsed(id).ok()?;
        let module_name = parsed.module_name(&content)?;
        Some((module_name, id))
    });

    for (name, file) in names {
        engine.set_module_file(&name, file);
    }

    let start = Instant::now();
    source.par_iter().for_each(|&id| {
        let stabilized = engine.stabilized(id);
        assert!(stabilized.is_ok());
    });
    let cst_id = start.elapsed();
    println!("CstId {cst_id:?}");

    let start = Instant::now();
    source.par_iter().for_each(|&id| {
        let engine = engine.snapshot();
        let indexed = engine.indexed(id);
        assert!(indexed.is_ok());
    });
    let indexing = start.elapsed();
    println!("Indexing {indexing:?}");

    let start = Instant::now();
    source.par_iter().for_each(|&id| {
        let engine = engine.snapshot();
        let resolved = engine.resolved(id);
        assert!(resolved.is_ok());
    });
    let resolving = start.elapsed();
    println!("Resolving {resolving:?}");

    let start = Instant::now();
    source.par_iter().for_each(|&id| {
        let engine = engine.snapshot();
        let lowered = engine.lowered(id);
        assert!(lowered.is_ok());
    });
    let lowering = start.elapsed();
    println!("Lowering {lowering:?}");

    let start = Instant::now();
    source.par_iter().for_each(|&id| {
        let engine = engine.snapshot();
        let grouped = engine.grouped(id);
        assert!(grouped.is_ok());
    });
    let grouped = start.elapsed();
    println!("Grouped {grouped:?}");

    let start = Instant::now();
    source.par_iter().for_each(|&id| {
        let engine = engine.snapshot();
        let bracketed = engine.bracketed(id);
        assert!(bracketed.is_ok());
    });
    let bracketing = start.elapsed();
    println!("Bracketing {bracketing:?}");

    let start = Instant::now();
    source.par_iter().for_each(|&id| {
        let engine = engine.snapshot();
        let sectioned = engine.sectioned(id);
        assert!(sectioned.is_ok());
    });
    let sectioning = start.elapsed();
    println!("Sectioning {sectioning:?}");

    let start = Instant::now();
    source.iter().for_each(|&id| {
        let engine = engine.snapshot();
        let checked = engine.checked(id);
        assert!(checked.is_ok());
    });
    let checking = start.elapsed();
    println!("Checking {checking:?}");

    println!(
        "Total {:?}",
        parsing
            + cst_id
            + indexing
            + resolving
            + lowering
            + grouped
            + bracketing
            + sectioning
            + checking
    );
}
