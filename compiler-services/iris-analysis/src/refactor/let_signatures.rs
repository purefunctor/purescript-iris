use std::collections::hash_map::Entry;

use checking::core::Name;
use checking::core::pretty::{
    ModuleScopeResolver, OutOfScopePolicy, Pretty, PrettyConfig, PrettyState,
};
use checking::{CheckedModule, Type, TypeId};
use files::FileId;
use indexing::TypeItemId;
use lowering::{GraphNodeId, LoweredModule};
use resolving::ResolvedModule;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::{SmolStr, format_smolstr};
use syntax::ast::AstNode;
use syntax::cst;

use crate::extract::AnnotationSyntaxRange;
use crate::refactor::imports::{self, Importability, ImportableType};
use crate::refactor::{FileRefactorPlan, RefactorPlan, SkipReason, SkippedEntry, SourceEdit};
use crate::scope::{ModuleTypeScopes, RigidScopeStatus};
use crate::{AnalyzerError, AnalyzerQueries};

const DEFAULT_LINE_WIDTH: usize = 100;
const MINIMUM_SIGNATURE_WIDTH: usize = 40;

struct BindingVariables {
    scope_node: Option<GraphNodeId>,
    active_rigids: Vec<(Name, SmolStr)>,
    local_forall_binders: Vec<Name>,
}

struct BindingPlacement {
    line_start: usize,
    indent: String,
    width: usize,
}

struct CandidateBinding {
    name: SmolStr,
    type_id: TypeId,
    placement: BindingPlacement,
    variables: BindingVariables,
    rendered: Option<SmolStr>,
}

struct ModuleRefactorContext<'a, Queries> {
    engine: &'a Queries,
    file_id: FileId,
    resolved: &'a ResolvedModule,
    lowered: &'a LoweredModule,
    checked: &'a CheckedModule,
    prim: &'a ResolvedModule,
    prim_function: TypeId,
    bundle_operator: Option<(FileId, TypeItemId)>,
    type_scopes: ModuleTypeScopes,
}

impl<'a, Queries> ModuleRefactorContext<'a, Queries>
where
    Queries: AnalyzerQueries,
{
    fn scope_resolver(&self) -> ModuleScopeResolver<'a> {
        ModuleScopeResolver::new(self.resolved, self.prim, OutOfScopePolicy::RecordUnresolved)
            .with_bundle_operator(self.bundle_operator)
    }

    fn classify_binding_type(
        &self,
        scope_node: Option<GraphNodeId>,
        type_id: TypeId,
    ) -> Result<BindingTypeClassification, AnalyzerError> {
        let mut classifier = BindingTypeClassifier {
            context: self,
            scope_node,
            locally_bound: FxHashSet::default(),
            active_rigids: Vec::new(),
            local_forall_binders: Vec::new(),
            unspellable_rigids: Vec::new(),
            has_unsolved_or_ambiguous: false,
        };
        classifier.visit(type_id)?;
        if classifier.has_unsolved_or_ambiguous {
            Ok(BindingTypeClassification::PolymorphicOrUnsolved)
        } else if !classifier.unspellable_rigids.is_empty() {
            Ok(BindingTypeClassification::UnspellableScopedVariables(classifier.unspellable_rigids))
        } else {
            Ok(BindingTypeClassification::Renderable(BindingVariables {
                scope_node,
                active_rigids: classifier.active_rigids,
                local_forall_binders: classifier.local_forall_binders,
            }))
        }
    }

    fn prepare_pretty_state(
        &self,
        binding_name: &str,
        variables: &BindingVariables,
        width: usize,
    ) -> Result<PrettyState<'a, Queries>, AnalyzerError> {
        let config = signature_pretty_config(width);
        let pretty = Pretty::with_config(self.engine, self.checked, config);
        let mut state = pretty.state();
        let mut reserved_names = FxHashSet::default();

        for (rigid_name, source_name) in &variables.active_rigids {
            state.assign_display_name(*rigid_name, SmolStr::clone(source_name));
            reserved_names.insert(SmolStr::clone(source_name));
        }

        let mut next_generated = 0u32;
        for &binder_name in &variables.local_forall_binders {
            let base = if let Some(original_name) = self.lookup_binder_source_name(binder_name)? {
                if self.name_in_scope(variables.scope_node, &original_name) {
                    format_smolstr!("{binding_name}_{original_name}")
                } else {
                    original_name
                }
            } else {
                let mut generated = format_smolstr!("t{next_generated}");
                while reserved_names.contains(&generated)
                    || self.name_in_scope(variables.scope_node, &generated)
                {
                    next_generated += 1;
                    generated = format_smolstr!("t{next_generated}");
                }
                next_generated += 1;
                generated
            };

            let mut candidate = SmolStr::clone(&base);
            let mut suffix = 1u32;
            while reserved_names.contains(&candidate)
                || self.name_in_scope(variables.scope_node, &candidate)
            {
                candidate = format_smolstr!("{base}{suffix}");
                suffix += 1;
            }
            reserved_names.insert(SmolStr::clone(&candidate));
            state.assign_display_name(binder_name, candidate);
        }

        Ok(state)
    }

    fn name_in_scope(&self, scope_node: Option<GraphNodeId>, name: &str) -> bool {
        self.type_scopes.name_in_scope(self.lowered, scope_node, name)
    }

    fn lookup_binder_source_name(&self, name: Name) -> Result<Option<SmolStr>, AnalyzerError> {
        let text_id = if name.file == self.file_id {
            self.checked.lookup_name(name)
        } else {
            self.engine.checked(name.file)?.lookup_name(name)
        };
        Ok(text_id.map(|id| SmolStr::clone(self.engine.lookup_smol_str(id))))
    }

    fn format_unspellable_rigids(
        &self,
        unspellable: Vec<UnspellableRigid>,
    ) -> Result<String, AnalyzerError> {
        let mut pretty_state = Pretty::new(self.engine, self.checked).state();
        let mut rendered_names = Vec::with_capacity(unspellable.len());
        for rigid in unspellable {
            if let Some(shadowed_name) = rigid.shadowed_as {
                rendered_names.push(format!("{shadowed_name} (shadowed)"));
            } else {
                let display = self
                    .lookup_binder_source_name(rigid.name)?
                    .unwrap_or_else(|| pretty_state.display_name(rigid.name));
                rendered_names.push(format!("{display} (not in lexical scope)"));
            }
        }
        Ok(rendered_names.join(", "))
    }

    fn can_elide_kind_applications(
        &self,
        head: TypeId,
        kind_argument_count: usize,
        type_argument_count: usize,
    ) -> Result<bool, AnalyzerError> {
        let head_kind = match *self.engine.lookup_type(head) {
            Type::Constructor(file_id, type_item_id) | Type::Synonym(file_id, type_item_id) => {
                let checked = self.engine.checked(file_id)?;
                checked.lookup_type_item_kind(type_item_id)
            }
            _ => None,
        };
        let Some(mut current_kind) = head_kind else {
            return Ok(false);
        };

        let mut kind_binders = Vec::with_capacity(kind_argument_count);
        for _ in 0..kind_argument_count {
            let Type::Forall(binder_id, inner_kind) = *self.engine.lookup_type(current_kind) else {
                return Ok(false);
            };
            kind_binders.push(self.engine.lookup_forall_binder(binder_id));
            current_kind = inner_kind;
        }

        let mut parameter_kinds = Vec::with_capacity(type_argument_count);
        for _ in 0..type_argument_count {
            let Some((parameter_kind, result_kind)) = self.split_function_kind(current_kind) else {
                break;
            };
            parameter_kinds.push(parameter_kind);
            current_kind = result_kind;
        }

        let mut determined = FxHashSet::default();
        for parameter_kind in parameter_kinds {
            collect_rigid_names(self.engine, parameter_kind, &mut determined);
        }

        for binder in kind_binders.iter().rev() {
            if determined.contains(&binder.name) {
                collect_rigid_names(self.engine, binder.kind, &mut determined);
            }
        }

        Ok(kind_binders.iter().all(|binder| determined.contains(&binder.name)))
    }

    fn split_function_kind(&self, kind: TypeId) -> Option<(TypeId, TypeId)> {
        match *self.engine.lookup_type(kind) {
            Type::Function(argument, result) => Some((argument, result)),
            Type::Application(function, result) => {
                if let Type::Application(constructor, argument) = *self.engine.lookup_type(function)
                    && constructor == self.prim_function
                {
                    return Some((argument, result));
                }
                None
            }
            _ => None,
        }
    }
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

    let indexed = engine.indexed(file_id)?;
    let resolved = engine.resolved(file_id)?;
    let lowered = engine.lowered(file_id)?;
    let checked = engine.checked(file_id)?;
    let stabilized = engine.stabilized(file_id)?;
    let root = parsed.syntax_node();
    let prim = engine.resolved(engine.prim_id())?;
    let core = engine.checked_core()?;

    let context = ModuleRefactorContext {
        engine,
        file_id,
        resolved: &resolved,
        lowered: &lowered,
        checked: &checked,
        prim: &prim,
        prim_function: core.prim.function,
        bundle_operator: Some((core.iris_effect.file_id, core.iris_effect.bundle_operator)),
        type_scopes: ModuleTypeScopes::build(engine, &indexed, &lowered, &checked),
    };

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

        let skip = |reason, detail| SkippedEntry {
            file_id,
            offset,
            name: name.to_string(),
            reason,
            detail,
        };

        let Some(type_id) = checked.node_types.lookup_let(group_id) else {
            skipped.push(skip(SkipReason::Unchecked, None));
            continue;
        };

        let scope_node = lowered.nodes.let_node(group_id);
        let variables = match context.classify_binding_type(scope_node, type_id)? {
            BindingTypeClassification::Renderable(variables) => variables,
            BindingTypeClassification::UnspellableScopedVariables(unspellable) => {
                let detail = context.format_unspellable_rigids(unspellable)?;
                skipped.push(skip(SkipReason::ScopedTypeVariable, Some(detail)));
                continue;
            }
            BindingTypeClassification::PolymorphicOrUnsolved => {
                skipped.push(skip(SkipReason::PolymorphicOrUnsolved, None));
                continue;
            }
        };

        let placement = match resolve_binding_placement(&content, &equation, offset) {
            Ok(placement) => placement,
            Err(reason) => {
                skipped.push(skip(reason, None));
                continue;
            }
        };

        let mut pretty_state = context.prepare_pretty_state(name, &variables, placement.width)?;
        let mut probe_resolver = context.scope_resolver();
        let rendered =
            pretty_state.render_signature_with_resolver(name, type_id, &mut probe_resolver)?;

        if rendered.is_none() {
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
                skipped
                    .push(skip(SkipReason::UnimportableTypes, Some(unimportable_names.join(", "))));
                continue;
            }

            pending_imports.extend(importable_for_binding);
        }

        candidates.push(CandidateBinding {
            name: SmolStr::clone(name),
            type_id,
            placement,
            variables,
            rendered,
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
            let mut pretty_state = context.prepare_pretty_state(
                &candidate.name,
                &candidate.variables,
                candidate.placement.width,
            )?;
            let mut resolver =
                context.scope_resolver().with_planned_spellings(&planned_imports.spellings);
            let Some(rendered) = pretty_state.render_signature_with_resolver(
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
            new_text.push_str(&candidate.placement.indent);
            new_text.push_str(line);
            new_text.push_str(newline);
        }
        edits.push(SourceEdit {
            start_byte: candidate.placement.line_start,
            end_byte: candidate.placement.line_start,
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

fn signature_pretty_config(width: usize) -> PrettyConfig {
    PrettyConfig::new()
        .width(width)
        .without_rigid_kinds()
        .without_kind_applications()
        .quote_record_labels()
}

fn resolve_binding_placement(
    content: &str,
    equation: &cst::LetBindingEquation,
    offset: usize,
) -> Result<BindingPlacement, SkipReason> {
    let annotation_range = AnnotationSyntaxRange::from_node(equation.syntax());
    if let Some(annotation) = annotation_range.annotation
        && has_leading_comment(
            content,
            usize::from(annotation.start()),
            usize::from(annotation.end()),
        )
    {
        return Err(SkipReason::LeadingComment);
    }

    let line_start = content[..offset].rfind('\n').map_or(0, |index| index + 1);
    let prefix = &content[line_start..offset];
    if prefix.contains('\t') {
        return Err(SkipReason::TabIndentation);
    }
    if !prefix.bytes().all(|byte| byte == b' ') {
        return Err(SkipReason::NotAtLineStart);
    }

    let indent = prefix.to_string();
    let width = DEFAULT_LINE_WIDTH.saturating_sub(indent.len()).max(MINIMUM_SIGNATURE_WIDTH);
    Ok(BindingPlacement { line_start, indent, width })
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
    Renderable(BindingVariables),
    UnspellableScopedVariables(Vec<UnspellableRigid>),
    PolymorphicOrUnsolved,
}

struct UnspellableRigid {
    name: Name,
    shadowed_as: Option<SmolStr>,
}

struct BindingTypeClassifier<'a, Queries> {
    context: &'a ModuleRefactorContext<'a, Queries>,
    scope_node: Option<GraphNodeId>,
    locally_bound: FxHashSet<Name>,
    active_rigids: Vec<(Name, SmolStr)>,
    local_forall_binders: Vec<Name>,
    unspellable_rigids: Vec<UnspellableRigid>,
    has_unsolved_or_ambiguous: bool,
}

impl<Queries> BindingTypeClassifier<'_, Queries>
where
    Queries: AnalyzerQueries,
{
    fn visit(&mut self, id: TypeId) -> Result<(), AnalyzerError> {
        match *self.context.engine.lookup_type(id) {
            Type::Constructor(_, _)
            | Type::Synonym(_, _)
            | Type::Integer(_)
            | Type::String(_, _) => {}
            Type::Rigid(name, _, _) => {
                if !self.locally_bound.contains(&name) {
                    match self.context.type_scopes.classify_rigid(
                        self.context.lowered,
                        self.scope_node,
                        name,
                    ) {
                        RigidScopeStatus::Active(source_name) => {
                            if !self.active_rigids.iter().any(|(existing, _)| *existing == name) {
                                self.active_rigids.push((name, source_name));
                            }
                        }
                        RigidScopeStatus::Shadowed(source_name) => {
                            if !self.unspellable_rigids.iter().any(|existing| existing.name == name)
                            {
                                self.unspellable_rigids.push(UnspellableRigid {
                                    name,
                                    shadowed_as: Some(source_name),
                                });
                            }
                        }
                        RigidScopeStatus::NotInScope => {
                            if !self.unspellable_rigids.iter().any(|existing| existing.name == name)
                            {
                                self.unspellable_rigids
                                    .push(UnspellableRigid { name, shadowed_as: None });
                            }
                        }
                    }
                }
            }
            Type::Application(_, _) | Type::KindApplication(_, _) => {
                let mut type_arguments = Vec::new();
                let mut kind_arguments = Vec::new();
                let mut current = id;
                while let Type::Application(function, argument) =
                    *self.context.engine.lookup_type(current)
                {
                    type_arguments.push(argument);
                    current = function;
                }
                while let Type::KindApplication(function, argument) =
                    *self.context.engine.lookup_type(current)
                {
                    kind_arguments.push(argument);
                    current = function;
                }
                type_arguments.reverse();

                if !kind_arguments.is_empty()
                    && !self.context.can_elide_kind_applications(
                        current,
                        kind_arguments.len(),
                        type_arguments.len(),
                    )?
                {
                    self.has_unsolved_or_ambiguous = true;
                }

                self.visit(current)?;
                for argument in type_arguments {
                    self.visit(argument)?;
                }
            }
            Type::Forall(binder_id, inner) => {
                let binder = self.context.engine.lookup_forall_binder(binder_id);
                self.visit(binder.kind)?;
                if !self.local_forall_binders.contains(&binder.name) {
                    self.local_forall_binders.push(binder.name);
                }
                let inserted = self.locally_bound.insert(binder.name);
                self.visit(inner)?;
                if inserted {
                    self.locally_bound.remove(&binder.name);
                }
            }
            Type::Constrained(constraint, inner) => {
                self.visit(constraint)?;
                self.visit(inner)?;
            }
            Type::ConstraintOrigin(constraint, _) => {
                self.visit(constraint)?;
            }
            Type::Function(left, right) | Type::Kinded(left, right) => {
                self.visit(left)?;
                self.visit(right)?;
            }
            Type::Row(row_id) => {
                let row = self.context.engine.lookup_row_type(row_id);
                for field in row.fields.iter() {
                    self.visit(field.id)?;
                }
                if let Some(tail) = row.tail {
                    self.visit(tail)?;
                }
            }
            Type::Unification(_) | Type::Free(_) | Type::Unknown(_) => {
                self.has_unsolved_or_ambiguous = true;
            }
        }
        Ok(())
    }
}

fn collect_rigid_names(engine: &impl AnalyzerQueries, id: TypeId, rigids: &mut FxHashSet<Name>) {
    match *engine.lookup_type(id) {
        Type::Rigid(name, _, kind) => {
            rigids.insert(name);
            collect_rigid_names(engine, kind, rigids);
        }
        Type::Application(left, right)
        | Type::KindApplication(left, right)
        | Type::Function(left, right)
        | Type::Kinded(left, right)
        | Type::Constrained(left, right)
        | Type::ConstraintOrigin(left, right) => {
            collect_rigid_names(engine, left, rigids);
            collect_rigid_names(engine, right, rigids);
        }
        Type::Forall(binder_id, inner) => {
            let binder = engine.lookup_forall_binder(binder_id);
            collect_rigid_names(engine, binder.kind, rigids);
            collect_rigid_names(engine, inner, rigids);
        }
        Type::Row(row_id) => {
            let row = engine.lookup_row_type(row_id);
            for field in row.fields.iter() {
                collect_rigid_names(engine, field.id, rigids);
            }
            if let Some(tail) = row.tail {
                collect_rigid_names(engine, tail, rigids);
            }
        }
        Type::Constructor(_, _)
        | Type::Synonym(_, _)
        | Type::Integer(_)
        | Type::String(_, _)
        | Type::Unification(_)
        | Type::Free(_)
        | Type::Unknown(_) => {}
    }
}
