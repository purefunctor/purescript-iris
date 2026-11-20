use std::collections::hash_map::Entry;
use std::sync::Arc;

use checking::core::Name;
use checking::core::pretty::{ModuleScopeResolver, OutOfScopePolicy, Pretty, PrettyConfig};
use checking::{Type, TypeId};
use files::FileId;
use indexing::{IndexedTypeItemKind, TypeItemId};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;
use syntax::ast::AstNode;

use crate::extract::AnnotationSyntaxRange;
use crate::refactor::imports::{self, Importability, ImportableType};
use crate::refactor::{FileRefactorPlan, RefactorPlan, SkipReason, SkippedEntry, SourceEdit};
use crate::{AnalyzerError, AnalyzerQueries};

const DEFAULT_LINE_WIDTH: usize = 100;
const MINIMUM_SIGNATURE_WIDTH: usize = 40;

struct CandidateBinding {
    name: SmolStr,
    type_id: TypeId,
    line_start: usize,
    indent: String,
    width: usize,
    rendered: Option<SmolStr>,
}

pub fn add_let_signatures<Queries>(
    engine: &Queries,
    files: impl IntoIterator<Item = FileId>,
) -> Result<RefactorPlan, AnalyzerError>
where
    Queries: AnalyzerQueries
        + iris_diagnostics::ExternalQueries
        + foreign_javascript::ForeignQueries
        + javascript::ModuleQueries,
{
    let mut plan = RefactorPlan::default();
    for file_id in files {
        let (file_plan, skipped) = refactor_module(engine, file_id)?;
        if let Some(file_plan) = file_plan {
            plan.files.push(file_plan);
        }
        plan.skipped.extend(skipped);
    }
    Ok(plan)
}

fn refactor_module<Queries>(
    engine: &Queries,
    file_id: FileId,
) -> Result<(Option<FileRefactorPlan>, Vec<SkippedEntry>), AnalyzerError>
where
    Queries: AnalyzerQueries
        + iris_diagnostics::ExternalQueries
        + foreign_javascript::ForeignQueries
        + javascript::ModuleQueries,
{
    let content = engine.content(file_id)?;
    let (parsed, _) = engine.parsed(file_id)?;

    let error_counts = iris_diagnostics::count_errors(engine, file_id)?;
    if let Some(detail) = error_counts.summary() {
        let module_name = parsed
            .module_name(&content)
            .map(|name| name.to_string())
            .unwrap_or_else(|| "<module>".to_string());
        let skipped = vec![SkippedEntry {
            file_id,
            offset: 0,
            name: module_name,
            reason: SkipReason::ModuleHasErrors,
            detail: Some(detail),
        }];
        return Ok((None, skipped));
    }

    let resolved = engine.resolved(file_id)?;
    let lowered = engine.lowered(file_id)?;
    let checked = engine.checked(file_id)?;

    let stabilized = engine.stabilized(file_id)?;
    let root = parsed.syntax_node();
    let prim = engine.resolved(engine.prim_id())?;
    let bundle_operator = lookup_iris_effect_bundle_operator(engine)?;

    let groups = lowered.tree.iter_let_binding_groups().map(|(group_id, group)| {
        let sort_offset = group
            .equations
            .first()
            .and_then(|&equation_id| stabilized.ast_ptr(equation_id))
            .and_then(|pointer| pointer.try_to_node(&root))
            .map_or(usize::MAX, |node| usize::from(node.syntax().text_range().start()));
        (sort_offset, group_id)
    });
    let mut groups = groups.collect::<Vec<_>>();
    groups.sort_by_key(|&(offset, group_id)| (offset, group_id.into_raw().into_u32()));

    let mut candidates = Vec::<CandidateBinding>::new();
    let mut skipped = Vec::<SkippedEntry>::new();
    let mut pending_imports = Vec::<ImportableType>::new();
    let mut importability_cache = FxHashMap::<(FileId, TypeItemId), Importability>::default();

    for (_, group_id) in groups {
        let group = lowered.tree.get_let_binding_group(group_id);
        if group.signature.is_some() {
            continue;
        }
        let Some(name) = group.name.as_ref() else {
            continue;
        };
        let Some(&first_equation_id) = group.equations.first() else {
            continue;
        };
        let Some(equation) =
            stabilized.ast_ptr(first_equation_id).and_then(|pointer| pointer.try_to_node(&root))
        else {
            continue;
        };
        let Some(name_token) = equation.name_token() else {
            continue;
        };
        let offset = usize::from(name_token.text_range().start());

        let Some(type_id) = checked.node_types.lookup_let(group_id) else {
            skipped.push(SkippedEntry {
                file_id,
                offset,
                name: name.to_string(),
                reason: SkipReason::Unchecked,
                detail: None,
            });
            continue;
        };

        match classify_binding_type(engine, type_id) {
            BindingTypeClassification::Monomorphic => {}
            BindingTypeClassification::ScopedTypeVariables(names) => {
                let mut pretty_state = Pretty::new(engine, &checked).state();
                let rendered_names = names
                    .into_iter()
                    .map(|rigid_name| pretty_state.display_name(rigid_name).to_string())
                    .collect::<Vec<_>>();
                skipped.push(SkippedEntry {
                    file_id,
                    offset,
                    name: name.to_string(),
                    reason: SkipReason::ScopedTypeVariable,
                    detail: Some(rendered_names.join(", ")),
                });
                continue;
            }
            BindingTypeClassification::PolymorphicOrUnsolved => {
                skipped.push(SkippedEntry {
                    file_id,
                    offset,
                    name: name.to_string(),
                    reason: SkipReason::PolymorphicOrUnsolved,
                    detail: None,
                });
                continue;
            }
        }

        let annotation_range = AnnotationSyntaxRange::from_node(equation.syntax());
        if let Some(annotation) = annotation_range.annotation
            && has_leading_comment(
                &content,
                usize::from(annotation.start()),
                usize::from(annotation.end()),
            )
        {
            skipped.push(SkippedEntry {
                file_id,
                offset,
                name: name.to_string(),
                reason: SkipReason::LeadingComment,
                detail: None,
            });
            continue;
        }

        let line_start = content[..offset].rfind('\n').map_or(0, |index| index + 1);
        let prefix = &content[line_start..offset];
        if prefix.contains('\t') {
            skipped.push(SkippedEntry {
                file_id,
                offset,
                name: name.to_string(),
                reason: SkipReason::TabIndentation,
                detail: None,
            });
            continue;
        }
        if !prefix.bytes().all(|byte| byte == b' ') {
            skipped.push(SkippedEntry {
                file_id,
                offset,
                name: name.to_string(),
                reason: SkipReason::NotAtLineStart,
                detail: None,
            });
            continue;
        }

        let indent = prefix.to_string();
        let width = DEFAULT_LINE_WIDTH.saturating_sub(indent.len()).max(MINIMUM_SIGNATURE_WIDTH);
        let config = PrettyConfig::new().width(width).quote_record_labels();
        let pretty = Pretty::with_config(engine, &checked, config);
        let mut probe_resolver =
            ModuleScopeResolver::new(&resolved, &prim, OutOfScopePolicy::RecordUnresolved)
                .with_bundle_operator(bundle_operator);

        if let Some(rendered) =
            pretty.render_signature_with_resolver(name, type_id, &mut probe_resolver)?
        {
            candidates.push(CandidateBinding {
                name: SmolStr::clone(name),
                type_id,
                line_start,
                indent,
                width,
                rendered: Some(rendered),
            });
            continue;
        }

        let mut importable_for_binding = Vec::new();
        let mut unimportable_names = Vec::new();
        for &(missing_file, missing_type) in probe_resolver.unresolved() {
            let importability = match importability_cache.entry((missing_file, missing_type)) {
                Entry::Occupied(occupied) => occupied.into_mut(),
                Entry::Vacant(vacant) => {
                    let importability =
                        imports::check_importable(engine, missing_file, missing_type)?;
                    vacant.insert(importability)
                }
            };
            match importability {
                Importability::Importable(importable) => {
                    importable_for_binding.push(importable.clone());
                }
                Importability::Unimportable(description) => {
                    unimportable_names.push(String::clone(description));
                }
            }
        }

        if !unimportable_names.is_empty() {
            skipped.push(SkippedEntry {
                file_id,
                offset,
                name: name.to_string(),
                reason: SkipReason::UnimportableTypes,
                detail: Some(unimportable_names.join(", ")),
            });
            continue;
        }

        pending_imports.extend(importable_for_binding);
        candidates.push(CandidateBinding {
            name: SmolStr::clone(name),
            type_id,
            line_start,
            indent,
            width,
            rendered: None,
        });
    }

    if candidates.is_empty() {
        return Ok((None, skipped));
    }

    let planned_imports =
        imports::plan_imports(&content, &root, &stabilized, &resolved, &prim, &pending_imports);
    let newline = if content.contains("\r\n") { "\r\n" } else { "\n" };

    let mut edits = planned_imports.edits;
    let mut signatures_added = 0;

    for candidate in candidates {
        let rendered = if let Some(rendered) = candidate.rendered {
            rendered
        } else {
            let config = PrettyConfig::new().width(candidate.width).quote_record_labels();
            let pretty = Pretty::with_config(engine, &checked, config);
            let mut resolver =
                ModuleScopeResolver::new(&resolved, &prim, OutOfScopePolicy::RecordUnresolved)
                    .with_bundle_operator(bundle_operator)
                    .with_planned_spellings(&planned_imports.spellings);
            let Some(rendered) = pretty.render_signature_with_resolver(
                &candidate.name,
                candidate.type_id,
                &mut resolver,
            )?
            else {
                unreachable!("invariant violated: planned imports did not resolve signature type");
            };
            rendered
        };

        let mut new_text = String::new();
        for line in rendered.lines() {
            new_text.push_str(&candidate.indent);
            new_text.push_str(line);
            new_text.push_str(newline);
        }
        edits.push(SourceEdit {
            start_byte: candidate.line_start,
            end_byte: candidate.line_start,
            new_text,
        });
        signatures_added += 1;
    }

    edits.sort_by(|left, right| {
        left.start_byte.cmp(&right.start_byte).then_with(|| left.end_byte.cmp(&right.end_byte))
    });

    let file_plan = FileRefactorPlan {
        file_id,
        edits,
        signatures_added,
        imports_added: planned_imports.imports_added,
        imports_updated: planned_imports.imports_updated,
    };
    Ok((Some(file_plan), skipped))
}

fn has_leading_comment(content: &str, start: usize, end: usize) -> bool {
    let text = &content[start..end];
    let leading = if start == 0 || content.as_bytes()[start - 1] == b'\n' {
        text
    } else if let Some(newline) = text.find('\n') {
        &text[newline + 1..]
    } else {
        return false;
    };
    leading.contains("--") || leading.contains("{-") || leading.contains("-}")
}

enum BindingTypeClassification {
    Monomorphic,
    ScopedTypeVariables(Vec<Name>),
    PolymorphicOrUnsolved,
}

struct BindingTypeInfo {
    rigid_names: Vec<Name>,
    has_kind_application: bool,
}

fn classify_binding_type(engine: &impl AnalyzerQueries, id: TypeId) -> BindingTypeClassification {
    let mut info = BindingTypeInfo { rigid_names: Vec::new(), has_kind_application: false };
    if !collect_binding_type_info(engine, id, &mut info) {
        return BindingTypeClassification::PolymorphicOrUnsolved;
    }
    if !info.rigid_names.is_empty() {
        BindingTypeClassification::ScopedTypeVariables(info.rigid_names)
    } else if info.has_kind_application {
        BindingTypeClassification::PolymorphicOrUnsolved
    } else {
        BindingTypeClassification::Monomorphic
    }
}

fn collect_binding_type_info(
    engine: &impl AnalyzerQueries,
    id: TypeId,
    info: &mut BindingTypeInfo,
) -> bool {
    match *engine.lookup_type(id) {
        Type::Constructor(_, _) | Type::Synonym(_, _) | Type::Integer(_) | Type::String(_, _) => {
            true
        }
        Type::Rigid(name, _, _) => {
            if !info.rigid_names.contains(&name) {
                info.rigid_names.push(name);
            }
            true
        }
        Type::KindApplication(left, right) => {
            info.has_kind_application = true;
            collect_binding_type_info(engine, left, info)
                && collect_binding_type_info(engine, right, info)
        }
        Type::Application(left, right)
        | Type::Function(left, right)
        | Type::Kinded(left, right) => {
            collect_binding_type_info(engine, left, info)
                && collect_binding_type_info(engine, right, info)
        }
        Type::Row(row_id) => {
            let row = engine.lookup_row_type(row_id);
            row.fields.iter().all(|field| collect_binding_type_info(engine, field.id, info))
                && row.tail.is_none_or(|tail| collect_binding_type_info(engine, tail, info))
        }
        Type::Forall(_, _)
        | Type::Constrained(_, _)
        | Type::ConstraintOrigin(_, _)
        | Type::Unification(_)
        | Type::Free(_)
        | Type::Unknown(_) => false,
    }
}

fn lookup_iris_effect_bundle_operator(
    engine: &impl AnalyzerQueries,
) -> Result<Option<(FileId, TypeItemId)>, AnalyzerError> {
    let Some(effect_file) = engine.module_file("Iris.Effect") else {
        return Ok(None);
    };
    let indexed: Arc<indexing::IndexedModule> = engine.indexed(effect_file)?;
    for (type_id, item) in indexed.items.iter_types() {
        if matches!(item.kind, IndexedTypeItemKind::Operator { .. })
            && item.name.as_deref() == Some("&")
        {
            return Ok(Some((effect_file, type_id)));
        }
    }
    Ok(None)
}
