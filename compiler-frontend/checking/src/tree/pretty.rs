//! Implements the pretty printer for the checked semantic tree.

use std::cell::RefCell;
use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{IndexedTermItem, IndexedTypeItem, InstanceSourceItemId, OrderedTermItemId};
use lowering::TermVariableResolution;
use pretty::{Arena, DocAllocator, DocBuilder};
use rustc_hash::FxHashMap;
use smol_str::{SmolStr, SmolStrBuilder, format_smolstr};

use crate::CheckedModule;
use crate::core::Type;
use crate::core::pretty::{
    Pretty as TypePretty, PrettyConfig as TypePrettyConfig, PrettyNames, PrettyQueries,
    PrettyState as TypePrettyState, breakable_continuation,
};
use crate::evidence::{
    Evidence, EvidenceBinderId, EvidenceState, EvidenceVarId, InstanceCandidateOrigin,
    ReflectableEvidence, ReflectableOrdering, SuperclassId, SynthesizedEvidence,
};
use crate::tree::{
    BinderId, BinderKind, BinderSource, CaseAlternative, DeclarationAbstraction, Equation,
    ExpressionId, ExpressionKind, GuardedAlternative, GuardedExpression, InstanceDeclaration,
    InstanceImplementation, InstanceMember, LetBindingChunk, LetBindings, LocalDeclarationId,
    PatternGuard, RecordBinderField, RecordExpressionField, RecordExpressionUpdate,
    TermDeclarationId, TermDeclarationKind, TypeDeclarationKind, VariableResolution,
    WhereExpression,
};

type Doc<'a> = DocBuilder<'a, Arena<'a>, ()>;

const DEFAULT_WIDTH: usize = 100;

const UNKNOWN_INSTANCE_EVIDENCE: SmolStr = SmolStr::new_static("<instance>");
const UNKNOWN_SUPERCLASS_EVIDENCE: SmolStr = SmolStr::new_static("<superclass>");
const EVIDENCE_DICTIONARY_NAME: SmolStr = SmolStr::new_static("evidenceDict");
const REFLECTABLE_LESS_EVIDENCE: SmolStr = SmolStr::new_static("reflectable(LT)");
const REFLECTABLE_EQUAL_EVIDENCE: SmolStr = SmolStr::new_static("reflectable(EQ)");
const REFLECTABLE_GREATER_EVIDENCE: SmolStr = SmolStr::new_static("reflectable(GT)");

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ExpressionPrecedence {
    Abstraction,
    Application,
    RecordUpdate,
    Atom,
}

#[derive(Clone, Copy)]
enum EvidencePrecedence {
    Application,
    Atom,
}

impl EvidencePrecedence {
    fn append_projection(self, output: &mut SmolStrBuilder, parent: &str, field: &str) {
        match self {
            EvidencePrecedence::Application => {
                output.push('(');
                output.push_str(parent);
                output.push(')');
            }
            EvidencePrecedence::Atom => output.push_str(parent),
        }
        output.push('.');
        output.push_str(field);
    }
}

fn character_literal(value: char) -> String {
    match value {
        '\n' => "'\\n'".to_string(),
        '\r' => "'\\r'".to_string(),
        '\t' => "'\\t'".to_string(),
        '\\' => "'\\\\'".to_string(),
        '\'' => "'\\\''".to_string(),
        '\0' => "'\\0'".to_string(),
        value if value.is_control() => format!("'\\x{:02x}'", value as u32),
        value => format!("'{value}'"),
    }
}

fn section_name(source: lowering::ExpressionId) -> String {
    format!("v{}", source.into_raw().get())
}

struct EvidenceNames {
    display_by_binder: FxHashMap<EvidenceBinderId, SmolStr>,
    names: PrettyNames,
}

impl EvidenceNames {
    fn new() -> EvidenceNames {
        EvidenceNames { display_by_binder: FxHashMap::default(), names: PrettyNames::new() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrettyConfig {
    width: usize,
    fully_qualified_names: bool,
}

impl PrettyConfig {
    pub const fn new() -> PrettyConfig {
        PrettyConfig { width: DEFAULT_WIDTH, fully_qualified_names: false }
    }

    #[must_use]
    pub const fn width(mut self, width: usize) -> PrettyConfig {
        self.width = width;
        self
    }

    #[must_use]
    pub const fn fully_qualified_names(mut self) -> PrettyConfig {
        self.fully_qualified_names = true;
        self
    }
}

impl Default for PrettyConfig {
    fn default() -> PrettyConfig {
        PrettyConfig::new()
    }
}

pub struct Pretty<'a, Q: ?Sized> {
    queries: &'a Q,
    checked: &'a CheckedModule,
    config: PrettyConfig,
}

#[derive(Default)]
pub struct InstanceNames {
    names: FxHashMap<InstanceCandidateOrigin, SmolStr>,
    modules: FxHashMap<FileId, ModuleInstanceNames>,
}

struct ModuleInstanceNames {
    names: PrettyNames,
    next_source: usize,
}

impl<'a, Q> Pretty<'a, Q>
where
    Q: PrettyQueries<Checked = Arc<CheckedModule>> + ?Sized,
{
    pub fn new(queries: &'a Q, checked: &'a CheckedModule) -> Pretty<'a, Q> {
        Pretty::with_config(queries, checked, PrettyConfig::default())
    }

    pub fn with_config(
        queries: &'a Q,
        checked: &'a CheckedModule,
        config: PrettyConfig,
    ) -> Pretty<'a, Q> {
        Pretty { queries, checked, config }
    }

    pub fn render(&self, file_id: FileId) -> QueryResult<SmolStr> {
        let indexed = self.queries.indexed(file_id)?;
        let lowered = self.queries.lowered(file_id)?;
        let arena = Arena::new();
        let mut printer = Printer::new(
            &arena,
            self.queries,
            file_id,
            &indexed,
            &lowered,
            self.checked,
            self.config,
        );
        let document = printer.module()?;

        let mut output = SmolStrBuilder::new();
        document
            .render_fmt(self.config.width, &mut output)
            .expect("critical failure: failed to render checked semantic tree");
        Ok(output.finish())
    }

    pub fn render_instance_name(
        &self,
        file_id: FileId,
        origin: InstanceCandidateOrigin,
    ) -> QueryResult<SmolStr> {
        self.render_instance_name_with_cache(file_id, origin, &mut InstanceNames::default())
    }

    pub fn render_instance_name_with_cache(
        &self,
        file_id: FileId,
        origin: InstanceCandidateOrigin,
        names: &mut InstanceNames,
    ) -> QueryResult<SmolStr> {
        if let Some(name) = names.names.get(&origin) {
            return Ok(SmolStr::clone(name));
        }
        let indexed = self.queries.indexed(file_id)?;
        let lowered = self.queries.lowered(file_id)?;
        let arena = Arena::new();
        let printer = Printer::new(
            &arena,
            self.queries,
            file_id,
            &indexed,
            &lowered,
            self.checked,
            self.config,
        );
        printer.instance_dictionary_name_with_cache(origin, names)
    }
}

struct Printer<'arena, 'context, 'module, Q>
where
    Q: PrettyQueries<Checked = Arc<CheckedModule>> + ?Sized,
{
    arena: &'arena Arena<'arena>,
    queries: &'context Q,
    file_id: FileId,
    indexed: &'module indexing::IndexedModule,
    lowered: &'module lowering::LoweredModule,
    checked: &'context CheckedModule,
    type_pretty: TypePretty<'context, Q>,
    signature_type_pretty: TypePretty<'context, Q>,
    fully_qualified_names: bool,
    instance_names: RefCell<InstanceNames>,
}

impl<'arena, 'context, 'module, Q> Printer<'arena, 'context, 'module, Q>
where
    Q: PrettyQueries<Checked = Arc<CheckedModule>> + ?Sized,
{
    fn new(
        arena: &'arena Arena<'arena>,
        queries: &'context Q,
        file_id: FileId,
        indexed: &'module indexing::IndexedModule,
        lowered: &'module lowering::LoweredModule,
        checked: &'context CheckedModule,
        config: PrettyConfig,
    ) -> Printer<'arena, 'context, 'module, Q> {
        let mut type_config = TypePrettyConfig::new().without_rigid_kinds().width(config.width);
        if config.fully_qualified_names {
            type_config = type_config.fully_qualified_names();
        }
        let signature_type_config = type_config.without_forall_kinds();
        let type_pretty = TypePretty::with_config(queries, checked, type_config);
        let signature_type_pretty =
            TypePretty::with_config(queries, checked, signature_type_config);
        Printer {
            arena,
            queries,
            file_id,
            indexed,
            lowered,
            checked,
            type_pretty,
            signature_type_pretty,
            fully_qualified_names: config.fully_qualified_names,
            instance_names: RefCell::new(InstanceNames::default()),
        }
    }

    fn display_local_type_name(&self, name: &str) -> String {
        if !self.fully_qualified_names {
            return name.to_string();
        }

        let Ok(content) = self.queries.content(self.file_id) else {
            return name.to_string();
        };

        let Ok((parsed, _)) = self.queries.parsed(self.file_id) else {
            return name.to_string();
        };

        let Some(module_name) = parsed.module_name(&content) else {
            return name.to_string();
        };

        format!("{module_name}.{name}")
    }

    fn module(&mut self) -> QueryResult<Doc<'arena>> {
        let mut declarations = vec![];

        for (type_id, IndexedTypeItem { name, .. }) in self.indexed.items.iter_types() {
            let Some(name) = name else { continue };
            let Some(declaration_id) = self.checked.tree.lookup_type_declaration(type_id) else {
                continue;
            };

            let declaration = &self.checked.tree[declaration_id];
            let (keyword, is_class) = match &declaration.declaration {
                TypeDeclarationKind::Data(_) => ("data", false),
                TypeDeclarationKind::Newtype(_) => ("newtype", false),
                TypeDeclarationKind::Class(_) => ("interface", true),
            };
            let kind = declaration.kind;

            let mut type_pretty = self.type_pretty.state();
            let signature = type_pretty.render_kind_signature(name, kind);
            let signature = self.arena.text(format!("{keyword} {signature}"));

            let declaration = if is_class {
                self.class_declaration(type_id, name, &mut type_pretty)?
            } else {
                self.data_declaration(type_id, keyword, name, &mut type_pretty)
            };

            declarations.push(signature.append(self.arena.hardline()).append(declaration));
        }

        let mut dictionary_names = PrettyNames::new();
        for (_, IndexedTermItem { name, .. }) in self.indexed.items.iter_terms() {
            if let Some(name) = name {
                dictionary_names.allocate_display_name(SmolStr::clone(name));
            }
        }

        for item_id in self.indexed.items.instance_sources() {
            let name = match item_id {
                InstanceSourceItemId::Instance(id) => &self.indexed.items[*id].name,
                InstanceSourceItemId::Derive(id) => &self.indexed.items[*id].name,
            };
            if let Some(name) = name {
                dictionary_names.allocate_display_name(SmolStr::clone(name));
            }
        }

        for &item_id in self.indexed.items.ordered_terms() {
            match item_id {
                OrderedTermItemId::Term(term_id) => {
                    let item = &self.indexed.items[term_id];
                    let Some(declaration_id) = self.checked.tree.lookup_term(term_id) else {
                        continue;
                    };
                    let declaration = &self.checked.tree[declaration_id];
                    match &declaration.kind {
                        TermDeclarationKind::Value(_) => {
                            let Some(name) = &item.name else { continue };
                            let Some(declaration) = self.value_declaration(term_id, name)? else {
                                continue;
                            };
                            declarations.push(declaration);
                        }
                        TermDeclarationKind::Foreign => {
                            let Some(name) = &item.name else { continue };
                            let type_id = self.signature_type_pretty.render(declaration.type_id);
                            let declaration =
                                self.arena.text(format!("foreign import {name} :: {type_id}"));
                            declarations.push(declaration);
                        }
                        TermDeclarationKind::Constructor(_) => {}
                        TermDeclarationKind::Instance(_) => {
                            unreachable!("invariant violated: instance stored as a term symbol")
                        }
                    }
                }
                OrderedTermItemId::Instance(id) => {
                    let name = &self.indexed.items[id].name;
                    let declaration_id = self.checked.tree.lookup_instance(id);
                    self.push_instance_declaration(
                        &mut declarations,
                        &mut dictionary_names,
                        name,
                        declaration_id,
                    )?;
                }
                OrderedTermItemId::Derive(id) => {
                    let name = &self.indexed.items[id].name;
                    let declaration_id = self.checked.tree.lookup_derive(id);
                    self.push_instance_declaration(
                        &mut declarations,
                        &mut dictionary_names,
                        name,
                        declaration_id,
                    )?;
                }
            }
        }

        let mut declarations = declarations.into_iter();
        if let Some(first) = declarations.next() {
            Ok(declarations.fold(first, |document, declaration| {
                document
                    .append(self.arena.hardline())
                    .append(self.arena.hardline())
                    .append(declaration)
            }))
        } else {
            Ok(self.arena.nil())
        }
    }

    fn push_instance_declaration(
        &mut self,
        declarations: &mut Vec<Doc<'arena>>,
        term_names: &mut PrettyNames,
        name: &Option<SmolStr>,
        declaration_id: Option<TermDeclarationId>,
    ) -> QueryResult<()> {
        let Some(declaration_id) = declaration_id else { return Ok(()) };
        let declaration = &self.checked.tree[declaration_id];
        let TermDeclarationKind::Instance(instance) = &declaration.kind else { return Ok(()) };
        let name = if let Some(name) = name {
            SmolStr::clone(name)
        } else {
            let base = self.dictionary_base_name(declaration.type_id, instance)?;
            term_names.allocate_display_name(base)
        };
        let declaration = self.instance_declaration(declaration_id, &name)?;
        declarations.push(declaration);
        Ok(())
    }

    fn data_declaration(
        &mut self,
        type_id: indexing::TypeItemId,
        keyword: &str,
        name: &str,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> Doc<'arena> {
        let declaration_id = self
            .checked
            .tree
            .lookup_type_declaration(type_id)
            .expect("invariant violated: missing checked type declaration");
        let declaration = &self.checked.tree[declaration_id];
        let data = match &declaration.declaration {
            TypeDeclarationKind::Data(data) | TypeDeclarationKind::Newtype(data) => data,
            TypeDeclarationKind::Class(_) => {
                unreachable!("invariant violated: class is not a data declaration")
            }
        };

        let mut parameter_names = vec![];
        for &parameter in data.parameters.iter() {
            let parameter = self.queries.lookup_forall_binder(parameter);
            let name = type_pretty.display_name(parameter.name);
            parameter_names.push((name.to_string(), parameter.visible));
        }

        let mut head = self.arena.text(format!("{keyword} {name}"));
        for (parameter, visible) in &parameter_names {
            let parameter = if *visible { format!("@{parameter}") } else { parameter.to_string() };
            head = head.append(self.arena.text(format!(" {parameter}")));
        }

        let mut declaration = head;
        let constructors = self.indexed.data_constructors(type_id);
        for (&declaration_id, constructor_id) in data.constructors.iter().zip(constructors) {
            let IndexedTermItem { name: constructor_name, .. } =
                &self.indexed.items[constructor_id];
            let Some(constructor_name) = constructor_name else { continue };
            let constructor = &self.checked.tree[declaration_id];
            let TermDeclarationKind::Constructor(constructor) = &constructor.kind else {
                unreachable!("invariant violated: data declaration contains a value declaration");
            };

            let result_name = self.display_local_type_name(name);
            let mut result = self.arena.text(result_name);
            for (parameter, _) in &parameter_names {
                result = result.append(self.arena.text(format!(" {parameter}")));
            }

            let mut constructor_type = result;
            for &argument_id in constructor.arguments.iter().rev() {
                let argument = type_pretty.render(argument_id);
                let argument = match *self.queries.lookup_type(argument_id) {
                    Type::Forall(..)
                    | Type::Constrained(..)
                    | Type::Function(..)
                    | Type::Kinded(..) => format!("({argument})"),
                    _ => argument.to_string(),
                };

                let result = constructor_type;
                constructor_type = self
                    .arena
                    .text(argument)
                    .append(self.arena.text(" ->"))
                    .append(self.arena.line().append(result).nest(2))
                    .group();
            }

            let constructor_type = self.arena.line().append(constructor_type).nest(4);
            let constructor = self
                .arena
                .text(format!("  | {constructor_name} ::"))
                .append(constructor_type)
                .group();
            declaration = declaration.append(self.arena.hardline()).append(constructor);
        }

        declaration
    }

    fn class_declaration(
        &mut self,
        type_id: indexing::TypeItemId,
        name: &str,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let declaration_id = self
            .checked
            .tree
            .lookup_type_declaration(type_id)
            .expect("invariant violated: missing checked type declaration");
        let declaration = &self.checked.tree[declaration_id];
        let TypeDeclarationKind::Class(class) = &declaration.declaration else {
            unreachable!("invariant violated: type declaration is not a class");
        };

        for &parameter in class.kind_binders.iter() {
            let parameter = self.queries.lookup_forall_binder(parameter);
            type_pretty.display_name(parameter.name);
        }

        let mut head = self.arena.text(format!("interface {name}"));
        for &parameter in class.type_parameters.iter() {
            let parameter = self.queries.lookup_forall_binder(parameter);
            let parameter = type_pretty.display_name(parameter.name);
            head = head.append(self.arena.text(format!(" {parameter}")));
        }
        let mut declaration = head.append(self.arena.text(" where"));

        let mut field_names = PrettyNames::new();
        for member_id in self.indexed.class_members(type_id) {
            let IndexedTermItem { name: Some(name), .. } = &self.indexed.items[member_id] else {
                continue;
            };
            field_names.allocate_display_name(SmolStr::clone(name));
        }

        for superclass in class.superclasses.iter() {
            let base = self.evidence_base_name(superclass.constraint)?;
            let field_name = field_names.allocate_display_name(base);
            let field_type = self.type_pretty.render(superclass.constraint);
            let field = self.arena.text(format!("  superclass {field_name} :: {field_type}"));
            declaration = declaration.append(self.arena.hardline()).append(field);
        }

        for member in class.members.iter() {
            let IndexedTermItem { name: Some(name), .. } = &self.indexed.items[member.source]
            else {
                continue;
            };
            let field_type = self.type_pretty.render(member.field_type);
            let field = self.arena.text(format!("  {name} :: {field_type}"));
            declaration = declaration.append(self.arena.hardline()).append(field);
        }

        Ok(declaration)
    }

    fn value_declaration(
        &mut self,
        term_id: indexing::TermItemId,
        name: &str,
    ) -> QueryResult<Option<Doc<'arena>>> {
        let declaration_id = self
            .checked
            .tree
            .lookup_term(term_id)
            .expect("invariant violated: missing checked term declaration");
        let declaration = &self.checked.tree[declaration_id];
        let TermDeclarationKind::Value(value) = &declaration.kind else {
            unreachable!("invariant violated: term declaration is not a value");
        };

        let mut signature_pretty = self.signature_type_pretty.state();
        let type_id = signature_pretty.render(declaration.type_id);
        let signature = self.arena.text(format!("{name} :: {type_id}"));
        let rigid_names = self.declaration_rigid_names(&mut signature_pretty, &value.abstractions);

        let mut evidence_names = EvidenceNames::new();
        for abstraction in value.abstractions.iter() {
            if let DeclarationAbstraction::Evidence { binder, .. } = abstraction {
                self.evidence_binder_name(&mut evidence_names, *binder)?;
            }
        }
        let mut equation_type_pretty = self.type_pretty.state();
        self.assign_rigid_names(&mut equation_type_pretty, &rigid_names);

        let Some(equations) = self.equation_declarations(
            name,
            "",
            &value.abstractions,
            &value.equations,
            &mut evidence_names,
            &mut equation_type_pretty,
        )?
        else {
            return Ok(None);
        };

        Ok(Some(signature.append(self.arena.hardline()).append(equations)))
    }

    fn instance_declaration(
        &mut self,
        declaration_id: TermDeclarationId,
        name: &str,
    ) -> QueryResult<Doc<'arena>> {
        let declaration = &self.checked.tree[declaration_id];
        let TermDeclarationKind::Instance(instance) = &declaration.kind else {
            unreachable!("invariant violated: term declaration is not an instance");
        };

        let mut outer_evidence_names = EvidenceNames::new();
        for evidence in instance.evidences.iter() {
            if let Evidence::Given(binder) = &evidence.evidence {
                self.evidence_binder_name(&mut outer_evidence_names, *binder)?;
            }
        }

        let (signature, rigid_names) = self.dictionary_signature(
            name,
            declaration.type_id,
            instance,
            &mut outer_evidence_names,
        )?;

        if let InstanceImplementation::Delegate { evidence, .. } = &instance.implementation {
            let evidence = self.evidence_variable_name(&mut outer_evidence_names, *evidence)?;
            let delegation = self.arena.text(format!(" = {evidence}"));
            return Ok(signature.append(delegation));
        }

        let mut fields = vec![];

        let superclass_names = self.instance_superclass_field_names(instance)?;
        for (superclass, field_name) in instance.superclasses.iter().zip(superclass_names) {
            let evidence =
                self.evidence_variable_name(&mut outer_evidence_names, superclass.evidence)?;
            let field = self.arena.text(format!("  superclass {field_name} = {evidence}"));
            fields.push(field);
        }

        match &instance.implementation {
            InstanceImplementation::Members(members) => {
                for member in members.iter() {
                    let Some(member_name) =
                        self.term_name(member.resolution.0, member.resolution.1)?
                    else {
                        continue;
                    };

                    let mut evidence_names = EvidenceNames::new();
                    let instance_binders = instance.evidences.iter().filter_map(|evidence| {
                        if let Evidence::Given(binder) = &evidence.evidence {
                            Some(binder)
                        } else {
                            None
                        }
                    });
                    let member_binders =
                        member.abstractions.iter().filter_map(|abstraction| match abstraction {
                            DeclarationAbstraction::Evidence { binder, .. } => Some(binder),
                            DeclarationAbstraction::Type { .. } => None,
                        });
                    for binder in instance_binders.chain(member_binders) {
                        self.evidence_binder_name(&mut evidence_names, *binder)?;
                    }

                    let (signature, member_rigid_names) =
                        self.instance_member_signature(member, &member_name, &rigid_names)?;
                    let equation_rigid_names =
                        rigid_names.iter().cloned().chain(member_rigid_names);
                    let equation_rigid_names = equation_rigid_names.collect::<Vec<_>>();
                    let mut equation_type_pretty = self.type_pretty.state();
                    self.assign_rigid_names(&mut equation_type_pretty, &equation_rigid_names);
                    let Some(equations) = self.equation_declarations(
                        &member_name,
                        "  ",
                        &member.abstractions,
                        &member.equations,
                        &mut evidence_names,
                        &mut equation_type_pretty,
                    )?
                    else {
                        continue;
                    };

                    fields.push(signature.append(self.arena.hardline()).append(equations));
                }
            }
            InstanceImplementation::Delegate { .. } => {
                unreachable!(
                    "invariant violated: delegated instances return before rendering members"
                )
            }
        }

        let mut fields = fields.into_iter();
        let Some(first) = fields.next() else { return Ok(signature) };
        let fields = fields
            .fold(first, |document, field| document.append(self.arena.hardline()).append(field));
        let where_clause = self.arena.line().append(self.arena.text("where")).nest(2);
        let header = signature.append(where_clause).group();
        Ok(header.append(self.arena.hardline()).append(fields))
    }

    fn instance_member_signature(
        &self,
        member: &InstanceMember,
        name: &str,
        rigid_names: &[(crate::TypeId, SmolStr)],
    ) -> QueryResult<(Doc<'arena>, Vec<(crate::TypeId, SmolStr)>)> {
        let mut type_pretty = self.type_pretty.state();

        for (rigid, display) in rigid_names {
            if let Type::Rigid(name, _, _) = *self.queries.lookup_type(*rigid) {
                type_pretty.assign_display_name(name, SmolStr::clone(display));
            }
        }

        for abstraction in member.abstractions.iter() {
            let DeclarationAbstraction::Type { binder, .. } = abstraction else {
                continue;
            };
            let binder = self.queries.lookup_forall_binder(*binder);
            let text = if member.resolution.0 == self.file_id {
                self.checked.lookup_name(binder.name)
            } else {
                self.queries.checked(member.resolution.0)?.lookup_name(binder.name)
            };
            if let Some(text) = text {
                let text = self.queries.lookup_smol_str(text);
                type_pretty.allocate_display_name(binder.name, SmolStr::clone(text));
            }
        }

        let type_id = type_pretty.render(member.implementation_type);
        let rigid_names = self.declaration_rigid_names(&mut type_pretty, &member.abstractions);
        let signature = self.arena.text(format!("  {name} :: {type_id}"));
        Ok((signature, rigid_names))
    }

    fn declaration_rigid_names(
        &self,
        type_pretty: &mut TypePrettyState<'context, Q>,
        abstractions: &[DeclarationAbstraction],
    ) -> Vec<(crate::TypeId, SmolStr)> {
        let rigid_names = abstractions.iter().filter_map(|abstraction| {
            let DeclarationAbstraction::Type { binder, rigid } = abstraction else {
                return None;
            };
            let binder = self.queries.lookup_forall_binder(*binder);
            let display = type_pretty.display_name(binder.name);
            Some((*rigid, display))
        });
        rigid_names.collect()
    }

    fn assign_rigid_names(
        &self,
        type_pretty: &mut TypePrettyState<'context, Q>,
        rigid_names: &[(crate::TypeId, SmolStr)],
    ) {
        for (rigid, display) in rigid_names {
            if let Type::Rigid(name, _, _) = *self.queries.lookup_type(*rigid) {
                type_pretty.assign_display_name(name, SmolStr::clone(display));
            }
        }
    }

    fn dictionary_signature(
        &self,
        name: &str,
        type_id: crate::TypeId,
        instance: &InstanceDeclaration,
        evidence_names: &mut EvidenceNames,
    ) -> QueryResult<(Doc<'arena>, Vec<(crate::TypeId, SmolStr)>)> {
        let mut binders = vec![];
        let mut current = type_id;
        while let Type::Forall(binder, inner) = *self.queries.lookup_type(current) {
            binders.push(binder);
            current = inner;
        }
        debug_assert_eq!(binders.len(), instance.rigid_parameters.len());

        while let Type::Constrained(_, inner) = *self.queries.lookup_type(current) {
            current = inner;
        }

        let mut type_pretty = self.type_pretty.state();
        let binder_names = binders.iter().map(|&binder| {
            let binder = self.queries.lookup_forall_binder(binder);
            type_pretty.display_name(binder.name)
        });
        let binder_names = binder_names.collect::<Vec<_>>();
        let rigid_names =
            instance.rigid_parameters.iter().copied().zip(binder_names.iter().cloned());
        let rigid_names = rigid_names.collect::<Vec<_>>();

        let mut lines = vec![];
        if !binder_names.is_empty() {
            lines.push(format!("forall {}.", binder_names.join(" ")));
        }

        for evidence in instance.evidences.iter() {
            let field_name = self.evidence_name(evidence_names, &evidence.evidence)?;
            let constraint = type_pretty.render(evidence.constraint);
            lines.push(format!("{{ {field_name} :: {constraint} }} =>"));
        }

        lines.push(type_pretty.render(current).to_string());

        let mut lines = lines.into_iter();
        let first = lines.next().expect("invariant violated: dictionary signature is empty");
        let lines = lines.fold(self.arena.text(first), |document, line| {
            document.append(self.arena.line()).append(self.arena.text(line))
        });
        let signature = self.arena.text(format!("dictionary {name} ::"));
        let signature = breakable_continuation(self.arena, signature, lines);
        Ok((signature, rigid_names))
    }

    fn instance_superclass_field_names(
        &self,
        instance: &InstanceDeclaration,
    ) -> QueryResult<Vec<SmolStr>> {
        let indexed = if instance.class.0 == self.file_id {
            None
        } else {
            Some(self.queries.indexed(instance.class.0)?)
        };
        let indexed = indexed.as_deref().unwrap_or(self.indexed);

        let mut field_names = PrettyNames::new();
        for member_id in indexed.class_members(instance.class.1) {
            if let Some(name) = &indexed.items[member_id].name {
                field_names.allocate_display_name(SmolStr::clone(name));
            }
        }

        let mut superclasses = vec![];
        for superclass in instance.superclasses.iter() {
            let base = self.evidence_base_name(superclass.constraint)?;
            superclasses.push(field_names.allocate_display_name(base));
        }
        Ok(superclasses)
    }

    fn dictionary_base_name(
        &self,
        type_id: crate::TypeId,
        instance: &InstanceDeclaration,
    ) -> QueryResult<SmolStr> {
        let class_name = self.type_name(instance.class.0, instance.class.1)?;
        let Some(class_name) = class_name else {
            return Ok(SmolStr::new("dictionary"));
        };

        let mut characters = class_name.chars();
        let Some(first) = characters.next() else {
            return Ok(SmolStr::new("dictionary"));
        };

        let mut base = String::with_capacity(class_name.len());
        base.extend(first.to_lowercase());
        base.push_str(characters.as_str());

        let mut current = type_id;
        let mut arguments = vec![];
        loop {
            match *self.queries.lookup_type(current) {
                Type::Forall(_, inner) | Type::Constrained(_, inner) | Type::Kinded(inner, _) => {
                    current = inner;
                }
                Type::Application(function, argument) => {
                    arguments.push(argument);
                    current = function;
                }
                Type::KindApplication(function, argument) => {
                    arguments.push(argument);
                    current = function;
                }
                _ => break,
            }
        }

        for argument in arguments.into_iter().rev() {
            self.append_type_constructor_names(&mut base, argument)?;
        }

        Ok(SmolStr::new(base))
    }

    fn append_type_constructor_names(
        &self,
        name: &mut String,
        type_id: crate::TypeId,
    ) -> QueryResult<()> {
        match *self.queries.lookup_type(type_id) {
            Type::Application(function, argument) | Type::KindApplication(function, argument) => {
                self.append_type_constructor_names(name, function)?;
                self.append_type_constructor_names(name, argument)?;
            }
            Type::Forall(_, inner) | Type::Kinded(inner, _) => {
                self.append_type_constructor_names(name, inner)?;
            }
            Type::Constrained(_, inner) => {
                self.append_type_constructor_names(name, inner)?;
            }
            Type::Function(parameter, result) => {
                name.push_str("Function");
                self.append_type_constructor_names(name, parameter)?;
                self.append_type_constructor_names(name, result)?;
            }
            Type::Constructor(file_id, item_id) => {
                if let Some(constructor_name) = self.type_name(file_id, item_id)? {
                    name.push_str(&constructor_name);
                }
            }
            Type::Row(_) => name.push_str("Row"),
            Type::Integer(_)
            | Type::String(..)
            | Type::Rigid(..)
            | Type::Unification(_)
            | Type::Free(_)
            | Type::Unknown(_) => {}
        }

        Ok(())
    }

    fn equation_declarations(
        &self,
        name: &str,
        prefix: &str,
        declaration_abstractions: &[DeclarationAbstraction],
        equations: &[Equation],
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Option<Doc<'arena>>> {
        let mut rendered_equations = vec![];
        for equation in equations.iter() {
            let has_abstraction = !equation.binders.is_empty()
                || declaration_abstractions.iter().any(|abstraction| {
                    matches!(abstraction, DeclarationAbstraction::Evidence { .. })
                });
            let (mut expression, where_bindings, force_body_break, is_lambda) = if let [alternative] =
                equation.guarded_expression.alternatives.as_ref()
                && alternative.pattern_guards.is_empty()
            {
                let where_expression = &alternative.where_expression;
                let expression =
                    self.expression(where_expression.expression, evidence_names, type_pretty)?;
                let bindings = (!where_expression.bindings.chunks.is_empty())
                    .then_some(&where_expression.bindings);
                let force_body_break =
                    self.expression_requires_body_break(where_expression.expression);
                let is_lambda = matches!(
                    self.checked.tree[where_expression.expression].kind,
                    ExpressionKind::Lambda { .. }
                );
                (expression, bindings, force_body_break, is_lambda)
            } else {
                let expression = self.guarded_expression(
                    &equation.guarded_expression,
                    evidence_names,
                    type_pretty,
                )?;
                (expression, None, false, false)
            };

            let mut abstractions = vec![];
            for abstraction in declaration_abstractions {
                // Type abstractions are omitted because the declaration's
                // rendered signature already communicates its binders.
                let DeclarationAbstraction::Evidence { binder, .. } = abstraction else {
                    continue;
                };
                let binder = self.evidence_binder_name(evidence_names, *binder)?;
                abstractions.push(self.arena.text(format!("\\{{{binder}}} ->")));
            }
            for &binder in equation.binders.iter() {
                let binder = self.binder(binder, type_pretty)?;
                let abstraction =
                    self.arena.text("\\").append(binder).append(self.arena.text(" ->"));
                abstractions.push(abstraction);
            }

            let mut abstractions = abstractions.into_iter();
            if let Some(first) = abstractions.next() {
                let abstractions = abstractions.fold(first, |document, abstraction| {
                    document.append(self.arena.softline().append(abstraction).nest(2))
                });
                let body = if force_body_break {
                    self.arena.hardline().append(expression).nest(2)
                } else {
                    self.arena.line().append(expression).nest(2).group()
                };
                expression = abstractions.append(body);
            }

            let mut equation = if has_abstraction {
                self.arena.text(format!("{name} = ")).append(expression)
            } else if force_body_break {
                self.arena
                    .text(format!("{name} ="))
                    .append(self.arena.hardline().append(expression).nest(2))
            } else if is_lambda {
                self.arena.text(format!("{name} = ")).append(expression).group()
            } else {
                self.arena
                    .text(format!("{name} ="))
                    .append(self.arena.line().append(expression).nest(2))
                    .group()
            };
            if let Some(bindings) = where_bindings {
                let where_clause = self.where_clause(bindings, evidence_names, type_pretty)?;
                equation = equation.append(self.arena.hardline().append(where_clause).nest(2));
            }
            if !prefix.is_empty() {
                let prefix = prefix.to_string();
                let indentation = prefix.len() as isize;
                equation = self.arena.text(prefix).append(equation.nest(indentation));
            }
            rendered_equations.push(equation);
        }

        let mut equations = rendered_equations.into_iter();
        let Some(first) = equations.next() else { return Ok(None) };
        let equations = equations.fold(first, |document, equation| {
            document.append(self.arena.hardline()).append(equation)
        });
        Ok(Some(equations))
    }

    fn local_declaration(
        &self,
        declaration_id: LocalDeclarationId,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let declaration = &self.checked.tree[declaration_id];
        let name = self.local_declaration_name(declaration.source);
        let type_pretty = &mut type_pretty.fork();
        let type_id = type_pretty.render(declaration.type_id);
        let signature = self.arena.text(format!("{name} :: {type_id}"));
        let rigid_names =
            self.declaration_rigid_names(type_pretty, &declaration.value.abstractions);
        self.assign_rigid_names(type_pretty, &rigid_names);

        for abstraction in declaration.value.abstractions.iter() {
            if let DeclarationAbstraction::Evidence { binder, .. } = abstraction {
                self.evidence_binder_name(evidence_names, *binder)?;
            }
        }

        let equations = self.equation_declarations(
            &name,
            "",
            &declaration.value.abstractions,
            &declaration.value.equations,
            evidence_names,
            type_pretty,
        )?;
        let equations = equations.unwrap_or_else(|| self.arena.text(format!("{name} = <error>")));
        Ok(signature.append(self.arena.hardline()).append(equations))
    }

    fn let_bindings(
        &self,
        bindings: &LetBindings,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let mut rendered = vec![];
        for chunk in bindings.chunks.iter() {
            match chunk {
                LetBindingChunk::Pattern { binder, where_expression, .. } => {
                    let binder = self.binder(*binder, type_pretty)?;
                    let expression =
                        self.where_expression(where_expression, evidence_names, type_pretty)?;
                    rendered.push(
                        binder
                            .append(self.arena.text(" ="))
                            .append(self.arena.line().append(expression).nest(2))
                            .group(),
                    );
                }
                LetBindingChunk::PatternError { where_expression, .. } => {
                    if let Some(where_expression) = where_expression {
                        let expression =
                            self.where_expression(where_expression, evidence_names, type_pretty)?;
                        rendered.push(
                            self.arena
                                .text("<error> =")
                                .append(self.arena.line().append(expression).nest(2))
                                .group(),
                        );
                    } else {
                        rendered.push(self.arena.text("<error-binding>"));
                    }
                }
                LetBindingChunk::Names { declarations, .. } => {
                    for &declaration in declarations.iter() {
                        rendered.push(self.local_declaration(
                            declaration,
                            evidence_names,
                            type_pretty,
                        )?);
                    }
                }
            }
        }

        let mut rendered = rendered.into_iter();
        let Some(first) = rendered.next() else { return Ok(self.arena.text("<error-binding>")) };
        let bindings = rendered.fold(first, |document, binding| {
            document.append(self.arena.hardline()).append(binding)
        });
        Ok(bindings)
    }

    fn where_clause(
        &self,
        bindings: &LetBindings,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let bindings = self.let_bindings(bindings, evidence_names, type_pretty)?;
        Ok(self.arena.text("where").append(self.arena.hardline()).append(bindings))
    }

    fn where_expression(
        &self,
        where_expression: &WhereExpression,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let expression =
            self.expression(where_expression.expression, evidence_names, type_pretty)?;
        if where_expression.bindings.chunks.is_empty() {
            return Ok(expression);
        }

        let where_clause =
            self.where_clause(&where_expression.bindings, evidence_names, type_pretty)?;
        Ok(expression.append(self.arena.hardline().append(where_clause).nest(2)))
    }

    fn guarded_expression(
        &self,
        guarded: &GuardedExpression,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        if let [alternative] = guarded.alternatives.as_ref()
            && alternative.pattern_guards.is_empty()
        {
            return self.where_expression(
                &alternative.where_expression,
                evidence_names,
                type_pretty,
            );
        }

        let mut alternatives = vec![];
        for alternative in guarded.alternatives.iter() {
            alternatives.push(self.guarded_alternative(
                alternative,
                evidence_names,
                type_pretty,
            )?);
        }

        let mut alternatives = alternatives.into_iter();
        let Some(first) = alternatives.next() else {
            return Ok(self.arena.text("<error>"));
        };
        let alternatives = alternatives.fold(first, |document, alternative| {
            document.append(self.arena.hardline()).append(alternative)
        });
        Ok(alternatives)
    }

    fn guarded_alternative(
        &self,
        alternative: &GuardedAlternative,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let mut pattern_guards = vec![];
        for pattern_guard in alternative.pattern_guards.iter() {
            pattern_guards.push(self.pattern_guard(pattern_guard, evidence_names, type_pretty)?);
        }

        let where_expression = &alternative.where_expression;
        let expression = self.where_expression(where_expression, evidence_names, type_pretty)?;
        let mut pattern_guards = pattern_guards.into_iter();
        let Some(first) = pattern_guards.next() else {
            let alternative = self.arena.text("| ->");
            if self.expression_requires_body_break(where_expression.expression) {
                return Ok(alternative.append(self.arena.hardline().append(expression).nest(2)));
            }
            return Ok(alternative.append(self.arena.space()).append(expression));
        };
        let pattern_guards = pattern_guards.fold(first, |document, pattern_guard| {
            document.append(self.arena.text(", ")).append(pattern_guard)
        });
        let alternative =
            self.arena.text("| ").append(pattern_guards).append(self.arena.text(" ->"));
        let alternative = if self.expression_requires_body_break(where_expression.expression) {
            alternative.append(self.arena.hardline().append(expression).nest(2))
        } else {
            alternative.append(self.arena.space()).append(expression)
        };
        Ok(alternative)
    }

    fn pattern_guard(
        &self,
        pattern_guard: &PatternGuard,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        match *pattern_guard {
            PatternGuard::Boolean { expression } => {
                self.expression(expression, evidence_names, type_pretty)
            }
            PatternGuard::Pattern { binder, expression } => {
                let binder = self.binder(binder, type_pretty)?;
                let expression = self.expression(expression, evidence_names, type_pretty)?;
                Ok(binder.append(self.arena.text(" <- ")).append(expression))
            }
        }
    }

    fn case_expression(
        &self,
        scrutinees: &[ExpressionId],
        alternatives: &[CaseAlternative],
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let mut rendered_scrutinees = vec![];
        for &scrutinee in scrutinees {
            rendered_scrutinees.push(self.expression(scrutinee, evidence_names, type_pretty)?);
        }

        let mut rendered_scrutinees = rendered_scrutinees.into_iter();
        let scrutinees = if let Some(first) = rendered_scrutinees.next() {
            rendered_scrutinees.fold(first, |document, scrutinee| {
                document.append(self.arena.text(", ")).append(scrutinee)
            })
        } else {
            self.arena.text("<error>")
        };
        let header = self.arena.text("case ").append(scrutinees.group()).append(" of");

        let mut rendered_alternatives = vec![];
        for alternative in alternatives {
            rendered_alternatives.push(self.case_alternative(
                alternative,
                evidence_names,
                type_pretty,
            )?);
        }

        let mut rendered_alternatives = rendered_alternatives.into_iter();
        let alternatives = if let Some(first) = rendered_alternatives.next() {
            rendered_alternatives.fold(first, |document, alternative| {
                document.append(self.arena.hardline()).append(alternative)
            })
        } else {
            self.arena.text("<error>")
        };
        Ok(header.append(self.arena.hardline().append(alternatives).nest(2)))
    }

    fn case_alternative(
        &self,
        alternative: &CaseAlternative,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let mut rendered_binders = vec![];
        for &binder in alternative.binders.iter() {
            rendered_binders.push(self.binder(binder, type_pretty)?);
        }

        let mut rendered_binders = rendered_binders.into_iter();
        let binders = if let Some(first) = rendered_binders.next() {
            rendered_binders.fold(first, |document, binder| {
                document.append(self.arena.text(", ")).append(binder)
            })
        } else {
            self.arena.text("<error>")
        };
        let binders = binders.group();

        if let [guarded] = alternative.guarded_expression.alternatives.as_ref()
            && guarded.pattern_guards.is_empty()
        {
            let expression =
                self.where_expression(&guarded.where_expression, evidence_names, type_pretty)?;
            let alternative = binders.append(self.arena.text(" ->"));
            if self.expression_requires_body_break(guarded.where_expression.expression) {
                return Ok(alternative.append(self.arena.hardline().append(expression).nest(2)));
            }
            return Ok(alternative.append(self.arena.space()).append(expression));
        }

        let guarded_expression =
            self.guarded_expression(&alternative.guarded_expression, evidence_names, type_pretty)?;
        Ok(binders.append(self.arena.hardline().append(guarded_expression).nest(2)))
    }

    fn expression_requires_body_break(&self, expression_id: ExpressionId) -> bool {
        matches!(
            &self.checked.tree[expression_id].kind,
            ExpressionKind::IfThenElse { .. } | ExpressionKind::Case { .. }
        )
    }

    fn expression_is_block_argument(&self, expression_id: ExpressionId) -> bool {
        matches!(
            &self.checked.tree[expression_id].kind,
            ExpressionKind::EvidenceAbstraction { .. }
                | ExpressionKind::Lambda { .. }
                | ExpressionKind::IfThenElse { .. }
                | ExpressionKind::Case { .. }
                | ExpressionKind::Let { .. }
        )
    }

    fn binder(
        &self,
        binder_id: BinderId,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let binder = &self.checked.tree[binder_id];
        match &binder.kind {
            BinderKind::Error => Ok(self.arena.text("<error>")),
            BinderKind::Typed { binder, annotation } => {
                let binder = self.binder(*binder, type_pretty)?;
                let annotation = type_pretty.render(*annotation);
                Ok(self
                    .arena
                    .text("(")
                    .append(binder)
                    .append(self.arena.text(format!(" :: {annotation})"))))
            }
            BinderKind::Integer { value } => {
                let value =
                    if value.is_negative() { format!("({value})") } else { value.to_string() };
                Ok(self.arena.text(value))
            }
            BinderKind::Number { negative, value } => {
                let value = if *negative { format!("(-{value})") } else { value.to_string() };
                Ok(self.arena.text(value))
            }
            BinderKind::Variable => match binder.source {
                BinderSource::Binder(source) => {
                    let kind = self
                        .lowered
                        .tree
                        .get_binder_kind(source)
                        .expect("invariant violated: semantic variable binder has no source");
                    let lowering::BinderKind::Variable { variable: Some(variable) } = kind else {
                        unreachable!(
                            "invariant violated: semantic variable binder has invalid source"
                        );
                    };
                    Ok(self.arena.text(variable.to_string()))
                }
                BinderSource::Section(source) => Ok(self.arena.text(section_name(source))),
                BinderSource::Generated { ref name, .. } => {
                    Ok(self.arena.text(self.queries.lookup_smol_str(*name).to_string()))
                }
                BinderSource::DoStatement(_) | BinderSource::Operator(_) => {
                    unreachable!("invariant violated: generated semantic variable binder")
                }
            },
            BinderKind::Named { name, binder } => {
                let binder = self.binder(*binder, type_pretty)?;
                Ok(self.arena.text(format!("{name}@")).append(binder))
            }
            BinderKind::Wildcard => Ok(self.arena.text("_")),
            BinderKind::String { value } => {
                let text = lowering::literal::encode_normal_string(value);
                Ok(self.arena.text(text))
            }
            BinderKind::Char { value } => {
                let text = character_literal(*value);
                Ok(self.arena.text(text))
            }
            BinderKind::Boolean { value } => {
                let text = if *value { "true" } else { "false" };
                Ok(self.arena.text(text))
            }
            BinderKind::Array { elements } => {
                let mut array = self.arena.text("[");
                for (position, &element) in elements.iter().enumerate() {
                    if position > 0 {
                        array = array.append(self.arena.text(", "));
                    }
                    array = array.append(self.binder(element, type_pretty)?);
                }
                Ok(array.append(self.arena.text("]")))
            }
            BinderKind::Record { fields } => {
                let mut record = self.arena.text("{ ");
                for (position, field) in fields.iter().enumerate() {
                    if position > 0 {
                        record = record.append(self.arena.text(", "));
                    }
                    match field {
                        RecordBinderField::Field { label, binder } => {
                            let binder = self.binder(*binder, type_pretty)?;
                            record =
                                record.append(self.arena.text(format!("{label}: "))).append(binder);
                        }
                        RecordBinderField::Pun { label } => {
                            record = record.append(self.arena.text(label.to_string()));
                        }
                    }
                }
                Ok(record.append(self.arena.text(" }")))
            }
            BinderKind::Constructor { resolution, arguments } => {
                let name = self.term_name(resolution.0, resolution.1)?;
                let name = name.unwrap_or_else(|| "?".to_string());
                let mut constructor = self.arena.text(name);
                if arguments.is_empty() {
                    return Ok(constructor);
                }
                for &argument in arguments.iter() {
                    let argument = self.binder(argument, type_pretty)?;
                    constructor = constructor.append(self.arena.space()).append(argument);
                }
                Ok(self.arena.text("(").append(constructor).append(self.arena.text(")")))
            }
        }
    }

    fn expression(
        &self,
        expression_id: ExpressionId,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        self.expression_at(
            expression_id,
            ExpressionPrecedence::Abstraction,
            evidence_names,
            type_pretty,
        )
    }

    fn expression_at(
        &self,
        expression_id: ExpressionId,
        required_precedence: ExpressionPrecedence,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let expression = &self.checked.tree[expression_id];
        let precedence = match &expression.kind {
            ExpressionKind::EvidenceAbstraction { .. }
            | ExpressionKind::Lambda { .. }
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Let { .. } => ExpressionPrecedence::Abstraction,
            ExpressionKind::TermApplication { .. } | ExpressionKind::EvidenceApplication { .. } => {
                ExpressionPrecedence::Application
            }
            ExpressionKind::RecordUpdate { .. } => ExpressionPrecedence::RecordUpdate,
            _ => ExpressionPrecedence::Atom,
        };
        let allow_block_argument = required_precedence != ExpressionPrecedence::Application;
        let expression = self.expression_unparenthesized(
            expression_id,
            allow_block_argument,
            evidence_names,
            type_pretty,
        )?;
        if precedence < required_precedence {
            Ok(self.arena.text("(").append(expression).append(self.arena.text(")")))
        } else {
            Ok(expression)
        }
    }

    fn expression_unparenthesized(
        &self,
        expression_id: ExpressionId,
        allow_block_argument: bool,
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        let expression = &self.checked.tree[expression_id];
        match &expression.kind {
            ExpressionKind::Error => Ok(self.arena.text("<error>")),
            ExpressionKind::String { kind, value } => match kind {
                lowering::StringKind::String => {
                    let text = lowering::literal::encode_normal_string(value);
                    Ok(self.arena.text(text))
                }
                lowering::StringKind::RawString => {
                    let value = value.to_utf8().unwrap_or_else(|_| {
                        unreachable!("invariant violated: raw string contains a lone surrogate")
                    });
                    let text = format!("\"\"\"{value}\"\"\"");
                    Ok(self.arena.text(text))
                }
            },
            ExpressionKind::Char { value } => {
                let text = character_literal(*value);
                Ok(self.arena.text(text))
            }
            ExpressionKind::Boolean { value } => {
                let text = if *value { "true" } else { "false" };
                Ok(self.arena.text(text))
            }
            ExpressionKind::Integer { value } => {
                let text = value.to_string();
                Ok(self.arena.text(text))
            }
            ExpressionKind::Number { value } => {
                let text = value.to_string();
                Ok(self.arena.text(text))
            }
            ExpressionKind::Array { elements } => {
                let mut array = self.arena.text("[");
                for (position, element) in elements.iter().enumerate() {
                    if position > 0 {
                        array = array.append(self.arena.text(", "));
                    }
                    let element = self.expression(*element, evidence_names, type_pretty)?;
                    array = array.append(element);
                }
                Ok(array.append(self.arena.text("]")))
            }
            ExpressionKind::Record { fields } => {
                if fields.is_empty() {
                    return Ok(self.arena.text("{ }"));
                }

                let mut record = self.arena.text("{ ");
                for (position, field) in fields.iter().enumerate() {
                    if position > 0 {
                        record = record.append(self.arena.text(", "));
                    }

                    let (label, expression, shorthand) = match field {
                        RecordExpressionField::Field { label, expression } => {
                            (label, expression, false)
                        }
                        RecordExpressionField::Pun { label, expression, .. } => {
                            let expression_kind = &self.checked.tree[*expression].kind;
                            let shorthand =
                                matches!(expression_kind, ExpressionKind::RecordPun { .. });
                            (label, expression, shorthand)
                        }
                    };

                    if shorthand {
                        record = record.append(self.arena.text(label.to_string()));
                    } else {
                        let expression =
                            self.expression(*expression, evidence_names, type_pretty)?;
                        let text = format!("{label}: ");
                        record = record.append(self.arena.text(text)).append(expression);
                    }
                }
                Ok(record.append(self.arena.text(" }")))
            }
            ExpressionKind::RecordAccess { record, labels } => {
                let mut record = self.expression_at(
                    *record,
                    ExpressionPrecedence::Atom,
                    evidence_names,
                    type_pretty,
                )?;
                for label in labels.iter() {
                    let label = format!(".{label}");
                    record = record.append(self.arena.text(label));
                }
                Ok(record)
            }
            ExpressionKind::RecordUpdate { record, updates } => {
                let record = self.expression_at(
                    *record,
                    ExpressionPrecedence::Atom,
                    evidence_names,
                    type_pretty,
                )?;
                let updates =
                    self.record_expression_updates(updates, evidence_names, type_pretty)?;
                Ok(record.append(self.arena.space()).append(updates))
            }
            ExpressionKind::Constructor { resolution } => {
                let name = self.term_name(resolution.0, resolution.1)?;
                let text = name.unwrap_or_else(|| "?".to_string());
                Ok(self.arena.text(text))
            }
            ExpressionKind::Variable { resolution }
            | ExpressionKind::RecordPun { resolution, .. } => {
                let name = match *resolution {
                    VariableResolution::Generated(binder) => {
                        let generated = &self.checked.tree[binder];
                        match &generated.source {
                            BinderSource::Generated { name, .. } => {
                                self.queries.lookup_smol_str(*name).to_string()
                            }
                            _ => {
                                let index = binder.into_raw().into_u32();
                                format!("<generated#{index}>")
                            }
                        }
                    }
                    VariableResolution::Source(resolution) => match resolution {
                        TermVariableResolution::Binder(binder) => {
                            let kind = self.lowered.tree.get_binder_kind(binder).expect(
                                "invariant violated: variable expression binder is missing",
                            );
                            match kind {
                                lowering::BinderKind::Variable { variable: Some(variable) } => {
                                    variable.to_string()
                                }
                                lowering::BinderKind::Named { named: Some(named), .. } => {
                                    named.to_string()
                                }
                                _ => {
                                    let index = binder.into_raw().get();
                                    format!("<binder#{index}>")
                                }
                            }
                        }
                        TermVariableResolution::Reference(file_id, term_id) => {
                            self.term_name(file_id, term_id)?.unwrap_or_else(|| "?".to_string())
                        }
                        TermVariableResolution::Let(let_binding) => {
                            let declaration = self.checked.tree.lookup_let(let_binding).expect(
                                "invariant violated: local variable declaration is missing",
                            );
                            let declaration = &self.checked.tree[declaration];
                            self.local_declaration_name(declaration.source)
                        }
                        TermVariableResolution::RecordPun(record_pun) => {
                            self.record_pun_name(record_pun).unwrap_or_else(|| {
                                let index = record_pun.into_raw().get();
                                format!("<pun#{index}>")
                            })
                        }
                    },
                };
                Ok(self.arena.text(name))
            }
            ExpressionKind::Section { binder } => {
                let binder = &self.checked.tree[*binder];
                let BinderSource::Section(source) = binder.source else {
                    unreachable!("invariant violated: semantic section has invalid binder")
                };
                Ok(self.arena.text(section_name(source)))
            }
            ExpressionKind::TermApplication { function, argument } => {
                let function = self.expression_at(
                    *function,
                    ExpressionPrecedence::Application,
                    evidence_names,
                    type_pretty,
                )?;
                let is_block_argument =
                    allow_block_argument && self.expression_is_block_argument(*argument);
                let argument_precedence = if is_block_argument {
                    ExpressionPrecedence::Abstraction
                } else {
                    ExpressionPrecedence::RecordUpdate
                };
                let argument = self.expression_at(
                    *argument,
                    argument_precedence,
                    evidence_names,
                    type_pretty,
                )?;
                if is_block_argument {
                    Ok(function.append(self.arena.space()).append(argument))
                } else {
                    Ok(breakable_continuation(self.arena, function, argument))
                }
            }
            ExpressionKind::EvidenceApplication { function, evidence, .. } => {
                let function = self.expression_at(
                    *function,
                    ExpressionPrecedence::Application,
                    evidence_names,
                    type_pretty,
                )?;
                let evidence = self.evidence_variable_name(evidence_names, *evidence)?;
                Ok(function.append(self.arena.text(format!(" {{{evidence}}}"))))
            }
            ExpressionKind::EvidenceAbstraction { binder, expression } => {
                let binder = self.evidence_binder_name(evidence_names, *binder)?;
                let abstraction = self.arena.text(format!("\\{{{binder}}} ->"));
                let body = self.expression(*expression, evidence_names, type_pretty)?;
                if self.expression_requires_body_break(*expression) {
                    Ok(abstraction.append(self.arena.hardline().append(body).nest(2)))
                } else {
                    Ok(breakable_continuation(self.arena, abstraction, body))
                }
            }
            ExpressionKind::Lambda { binders, expression } => {
                let mut lambda = self.arena.text("\\");
                if binders.is_empty() {
                    lambda = lambda.append(self.arena.text("<error>"));
                } else {
                    for (position, &binder) in binders.iter().enumerate() {
                        if position > 0 {
                            lambda = lambda.append(self.arena.space());
                        }
                        lambda = lambda.append(self.binder(binder, type_pretty)?);
                    }
                }

                let body = self.expression(*expression, evidence_names, type_pretty)?;
                lambda = lambda.append(self.arena.text(" ->"));
                if self.expression_requires_body_break(*expression) {
                    Ok(lambda.append(self.arena.hardline().append(body).nest(2)))
                } else {
                    Ok(breakable_continuation(self.arena, lambda, body))
                }
            }
            ExpressionKind::IfThenElse { condition, then, else_ } => {
                let condition = self.expression(*condition, evidence_names, type_pretty)?;
                let then = self.expression(*then, evidence_names, type_pretty)?;
                let else_ = self.expression(*else_, evidence_names, type_pretty)?;
                Ok(self
                    .arena
                    .text("if ")
                    .append(condition)
                    .append(self.arena.text(" then"))
                    .append(self.arena.line().append(then).nest(2))
                    .append(self.arena.line())
                    .append(self.arena.text("else"))
                    .append(self.arena.line().append(else_).nest(2))
                    .group())
            }
            ExpressionKind::Case { scrutinees, alternatives } => {
                self.case_expression(scrutinees, alternatives, evidence_names, type_pretty)
            }
            ExpressionKind::Let { bindings, expression } => {
                let bindings = self.let_bindings(bindings, evidence_names, type_pretty)?;
                let expression = self.expression(*expression, evidence_names, type_pretty)?;
                Ok(self
                    .arena
                    .text("let")
                    .append(self.arena.hardline().append(bindings).nest(2))
                    .append(self.arena.hardline())
                    .append(self.arena.text("in"))
                    .append(self.arena.hardline().append(expression).nest(2)))
            }
        }
    }

    fn record_expression_updates(
        &self,
        updates: &[RecordExpressionUpdate],
        evidence_names: &mut EvidenceNames,
        type_pretty: &mut TypePrettyState<'context, Q>,
    ) -> QueryResult<Doc<'arena>> {
        if updates.is_empty() {
            return Ok(self.arena.text("{ }"));
        }

        let mut rendered = self.arena.text("{ ");
        for (position, update) in updates.iter().enumerate() {
            if position > 0 {
                rendered = rendered.append(self.arena.text(", "));
            }

            match update {
                RecordExpressionUpdate::Error => {
                    rendered = rendered.append(self.arena.text("<error>"));
                }
                RecordExpressionUpdate::Leaf { label, expression } => {
                    let expression = self.expression(*expression, evidence_names, type_pretty)?;
                    let label = format!("{label} = ");
                    rendered = rendered.append(self.arena.text(label)).append(expression);
                }
                RecordExpressionUpdate::Branch { label, updates } => {
                    let updates =
                        self.record_expression_updates(updates, evidence_names, type_pretty)?;
                    let label = format!("{label} ");
                    rendered = rendered.append(self.arena.text(label)).append(updates);
                }
            }
        }
        Ok(rendered.append(self.arena.text(" }")))
    }

    fn evidence_variable_name(
        &self,
        names: &mut EvidenceNames,
        evidence: EvidenceVarId,
    ) -> QueryResult<SmolStr> {
        self.evidence_name(names, &Evidence::Variable(evidence))
    }

    fn evidence_name(
        &self,
        names: &mut EvidenceNames,
        evidence: &Evidence,
    ) -> QueryResult<SmolStr> {
        enum Pending<'a> {
            Evidence(&'a Evidence),
            Variable(EvidenceVarId),
            Text(&'static str),
            Precedence(EvidencePrecedence),
            Superclass { prefix: SmolStrBuilder, superclass: SuperclassId },
        }

        let mut rendered = SmolStrBuilder::new();
        let mut precedence = EvidencePrecedence::Atom;
        let mut pending = vec![Pending::Evidence(evidence)];
        while let Some(next) = pending.pop() {
            match next {
                Pending::Evidence(Evidence::Variable(evidence)) => {
                    pending.push(Pending::Variable(*evidence));
                }
                Pending::Variable(evidence) => match self.checked.evidence[evidence].state {
                    EvidenceState::Solved(proof) => {
                        pending.push(Pending::Evidence(&self.checked.evidence[proof]));
                    }
                    EvidenceState::Unsolved => {
                        rendered.push_str("unsolved");
                        precedence = EvidencePrecedence::Atom;
                    }
                    EvidenceState::Error => {
                        rendered.push_str("error");
                        precedence = EvidencePrecedence::Atom;
                    }
                },
                Pending::Evidence(Evidence::Given(binder)) => {
                    rendered.push_str(&self.evidence_binder_name(names, *binder)?);
                    precedence = EvidencePrecedence::Atom;
                }
                Pending::Evidence(Evidence::Instance { origin, subgoals }) => {
                    rendered.push_str(&self.instance_dictionary_name(*origin)?);
                    let instance_precedence = if subgoals.is_empty() {
                        EvidencePrecedence::Atom
                    } else {
                        EvidencePrecedence::Application
                    };
                    pending.push(Pending::Precedence(instance_precedence));
                    for subgoal in subgoals.iter().rev() {
                        pending.push(Pending::Text("}"));
                        pending.push(Pending::Variable(*subgoal));
                        pending.push(Pending::Text(" {"));
                    }
                }
                Pending::Evidence(Evidence::Superclass { parent, superclass }) => {
                    pending.push(Pending::Superclass {
                        prefix: std::mem::take(&mut rendered),
                        superclass: *superclass,
                    });
                    pending.push(Pending::Evidence(&self.checked.evidence[*parent]));
                }
                Pending::Evidence(Evidence::Trivial) => {
                    rendered.push_str("trivial");
                    precedence = EvidencePrecedence::Atom;
                }
                Pending::Evidence(Evidence::Synthesized(evidence)) => {
                    rendered.push_str(&synthesized_evidence_name(evidence));
                    precedence = EvidencePrecedence::Atom;
                }
                Pending::Text(text) => rendered.push_str(text),
                Pending::Precedence(completed) => precedence = completed,
                Pending::Superclass { prefix, superclass } => {
                    let parent = rendered.finish();
                    rendered = prefix;
                    let field = self.superclass_field_name(superclass)?;
                    precedence.append_projection(&mut rendered, &parent, &field);
                    precedence = EvidencePrecedence::Atom;
                }
            }
        }
        Ok(rendered.finish())
    }

    fn instance_dictionary_name(&self, origin: InstanceCandidateOrigin) -> QueryResult<SmolStr> {
        self.instance_dictionary_name_with_cache(origin, &mut self.instance_names.borrow_mut())
    }

    fn instance_dictionary_name_with_cache(
        &self,
        origin: InstanceCandidateOrigin,
        instance_names: &mut InstanceNames,
    ) -> QueryResult<SmolStr> {
        if let Some(name) = instance_names.names.get(&origin) {
            return Ok(SmolStr::clone(name));
        }
        let file_id = match origin {
            InstanceCandidateOrigin::Instance(file_id, _)
            | InstanceCandidateOrigin::Derive(file_id, _) => file_id,
        };
        let indexed =
            if file_id == self.file_id { None } else { Some(self.queries.indexed(file_id)?) };
        let indexed = indexed.as_deref().unwrap_or(self.indexed);

        let item_id = match origin {
            InstanceCandidateOrigin::Instance(_, id) => {
                indexed.pairs.instance_to_item(id).map(InstanceSourceItemId::Instance)
            }
            InstanceCandidateOrigin::Derive(_, id) => {
                indexed.pairs.derive_to_item(id).map(InstanceSourceItemId::Derive)
            }
        };
        let Some(item_id) = item_id else {
            return Ok(UNKNOWN_INSTANCE_EVIDENCE);
        };

        let item_name = match item_id {
            InstanceSourceItemId::Instance(id) => &indexed.items[id].name,
            InstanceSourceItemId::Derive(id) => &indexed.items[id].name,
        };
        if let Some(name) = item_name {
            instance_names.names.insert(origin, SmolStr::clone(name));
            return Ok(SmolStr::clone(name));
        }

        let checked =
            if file_id == self.file_id { None } else { Some(self.queries.checked(file_id)?) };
        let checked = checked.as_deref().unwrap_or(self.checked);

        let module_names = instance_names.modules.entry(file_id).or_insert_with(|| {
            let mut names = PrettyNames::new();
            for (_, item) in indexed.items.iter_terms() {
                if let Some(name) = &item.name {
                    names.allocate_display_name(SmolStr::clone(name));
                }
            }

            for &candidate_id in indexed.items.instance_sources() {
                let name = match candidate_id {
                    InstanceSourceItemId::Instance(id) => &indexed.items[id].name,
                    InstanceSourceItemId::Derive(id) => &indexed.items[id].name,
                };
                if let Some(name) = name {
                    names.allocate_display_name(SmolStr::clone(name));
                }
            }
            ModuleInstanceNames { names, next_source: 0 }
        });

        let remaining_sources = indexed.items.instance_sources()[module_names.next_source..].iter();
        for &candidate_id in remaining_sources {
            let (candidate_name, declaration_id, candidate_origin) = match candidate_id {
                InstanceSourceItemId::Instance(id) => (
                    &indexed.items[id].name,
                    checked.tree.lookup_instance(id),
                    InstanceCandidateOrigin::Instance(file_id, indexed.items[id].id),
                ),
                InstanceSourceItemId::Derive(id) => (
                    &indexed.items[id].name,
                    checked.tree.lookup_derive(id),
                    InstanceCandidateOrigin::Derive(file_id, indexed.items[id].id),
                ),
            };
            let declaration = declaration_id.map(|id| &checked.tree[id]);
            let name = if let Some(name) = candidate_name {
                Some(SmolStr::clone(name))
            } else if let Some(declaration) = declaration
                && let TermDeclarationKind::Instance(instance) = &declaration.kind
            {
                let base = self.dictionary_base_name(declaration.type_id, instance)?;
                Some(module_names.names.allocate_display_name(base))
            } else {
                None
            };
            module_names.next_source += 1;
            let Some(name) = name else { continue };
            instance_names.names.insert(candidate_origin, SmolStr::clone(&name));
            if candidate_id == item_id {
                return Ok(name);
            }
        }
        Ok(UNKNOWN_INSTANCE_EVIDENCE)
    }

    fn superclass_field_name(&self, superclass: SuperclassId) -> QueryResult<SmolStr> {
        let indexed = if superclass.file_id == self.file_id {
            None
        } else {
            Some(self.queries.indexed(superclass.file_id)?)
        };
        let indexed = indexed.as_deref().unwrap_or(self.indexed);
        let checked = if superclass.file_id == self.file_id {
            None
        } else {
            Some(self.queries.checked(superclass.file_id)?)
        };
        let checked = checked.as_deref().unwrap_or(self.checked);
        let Some(declaration_id) = checked.tree.lookup_type_declaration(superclass.type_id) else {
            return Ok(UNKNOWN_SUPERCLASS_EVIDENCE);
        };
        let declaration = &checked.tree[declaration_id];
        let TypeDeclarationKind::Class(class) = &declaration.declaration else {
            return Ok(UNKNOWN_SUPERCLASS_EVIDENCE);
        };

        let mut names = PrettyNames::new();
        for member_id in indexed.class_members(superclass.type_id) {
            if let Some(name) = &indexed.items[member_id].name {
                names.allocate_display_name(SmolStr::clone(name));
            }
        }
        for candidate in class.superclasses.iter() {
            let base = self.evidence_base_name(candidate.constraint)?;
            let name = names.allocate_display_name(base);
            if candidate.id == superclass {
                return Ok(name);
            }
        }
        Ok(UNKNOWN_SUPERCLASS_EVIDENCE)
    }

    fn evidence_binder_name(
        &self,
        names: &mut EvidenceNames,
        binder: EvidenceBinderId,
    ) -> QueryResult<SmolStr> {
        if let Some(display) = names.display_by_binder.get(&binder) {
            return Ok(SmolStr::clone(display));
        }

        let constraint = self.checked.evidence[binder].constraint;
        let base = self.evidence_base_name(constraint)?;
        let display = names.names.allocate_display_name(base);
        names.display_by_binder.insert(binder, SmolStr::clone(&display));
        Ok(display)
    }

    fn evidence_base_name(&self, mut constraint: crate::TypeId) -> QueryResult<SmolStr> {
        let class_name = loop {
            match *self.queries.lookup_type(constraint) {
                Type::Application(function, _)
                | Type::KindApplication(function, _)
                | Type::Kinded(function, _) => constraint = function,
                Type::Constructor(file_id, type_id) => {
                    break self.type_name(file_id, type_id)?;
                }
                _ => break None,
            }
        };
        let Some(class_name) = class_name else {
            return Ok(EVIDENCE_DICTIONARY_NAME);
        };
        let mut characters = class_name.chars();
        let Some(first) = characters.next() else {
            return Ok(EVIDENCE_DICTIONARY_NAME);
        };
        let first = first.to_lowercase().collect::<String>();
        Ok(format_smolstr!("{first}{}Dict", characters.as_str()))
    }

    fn term_name(
        &self,
        file_id: FileId,
        term_id: indexing::TermItemId,
    ) -> QueryResult<Option<String>> {
        if file_id == self.file_id {
            return Ok(self.indexed.items[term_id].name.as_ref().map(ToString::to_string));
        }

        let indexed = self.queries.indexed(file_id)?;
        Ok(indexed.items[term_id].name.as_ref().map(ToString::to_string))
    }

    fn record_pun_name(&self, record_pun: lowering::RecordPunId) -> Option<String> {
        self.lowered.tree.iter_binder().find_map(|(_, kind)| {
            let lowering::BinderKind::Record { record } = kind else {
                return None;
            };
            record.iter().find_map(|item| {
                let lowering::BinderRecordItem::RecordPun { id, name } = item else {
                    return None;
                };
                if *id == record_pun { name.as_ref().map(ToString::to_string) } else { None }
            })
        })
    }

    fn local_declaration_name(&self, source: lowering::LetBindingNameGroupId) -> String {
        let group = self.lowered.tree.get_let_binding_group(source);
        group.name.as_ref().map(ToString::to_string).unwrap_or_else(|| {
            let index = source.into_raw().into_u32();
            format!("<let#{index}>")
        })
    }

    fn type_name(
        &self,
        file_id: FileId,
        type_id: indexing::TypeItemId,
    ) -> QueryResult<Option<SmolStr>> {
        if file_id == self.file_id {
            return Ok(self.indexed.items[type_id].name.clone());
        }

        let indexed = self.queries.indexed(file_id)?;
        Ok(indexed.items[type_id].name.clone())
    }
}

fn synthesized_evidence_name(evidence: &SynthesizedEvidence) -> SmolStr {
    match evidence {
        SynthesizedEvidence::IsSymbol(symbol) => format_smolstr!("isSymbol({symbol:?})"),
        SynthesizedEvidence::Reflectable(ReflectableEvidence::Integer(value)) => {
            format_smolstr!("reflectable({value})")
        }
        SynthesizedEvidence::Reflectable(ReflectableEvidence::String(value)) => {
            format_smolstr!("reflectable({value:?})")
        }
        SynthesizedEvidence::Reflectable(ReflectableEvidence::Boolean(value)) => {
            format_smolstr!("reflectable({value})")
        }
        SynthesizedEvidence::Reflectable(ReflectableEvidence::Ordering(
            ReflectableOrdering::Less,
        )) => REFLECTABLE_LESS_EVIDENCE,
        SynthesizedEvidence::Reflectable(ReflectableEvidence::Ordering(
            ReflectableOrdering::Equal,
        )) => REFLECTABLE_EQUAL_EVIDENCE,
        SynthesizedEvidence::Reflectable(ReflectableEvidence::Ordering(
            ReflectableOrdering::Greater,
        )) => REFLECTABLE_GREATER_EVIDENCE,
        SynthesizedEvidence::AbortIdentity(identity) => {
            format_smolstr!("abortIdentity({identity:?})")
        }
    }
}
