//! Conversion from the checked semantic tree into the owned functional tree.

mod application;
mod declaration;
mod evidence;
mod expression;
mod stylex;

use std::cell::RefCell;
use std::sync::Arc;

use building_types::{QueryError, QueryResult};
use checking::evidence::{EvidenceBinderId, InstanceCandidateOrigin};
use checking::tree as checking_tree;
use files::FileId;
use indexing::{IndexedTermItemKind, IndexedTypeItemKind, OrderedTermItemId, TermItemId};
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::{SmolStr, format_smolstr};
use thiserror::Error;

use crate::error::{ModuleError, ModuleResult, UnsupportedState};
use crate::optimize::inline_simple_bindings;
use crate::tree::{
    Declaration, DeclarationKind, Expression, ExpressionId, ExpressionKind, Field, FieldIdentity,
    GeneratedGlobalId, Global, GlobalId, IndirectModuleExports, InstanceIdentity, LocalId, Module,
    ModuleDependency, ModuleSurface, Parameter, Pattern, PatternId, PatternKind, RecursiveGroupId,
    Storage, SuperclassIdentity,
};

use self::declaration::{derive_declaration, instance_declaration, term_declaration};
use self::evidence::{EvidenceHoisting, EvidenceKeys, EvidenceScope};

type ConversionResult<T> = Result<T, ConversionError>;

#[derive(Debug, Error)]
enum ConversionError {
    #[error(transparent)]
    Query(#[from] QueryError),
    #[error(transparent)]
    Module(#[from] ModuleError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum BindingSource {
    SourceBinder(lowering::BinderId),
    CheckedBinder(checking_tree::BinderId),
    Let(lowering::LetBindingNameGroupId),
    RecordPun(lowering::RecordPunId),
    Evidence(EvidenceBinderId),
}

struct Dependency {
    module_name: SmolStr,
    indexed: Option<Arc<indexing::IndexedModule>>,
}

pub fn convert_module(
    queries: &impl checking::ExternalQueries,
    file_id: FileId,
) -> QueryResult<ModuleResult<Module>> {
    match convert_module_inner(queries, file_id) {
        Ok(module) => Ok(Ok(module)),
        Err(ConversionError::Query(error)) => Err(error),
        Err(ConversionError::Module(error)) => Ok(Err(error)),
    }
}

fn convert_module_inner(
    queries: &impl checking::ExternalQueries,
    file_id: FileId,
) -> ConversionResult<Module> {
    let context = Context::new(queries, file_id)?;
    convert(context)
}

struct Context<'c, Q> {
    queries: &'c Q,
    file_id: FileId,
    module_name: SmolStr,
    indexed: Arc<indexing::IndexedModule>,
    lowered: Arc<lowering::LoweredModule>,
    checked: Arc<checking::CheckedModule>,
    recursive_groups: FxHashMap<TermItemId, RecursiveGroupId>,
    record_pun_names: FxHashMap<lowering::RecordPunId, SmolStr>,
    dependencies: FxHashMap<FileId, Dependency>,
    indexed_dependencies: RefCell<FxHashMap<FileId, Arc<indexing::IndexedModule>>>,

    parameters: FxHashMap<BindingSource, Parameter>,
    next_local: u32,
    next_generated_global: u32,
    evidence_keys: EvidenceKeys,
    evidence_scopes: Vec<EvidenceScope>,
    evidence_hoisting: EvidenceHoisting,
    instance_names: RefCell<checking_tree::pretty::InstanceNames>,

    storage: Storage,
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    fn new(queries: &'c Q, file_id: FileId) -> ConversionResult<Context<'c, Q>> {
        let content = queries.content(file_id)?;
        let (parsed, _) = queries.parsed(file_id)?;
        let module_name = parsed.module_name(&content).filter(|name| !name.is_empty()).ok_or(
            ModuleError::Unsupported { file_id, state: UnsupportedState::MissingModuleName },
        )?;
        let indexed = queries.indexed(file_id)?;
        let lowered = queries.lowered(file_id)?;
        let grouped = queries.grouped(file_id)?;
        let checked = queries.checked(file_id)?;

        let mut recursive_groups = FxHashMap::default();
        for (position, group) in grouped.term_scc.iter().enumerate() {
            if !group.is_recursive() {
                continue;
            }
            let group_id = RecursiveGroupId(position as u32);
            for &term_id in group.as_slice() {
                recursive_groups.insert(term_id, group_id);
            }
        }

        let mut record_pun_names = FxHashMap::default();
        for (_, binder) in lowered.tree.iter_binder() {
            let lowering::BinderKind::Record { record } = binder else { continue };
            for field in record.iter() {
                let lowering::BinderRecordItem::RecordPun { id, name: Some(name) } = field else {
                    continue;
                };
                record_pun_names.insert(*id, SmolStr::clone(name));
            }
        }

        Ok(Context {
            queries,
            file_id,
            module_name,
            indexed,
            lowered,
            checked,
            recursive_groups,
            record_pun_names,
            dependencies: FxHashMap::default(),
            indexed_dependencies: RefCell::default(),

            parameters: FxHashMap::default(),
            next_local: 0,
            next_generated_global: 0,
            evidence_keys: EvidenceKeys::default(),
            evidence_scopes: Vec::new(),
            evidence_hoisting: EvidenceHoisting::default(),
            instance_names: RefCell::new(checking_tree::pretty::InstanceNames::default()),

            storage: Storage::default(),
        })
    }
}

fn convert(mut context: Context<'_, impl checking::ExternalQueries>) -> ConversionResult<Module> {
    let exported = context.queries.exported(context.file_id)?;
    let RuntimeExports { local, surface } = runtime_exports(&mut context, &exported)?;
    let ordered_terms = context.indexed.items.ordered_terms().iter().copied();
    let ordered_terms = ordered_terms.collect_vec();
    let mut declarations = Vec::new();
    for item_id in ordered_terms {
        let declaration = match item_id {
            OrderedTermItemId::Term(term_id) => {
                term_declaration(&mut context, term_id, local.contains(&term_id))?
            }
            OrderedTermItemId::Instance(instance_id) => {
                Some(instance_declaration(&mut context, instance_id)?)
            }
            OrderedTermItemId::Derive(derive_id) => {
                Some(derive_declaration(&mut context, derive_id)?)
            }
        };
        declarations.extend(declaration);
    }
    validate_runtime_exports(&context, &declarations, &surface)?;
    context.validate_stylex_uses(&declarations)?;
    context.hoist_closed_evidence(&mut declarations)?;

    let recursive_globals = declarations
        .iter()
        .filter_map(|declaration| declaration.recursive_group.map(|_| declaration.global.id));
    let recursive_globals = recursive_globals.collect::<FxHashSet<_>>();
    for declaration in &declarations {
        if let DeclarationKind::Value(expression) = declaration.kind {
            inline_simple_bindings(&mut context.storage, expression, &recursive_globals);
        }
    }

    let dependencies = context.dependencies.iter().map(|(&file_id, dependency)| ModuleDependency {
        file_id,
        module_name: SmolStr::clone(&dependency.module_name),
    });
    let mut dependencies = dependencies.collect_vec();
    dependencies.sort_by(|left, right| {
        left.module_name.cmp(&right.module_name).then_with(|| left.file_id.cmp(&right.file_id))
    });

    Ok(Module {
        file_id: context.file_id,
        name: context.module_name,
        dependencies: dependencies.into(),
        surface,
        declarations: declarations.into(),
        storage: context.storage,
    })
}

struct RuntimeExports {
    local: FxHashSet<TermItemId>,
    surface: ModuleSurface,
}

fn runtime_exports(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    exports: &resolving::ExportedModule,
) -> ConversionResult<RuntimeExports> {
    let mut local = FxHashSet::default();
    for &term_id in exports.local.iter() {
        let Some(global) = local_runtime_export(context, term_id)? else {
            continue;
        };
        let GlobalId::Term(file_id, term_id) = global.id else {
            unreachable!("invariant violated: resolved term export became an instance")
        };
        if file_id == context.file_id {
            local.insert(term_id);
        }
    }

    let mut indirect = Vec::new();
    for exports in exports.indirect.iter() {
        let mut globals = Vec::new();
        for &term_id in exports.terms.iter() {
            if let Some(global) = runtime_term_global(context, exports.file_id, term_id)? {
                globals.push(global);
            }
        }
        globals.sort_by(|left, right| left.item_name.cmp(&right.item_name));
        if !globals.is_empty() {
            indirect
                .push(IndirectModuleExports { file_id: exports.file_id, globals: globals.into() });
        }
    }
    indirect.sort_by(|left, right| {
        context.dependencies[&left.file_id]
            .module_name
            .cmp(&context.dependencies[&right.file_id].module_name)
            .then_with(|| left.file_id.cmp(&right.file_id))
    });

    let surface = ModuleSurface { indirect: indirect.into() };
    Ok(RuntimeExports { local, surface })
}

fn local_runtime_export(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    term_id: TermItemId,
) -> ConversionResult<Option<Global>> {
    let indexed = Arc::clone(&context.indexed);
    if !matches!(indexed.items[term_id].kind, IndexedTermItemKind::Operator { .. }) {
        return runtime_term_global(context, context.file_id, term_id);
    }

    let resolution = context.lowered.tree.get_term_item_kind(term_id).and_then(|kind| match kind {
        lowering::TermItemKind::Operator { resolution, .. } => *resolution,
        _ => None,
    });
    let Some((file_id, term_id)) = resolution else {
        let state = UnsupportedState::MissingRuntimeExportOperatorResolution { term_id };
        return Err(context.unsupported(state));
    };
    if file_id != context.file_id {
        return Ok(None);
    }
    runtime_term_global(context, file_id, term_id)
}

fn runtime_term_global(
    context: &mut Context<'_, impl checking::ExternalQueries>,
    file_id: FileId,
    term_id: TermItemId,
) -> ConversionResult<Option<Global>> {
    context.validate_runtime_reference(file_id, term_id)?;
    let indexed = context.indexed_module(file_id)?;
    match &indexed.items[term_id].kind {
        IndexedTermItemKind::Value { .. } | IndexedTermItemKind::Foreign { .. } => {
            Ok(Some(context.term_global(file_id, term_id)?))
        }
        IndexedTermItemKind::Constructor { .. } => {
            if context.constructor_is_newtype(file_id, term_id)? {
                Ok(None)
            } else {
                Ok(Some(context.term_global(file_id, term_id)?))
            }
        }
        IndexedTermItemKind::ClassMember { .. } => Ok(Some(context.term_global(file_id, term_id)?)),
        IndexedTermItemKind::Operator { .. } => Ok(None),
    }
}

fn validate_runtime_exports(
    context: &Context<'_, impl checking::ExternalQueries>,
    declarations: &[Declaration],
    surface: &ModuleSurface,
) -> ConversionResult<()> {
    let local = declarations.iter().filter(|declaration| declaration.exported);
    let local = local.map(|declaration| &declaration.global);
    let indirect = surface.indirect.iter().flat_map(|exports| exports.globals.iter());
    let globals = local.chain(indirect);
    let mut names = FxHashMap::default();
    for global in globals {
        if let Some(existing) = names.insert(SmolStr::clone(&global.item_name), global.id)
            && existing != global.id
        {
            let state = UnsupportedState::ConflictingRuntimeExport {
                name: global.item_name.to_string(),
                existing,
                duplicate: global.id,
            };
            return Err(context.unsupported(state));
        }
    }
    Ok(())
}

impl<'c, Q> Context<'c, Q>
where
    Q: checking::ExternalQueries,
{
    fn checked_binder_parameter(
        &mut self,
        binder_id: checking_tree::BinderId,
    ) -> ConversionResult<Parameter> {
        let source = self.binding_source(binder_id);
        let name = self.checked_binder_name(binder_id);
        self.parameter(source, name)
    }

    fn binding_source(&self, binder_id: checking_tree::BinderId) -> BindingSource {
        match self.checked.tree[binder_id].source {
            checking_tree::BinderSource::Binder(source) => BindingSource::SourceBinder(source),
            _ => BindingSource::CheckedBinder(binder_id),
        }
    }

    fn checked_binder_name(&self, binder_id: checking_tree::BinderId) -> SmolStr {
        let binder = &self.checked.tree[binder_id];
        match &binder.kind {
            checking_tree::BinderKind::Named { name, .. } => name.clone(),
            _ => match binder.source {
                checking_tree::BinderSource::Binder(source) => self.source_binder_name(source),
                checking_tree::BinderSource::Generated { name, .. } => {
                    SmolStr::clone(self.queries.lookup_smol_str(name))
                }
                checking_tree::BinderSource::Section(source) => {
                    format_smolstr!("section{}", source.into_raw().get())
                }
                _ => format_smolstr!("local{}", binder_id.into_raw().into_u32()),
            },
        }
    }

    fn source_binder_name(&self, binder: lowering::BinderId) -> SmolStr {
        match self.lowered.tree.get_binder_kind(binder) {
            Some(lowering::BinderKind::Variable { variable: Some(name) }) => name.clone(),
            Some(lowering::BinderKind::Named { named: Some(name), .. }) => name.clone(),
            _ => format_smolstr!("binder{}", binder.into_raw().get()),
        }
    }

    fn local_parameter(
        &mut self,
        source: lowering::LetBindingNameGroupId,
    ) -> ConversionResult<Parameter> {
        let group = self.lowered.tree.get_let_binding_group(source);
        let name = group
            .name
            .clone()
            .unwrap_or_else(|| format_smolstr!("binding{}", source.into_raw().into_u32()));
        self.parameter(BindingSource::Let(source), name)
    }

    fn record_pun_parameter(
        &mut self,
        source: lowering::RecordPunId,
        name: SmolStr,
    ) -> ConversionResult<Parameter> {
        self.parameter(BindingSource::RecordPun(source), name)
    }

    fn type_parameter_name(
        &self,
        type_id: checking::TypeId,
        fallback: SmolStr,
    ) -> QueryResult<SmolStr> {
        let name = match *self.queries.lookup_type(type_id) {
            checking::Type::Application(function, _)
            | checking::Type::KindApplication(function, _)
            | checking::Type::Kinded(function, _)
            | checking::Type::Forall(_, function)
            | checking::Type::Constrained(_, function) => {
                return self.type_parameter_name(function, fallback);
            }
            checking::Type::Function(_, _) => Some(SmolStr::new("function")),
            checking::Type::Constructor(file_id, type_id) => self
                .type_item_name(file_id, type_id)?
                .and_then(|name| lowercase_initial(&name).map(SmolStr::new)),
            checking::Type::Row(_) => Some(SmolStr::new("record")),
            checking::Type::Rigid(rigid, _, _) => self.rigid_type_name(rigid)?,
            checking::Type::Integer(_)
            | checking::Type::String(..)
            | checking::Type::Unification(_)
            | checking::Type::Free(_)
            | checking::Type::Unknown(_) => None,
        };
        let name = name.unwrap_or(fallback);
        Ok(format_smolstr!("${name}"))
    }

    fn rigid_type_name(&self, rigid: checking::core::Name) -> QueryResult<Option<SmolStr>> {
        let checked = if rigid.file == self.file_id {
            Arc::clone(&self.checked)
        } else {
            self.queries.checked(rigid.file)?
        };
        Ok(checked
            .lookup_name(rigid)
            .map(|name| SmolStr::clone(self.queries.lookup_smol_str(name))))
    }

    fn parameter(&mut self, source: BindingSource, name: SmolStr) -> ConversionResult<Parameter> {
        if let Some(parameter) = self.parameters.get(&source) {
            return Ok(parameter.clone());
        }
        let parameter = self.fresh_parameter(name)?;
        self.parameters.insert(source, parameter.clone());
        Ok(parameter)
    }

    fn fresh_parameter(&mut self, name: SmolStr) -> ConversionResult<Parameter> {
        let id = LocalId(self.next_local);
        self.next_local = self
            .next_local
            .checked_add(1)
            .ok_or_else(|| self.unsupported(UnsupportedState::LocalIdentityOverflow))?;
        Ok(Parameter { id, name })
    }

    fn fresh_generated_global(&mut self, item_name: SmolStr) -> ConversionResult<Global> {
        let id = GeneratedGlobalId(self.next_generated_global);
        self.next_generated_global = self
            .next_generated_global
            .checked_add(1)
            .ok_or_else(|| self.unsupported(UnsupportedState::GeneratedGlobalIdentityOverflow))?;
        Ok(Global { id: GlobalId::Generated(self.file_id, id), item_name })
    }

    fn parameter_abstraction(
        &mut self,
        parameters: impl IntoIterator<Item = Parameter>,
        body: ExpressionId,
    ) -> ExpressionId {
        let patterns =
            parameters.into_iter().map(|parameter| self.pattern(PatternKind::Variable(parameter)));
        let patterns = patterns.collect_vec();
        self.abstraction(patterns, body)
    }

    fn abstraction(&mut self, parameters: Vec<PatternId>, body: ExpressionId) -> ExpressionId {
        if parameters.is_empty() {
            body
        } else {
            self.expression(ExpressionKind::Abstraction { parameters: parameters.into(), body })
        }
    }

    fn expression(&mut self, kind: ExpressionKind) -> ExpressionId {
        self.storage.allocate_expression(Expression { kind })
    }

    fn pattern(&mut self, kind: PatternKind) -> PatternId {
        self.storage.allocate_pattern(Pattern { kind })
    }

    fn label_field(&self, label: SmolStr) -> Field {
        Field { identity: FieldIdentity::Label(SmolStr::clone(&label)), name: label }
    }

    fn member_field(&self, (file_id, term_id): (FileId, TermItemId)) -> QueryResult<Field> {
        let indexed = self.indexed_module(file_id)?;
        let name = match &indexed.items[term_id].name {
            Some(name) => SmolStr::clone(name),
            None => self.term_fallback(term_id),
        };
        Ok(Field { identity: FieldIdentity::Member(file_id, term_id), name })
    }

    fn superclass_field(
        &self,
        superclass: checking::evidence::SuperclassId,
    ) -> ConversionResult<Field> {
        let identity = SuperclassIdentity {
            file_id: superclass.file_id,
            class: superclass.type_id,
            source: superclass.source_id,
        };
        let checked = if superclass.file_id == self.file_id {
            Arc::clone(&self.checked)
        } else {
            self.queries.checked(superclass.file_id)?
        };
        let declaration = checked.tree.lookup_type_declaration(superclass.type_id);
        let class = declaration.and_then(|declaration| {
            let declaration = &checked.tree[declaration];
            if let checking_tree::TypeDeclarationKind::Class(class) = &declaration.declaration {
                Some(class)
            } else {
                None
            }
        });
        let candidate = class.and_then(|class| {
            class.superclasses.iter().enumerate().find(|(_, candidate)| candidate.id == superclass)
        });
        let (position, base) = match candidate {
            Some((position, candidate)) => {
                let base = self.constraint_class_name(candidate.constraint)?;
                (position, base.unwrap_or_else(|| SmolStr::new("Superclass")))
            }
            None => (0, SmolStr::new("Superclass")),
        };
        let name = format_smolstr!("{base}{position}");
        Ok(Field { identity: FieldIdentity::Superclass(identity), name })
    }

    fn constraint_class_name(&self, constraint: checking::TypeId) -> QueryResult<Option<SmolStr>> {
        let mut current = constraint;
        loop {
            match *self.queries.lookup_type(current) {
                checking::Type::Application(function, _)
                | checking::Type::KindApplication(function, _)
                | checking::Type::Kinded(function, _) => current = function,
                checking::Type::Constructor(file_id, type_id) => {
                    return self.type_item_name(file_id, type_id);
                }
                checking::Type::Forall(_, _)
                | checking::Type::Constrained(_, _)
                | checking::Type::Function(_, _)
                | checking::Type::Row(_)
                | checking::Type::Rigid(_, _, _)
                | checking::Type::Integer(_)
                | checking::Type::String(..)
                | checking::Type::Unification(_)
                | checking::Type::Free(_)
                | checking::Type::Unknown(_) => return Ok(None),
            }
        }
    }

    fn source_module_name(&self, file_id: FileId) -> QueryResult<SmolStr> {
        if file_id == self.file_id {
            return Ok(SmolStr::clone(&self.module_name));
        }
        if let Some(dependency) = self.dependencies.get(&file_id) {
            return Ok(SmolStr::clone(&dependency.module_name));
        }
        let content = self.queries.content(file_id)?;
        let (parsed, _) = self.queries.parsed(file_id)?;
        let name = parsed
            .module_name(&content)
            .expect("invariant violated: referenced checked module has no source module name");
        Ok(name)
    }

    fn indexed_module(&self, file_id: FileId) -> QueryResult<Arc<indexing::IndexedModule>> {
        if file_id == self.file_id {
            Ok(Arc::clone(&self.indexed))
        } else if let Some(indexed) =
            self.dependencies.get(&file_id).and_then(|dependency| dependency.indexed.as_ref())
        {
            Ok(Arc::clone(indexed))
        } else {
            if let Some(indexed) = self.indexed_dependencies.borrow().get(&file_id) {
                return Ok(Arc::clone(indexed));
            }
            let indexed = self.queries.indexed(file_id)?;
            self.indexed_dependencies.borrow_mut().insert(file_id, Arc::clone(&indexed));
            Ok(indexed)
        }
    }

    fn type_item_name(
        &self,
        file_id: FileId,
        type_id: indexing::TypeItemId,
    ) -> QueryResult<Option<SmolStr>> {
        let indexed = self.indexed_module(file_id)?;
        Ok(indexed.items[type_id].name.clone())
    }

    fn term_global(&mut self, file_id: FileId, term_id: TermItemId) -> ConversionResult<Global> {
        let indexed = self.indexed_module(file_id)?;
        let item_name = if let Some(name) = &indexed.items[term_id].name {
            SmolStr::clone(name)
        } else {
            self.term_fallback(term_id)
        };
        self.register_dependency(file_id, Some(Arc::clone(&indexed)))?;
        Ok(Global { id: GlobalId::Term(file_id, term_id), item_name })
    }

    fn constructor_is_newtype(&self, file_id: FileId, term_id: TermItemId) -> QueryResult<bool> {
        let indexed = self.indexed_module(file_id)?;
        let Some(type_id) = indexed.constructor_type(term_id) else {
            return Ok(false);
        };
        Ok(matches!(indexed.items[type_id].kind, IndexedTypeItemKind::Newtype { .. }))
    }

    fn constructor_arity(&self, file_id: FileId, term_id: TermItemId) -> ConversionResult<usize> {
        let checked = if file_id == self.file_id {
            Arc::clone(&self.checked)
        } else {
            self.queries.checked(file_id)?
        };
        let declaration_id = checked
            .tree
            .lookup_term(term_id)
            .ok_or_else(|| self.unsupported(UnsupportedState::MissingTermDeclaration(term_id)))?;
        let declaration = &checked.tree[declaration_id];
        let checking_tree::TermDeclarationKind::Constructor(constructor) = &declaration.kind else {
            unreachable!("invariant violated: constructor expression resolved to another term kind")
        };
        Ok(constructor.arguments.len())
    }

    fn instance_global(&mut self, origin: InstanceCandidateOrigin) -> ConversionResult<Global> {
        let identity = match origin {
            InstanceCandidateOrigin::Instance(file_id, id) => {
                InstanceIdentity::Declared(file_id, id)
            }
            InstanceCandidateOrigin::Derive(file_id, id) => InstanceIdentity::Derived(file_id, id),
        };
        let item_name = self.instance_name(identity)?;
        let file_id = match identity {
            InstanceIdentity::Declared(file_id, _) | InstanceIdentity::Derived(file_id, _) => {
                file_id
            }
        };
        self.register_dependency(file_id, None)?;
        Ok(Global { id: GlobalId::Instance(identity), item_name })
    }

    fn register_dependency(
        &mut self,
        file_id: FileId,
        indexed: Option<Arc<indexing::IndexedModule>>,
    ) -> QueryResult<()> {
        if file_id == self.file_id
            || self.module_is_virtual(file_id)
            || self.dependencies.contains_key(&file_id)
        {
            return Ok(());
        }
        let module_name = self.source_module_name(file_id)?;
        self.dependencies.insert(file_id, Dependency { module_name, indexed });
        Ok(())
    }

    fn instance_name(&self, identity: InstanceIdentity) -> QueryResult<SmolStr> {
        let origin = match identity {
            InstanceIdentity::Declared(file_id, id) => {
                InstanceCandidateOrigin::Instance(file_id, id)
            }
            InstanceIdentity::Derived(file_id, id) => InstanceCandidateOrigin::Derive(file_id, id),
        };
        let pretty = checking::tree::pretty::Pretty::new(self.queries, &self.checked);
        pretty.render_instance_name_with_cache(
            self.file_id,
            origin,
            &mut self.instance_names.borrow_mut(),
        )
    }

    fn record_pun_source(
        &self,
        binder_id: checking_tree::BinderId,
        label: &str,
    ) -> Option<lowering::RecordPunId> {
        let checking_tree::BinderSource::Binder(source) = self.checked.tree[binder_id].source
        else {
            return None;
        };
        let lowering::BinderKind::Record { record } = self.lowered.tree.get_binder_kind(source)?
        else {
            return None;
        };
        record.iter().find_map(|field| {
            let lowering::BinderRecordItem::RecordPun { id, name } = field else {
                return None;
            };
            name.as_deref().filter(|name| *name == label).map(|_| *id)
        })
    }

    fn record_pun_name(&self, source: lowering::RecordPunId) -> SmolStr {
        match self.record_pun_names.get(&source) {
            Some(name) => SmolStr::clone(name),
            None => format_smolstr!("pun{}", source.into_raw().get()),
        }
    }

    fn term_fallback(&self, term_id: TermItemId) -> SmolStr {
        format_smolstr!("term{}", term_id.into_raw().into_u32())
    }

    fn unsupported(&self, state: UnsupportedState) -> ConversionError {
        crate::ModuleError::Unsupported { file_id: self.file_id, state }.into()
    }
}

fn lowercase_initial(name: &str) -> Option<String> {
    let mut characters = name.chars();
    let first = characters.next()?;
    let first = first.to_lowercase().collect::<String>();
    Some(format!("{first}{}", characters.as_str()))
}
