//! Implements the pretty printer for core types.

use std::num::NonZeroU32;
use std::sync::Arc;

use building_types::{QueryProxy, QueryResult};
use files::FileId;
use indexing::{IndexedTypeItemKind, TypeItemId};
use lowering::StringKind;
use num_bigint::{BigInt, Sign};
use pretty::{Arena, DocAllocator, DocBuilder};
use resolving::{ResolvedModule, ScopeSpelling, TypeNamespace};
use rustc_hash::FxHashMap;
use smol_str::{SmolStr, SmolStrBuilder, format_smolstr};

use crate::core::{
    ForallBinder, ForallBinderId, IntegerId, Name, RowField, RowType, RowTypeId, SmolStrId,
    StringLiteralId, Type, TypeId,
};
use crate::{CheckedCore, CheckedModule, safe_loop};

pub(crate) type Doc<'a> = DocBuilder<'a, Arena<'a>, ()>;

const FIRST_SUFFIX: NonZeroU32 = NonZeroU32::new(1).unwrap();
const DEFAULT_WIDTH: usize = 100;

pub(crate) fn breakable_continuation<'arena>(
    arena: &'arena Arena<'arena>,
    head: Doc<'arena>,
    continuation: Doc<'arena>,
) -> Doc<'arena> {
    head.append(arena.line().append(continuation).nest(2)).group()
}

pub trait PrettyQueries:
    QueryProxy<
        Parsed = parsing::FullParsedModule,
        Indexed = Arc<indexing::IndexedModule>,
        Lowered = Arc<lowering::LoweredModule>,
        CheckedCore = Arc<CheckedCore>,
    >
{
    fn lookup_type(&self, id: TypeId) -> &Type;

    fn lookup_forall_binder(&self, id: ForallBinderId) -> ForallBinder;

    fn lookup_row_type(&self, id: RowTypeId) -> &RowType;

    fn lookup_integer(&self, id: IntegerId) -> &BigInt;

    fn lookup_smol_str(&self, id: SmolStrId) -> &SmolStr;

    fn lookup_string_literal(&self, id: StringLiteralId) -> &lowering::StringLiteral;
}

#[derive(Clone, Debug)]
pub struct PrettyNames {
    display_by_name: FxHashMap<Name, SmolStr>,
    next_suffix: FxHashMap<SmolStr, NonZeroU32>,
    next_generated_suffix: FxHashMap<SmolStr, u32>,
    default_name: SmolStr,
}

impl Default for PrettyNames {
    fn default() -> PrettyNames {
        PrettyNames {
            display_by_name: FxHashMap::default(),
            next_suffix: FxHashMap::default(),
            next_generated_suffix: FxHashMap::default(),
            default_name: SmolStr::new("t"),
        }
    }
}

impl PrettyNames {
    pub fn new() -> PrettyNames {
        PrettyNames::default()
    }

    pub fn reset(&mut self) {
        self.display_by_name.clear();
        self.next_suffix.clear();
        self.next_generated_suffix.clear();
    }

    fn set_default_name(&mut self, default_name: &str) {
        self.default_name = SmolStr::new(default_name);
    }

    pub fn display_name<Q>(
        &mut self,
        queries: &Q,
        names: &FxHashMap<Name, SmolStrId>,
        name: Name,
    ) -> SmolStr
    where
        Q: PrettyQueries + ?Sized,
    {
        if let Some(display) = self.display_by_name.get(&name) {
            return SmolStr::clone(display);
        }

        let display = if let Some(&id) = names.get(&name) {
            let base = queries.lookup_smol_str(id);
            self.allocate_display_name(SmolStr::clone(base))
        } else {
            let base = SmolStr::clone(&self.default_name);
            self.allocate_generated_display_name(base)
        };
        self.display_by_name.insert(name, SmolStr::clone(&display));
        display
    }

    fn allocate_generated_display_name(&mut self, base: SmolStr) -> SmolStr {
        let mut suffix = self.next_generated_suffix.get(&base).copied().unwrap_or(0);

        safe_loop! {
            let display = format_smolstr!("{base}{suffix}");

            if !self.next_suffix.contains_key(&display) {
                self.next_suffix.insert(SmolStr::clone(&display), FIRST_SUFFIX);
                if let Some(next_suffix) = suffix.checked_add(1) {
                    self.next_generated_suffix.insert(base, next_suffix);
                }
                return display;
            }

            suffix = suffix.checked_add(1).expect("critical failure: exhausted suffixes");
        }
    }

    pub fn allocate_display_name(&mut self, base: SmolStr) -> SmolStr {
        if !self.next_suffix.contains_key(&base) {
            self.next_suffix.insert(SmolStr::clone(&base), FIRST_SUFFIX);
            return base;
        }

        let mut suffix = self
            .next_suffix
            .get(&base)
            .copied()
            .expect("critical failure: display name missing suffix state");

        safe_loop! {
            let display = format_smolstr!("{base}{suffix}");

            if !self.next_suffix.contains_key(&display) {
                self.next_suffix.insert(SmolStr::clone(&display), FIRST_SUFFIX);
                if let Some(next_suffix) = try_next_suffix(suffix) {
                    self.next_suffix.insert(base, next_suffix);
                }
                return display;
            }

            suffix = try_next_suffix(suffix).expect("critical failure: exhausted suffixes");
        }
    }

    pub(crate) fn assign_display_name(&mut self, name: Name, display: SmolStr) {
        if !self.next_suffix.contains_key(&display) {
            self.next_suffix.insert(SmolStr::clone(&display), FIRST_SUFFIX);
        }
        self.display_by_name.insert(name, display);
    }
}

fn try_next_suffix(suffix: NonZeroU32) -> Option<NonZeroU32> {
    suffix.get().checked_add(1).and_then(NonZeroU32::new)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeNameResolution {
    Name(SmolStr),
    FallbackDefiningModule,
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BundleRendering {
    Infix,
    Constructor,
}

pub trait TypeNameResolver {
    fn resolve_type(
        &mut self,
        file_id: FileId,
        type_id: TypeItemId,
        name: &str,
        namespace: TypeNamespace,
    ) -> TypeNameResolution;

    fn bundle_rendering(&self) -> BundleRendering {
        BundleRendering::Infix
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutOfScopePolicy {
    RecordUnresolved,
    FallbackDefiningModule,
}

pub struct ModuleScopeResolver<'a> {
    resolved: &'a ResolvedModule,
    prim: &'a ResolvedModule,
    bundle_operator: Option<(FileId, TypeItemId)>,
    planned_spellings: Option<&'a FxHashMap<(FileId, TypeItemId), SmolStr>>,
    out_of_scope: OutOfScopePolicy,
    unresolved: Vec<(FileId, TypeItemId)>,
}

impl<'a> ModuleScopeResolver<'a> {
    pub fn new(
        resolved: &'a ResolvedModule,
        prim: &'a ResolvedModule,
        out_of_scope: OutOfScopePolicy,
    ) -> ModuleScopeResolver<'a> {
        ModuleScopeResolver {
            resolved,
            prim,
            bundle_operator: None,
            planned_spellings: None,
            out_of_scope,
            unresolved: Vec::new(),
        }
    }

    pub fn with_bundle_operator(
        mut self,
        bundle_operator: Option<(FileId, TypeItemId)>,
    ) -> ModuleScopeResolver<'a> {
        self.bundle_operator = bundle_operator;
        self
    }

    pub fn with_planned_spellings(
        mut self,
        planned_spellings: &'a FxHashMap<(FileId, TypeItemId), SmolStr>,
    ) -> ModuleScopeResolver<'a> {
        self.planned_spellings = Some(planned_spellings);
        self
    }

    pub fn unresolved(&self) -> &[(FileId, TypeItemId)] {
        &self.unresolved
    }
}

impl TypeNameResolver for ModuleScopeResolver<'_> {
    fn resolve_type(
        &mut self,
        file_id: FileId,
        type_id: TypeItemId,
        name: &str,
        namespace: TypeNamespace,
    ) -> TypeNameResolution {
        match self.resolved.spell_type_item(self.prim, file_id, type_id, name, namespace) {
            ScopeSpelling::Unqualified(spelled) => TypeNameResolution::Name(spelled),
            ScopeSpelling::Qualified { qualifier, name } => {
                TypeNameResolution::Name(format_smolstr!("{qualifier}.{name}"))
            }
            ScopeSpelling::NotInScope => {
                if let Some(planned) =
                    self.planned_spellings.and_then(|map| map.get(&(file_id, type_id)))
                {
                    return TypeNameResolution::Name(SmolStr::clone(planned));
                }
                match self.out_of_scope {
                    OutOfScopePolicy::RecordUnresolved => {
                        if !self.unresolved.contains(&(file_id, type_id)) {
                            self.unresolved.push((file_id, type_id));
                        }
                        TypeNameResolution::Unresolved
                    }
                    OutOfScopePolicy::FallbackDefiningModule => {
                        TypeNameResolution::FallbackDefiningModule
                    }
                }
            }
        }
    }

    fn bundle_rendering(&self) -> BundleRendering {
        let Some((effect_file, operator_id)) = self.bundle_operator else {
            return BundleRendering::Constructor;
        };
        let spelling = self.resolved.spell_type_item(
            self.prim,
            effect_file,
            operator_id,
            "&",
            TypeNamespace::Type,
        );
        if matches!(spelling, ScopeSpelling::Unqualified(_)) {
            BundleRendering::Infix
        } else {
            BundleRendering::Constructor
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrettyConfig {
    width: usize,
    show_rigid_kinds: bool,
    show_forall_kinds: bool,
    fully_qualified_names: bool,
    quote_record_labels: bool,
}

impl PrettyConfig {
    pub const fn new() -> PrettyConfig {
        PrettyConfig {
            width: DEFAULT_WIDTH,
            show_rigid_kinds: true,
            show_forall_kinds: true,
            fully_qualified_names: false,
            quote_record_labels: false,
        }
    }

    #[must_use]
    pub const fn width(mut self, width: usize) -> PrettyConfig {
        self.width = width;
        self
    }

    #[must_use]
    pub const fn without_rigid_kinds(mut self) -> PrettyConfig {
        self.show_rigid_kinds = false;
        self
    }

    #[must_use]
    pub const fn without_forall_kinds(mut self) -> PrettyConfig {
        self.show_forall_kinds = false;
        self
    }

    #[must_use]
    pub const fn fully_qualified_names(mut self) -> PrettyConfig {
        self.fully_qualified_names = true;
        self
    }

    #[must_use]
    pub const fn quote_record_labels(mut self) -> PrettyConfig {
        self.quote_record_labels = true;
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

impl<'a, Q> Pretty<'a, Q>
where
    Q: PrettyQueries + ?Sized,
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

    pub fn state(&self) -> PrettyState<'a, Q> {
        PrettyState {
            queries: self.queries,
            checked: self.checked,
            config: self.config,
            names: PrettyNames::new(),
        }
    }

    pub fn render(&self, id: TypeId) -> QueryResult<SmolStr> {
        self.state().render(id)
    }

    pub fn render_kind(&self, id: TypeId) -> QueryResult<SmolStr> {
        self.state().render_kind(id)
    }

    pub fn render_signature(&self, name: &str, id: TypeId) -> QueryResult<SmolStr> {
        self.state().render_signature(name, id)
    }

    pub fn render_kind_signature(&self, name: &str, id: TypeId) -> QueryResult<SmolStr> {
        self.state().render_kind_signature(name, id)
    }

    pub fn render_with_resolver(
        &self,
        id: TypeId,
        resolver: &mut dyn TypeNameResolver,
    ) -> QueryResult<Option<SmolStr>> {
        self.state().render_with_resolver(id, resolver)
    }

    pub fn render_signature_with_resolver(
        &self,
        name: &str,
        id: TypeId,
        resolver: &mut dyn TypeNameResolver,
    ) -> QueryResult<Option<SmolStr>> {
        self.state().render_signature_with_resolver(name, id, resolver)
    }
}

pub struct PrettyState<'a, Q: ?Sized> {
    queries: &'a Q,
    checked: &'a CheckedModule,
    config: PrettyConfig,
    names: PrettyNames,
}

impl<'a, Q> PrettyState<'a, Q>
where
    Q: PrettyQueries + ?Sized,
{
    pub(crate) fn fork(&self) -> PrettyState<'a, Q> {
        PrettyState {
            queries: self.queries,
            checked: self.checked,
            config: self.config,
            names: self.names.clone(),
        }
    }

    pub fn display_name(&mut self, name: Name) -> SmolStr {
        self.names.set_default_name("t");
        self.names.display_name(self.queries, &self.checked.names, name)
    }

    pub(crate) fn assign_display_name(&mut self, name: Name, display: SmolStr) {
        self.names.assign_display_name(name, display);
    }

    pub(crate) fn allocate_display_name(&mut self, name: Name, base: SmolStr) {
        let display = self.names.allocate_display_name(base);
        self.names.assign_display_name(name, display);
    }

    pub(crate) fn is_function_type(&self, id: TypeId) -> QueryResult<bool> {
        match *self.queries.lookup_type(id) {
            Type::Function(..) => Ok(true),
            Type::Application(function, _) => {
                let Type::Application(constructor, _) = *self.queries.lookup_type(function) else {
                    return Ok(false);
                };
                let core = self.queries.checked_core()?;
                Ok(constructor == core.prim.function)
            }
            _ => Ok(false),
        }
    }

    pub fn render(&mut self, id: TypeId) -> QueryResult<SmolStr> {
        self.names.set_default_name("t");
        self.render_with_signature(None, id, Precedence::Top)
    }

    pub fn render_kind(&mut self, id: TypeId) -> QueryResult<SmolStr> {
        self.names.set_default_name("k");
        self.render_with_signature(None, id, Precedence::Top)
    }

    pub fn render_signature(&mut self, name: &str, id: TypeId) -> QueryResult<SmolStr> {
        self.names.set_default_name("t");
        self.render_with_signature(Some(name), id, Precedence::Top)
    }

    pub fn render_kind_signature(&mut self, name: &str, id: TypeId) -> QueryResult<SmolStr> {
        self.names.set_default_name("k");
        self.render_with_signature(Some(name), id, Precedence::Top)
    }

    pub fn render_with_resolver(
        &mut self,
        id: TypeId,
        resolver: &mut dyn TypeNameResolver,
    ) -> QueryResult<Option<SmolStr>> {
        self.names.set_default_name("t");
        self.render_with_signature_and_resolver(None, id, Precedence::Top, Some(resolver))
    }

    pub fn render_signature_with_resolver(
        &mut self,
        name: &str,
        id: TypeId,
        resolver: &mut dyn TypeNameResolver,
    ) -> QueryResult<Option<SmolStr>> {
        self.names.set_default_name("t");
        self.render_with_signature_and_resolver(Some(name), id, Precedence::Top, Some(resolver))
    }

    fn render_with_signature(
        &mut self,
        signature: Option<&str>,
        id: TypeId,
        precedence: Precedence,
    ) -> QueryResult<SmolStr> {
        let rendered = self.render_with_signature_and_resolver(signature, id, precedence, None)?;
        Ok(rendered.expect("invariant violated: unscoped render produced unresolved type"))
    }

    fn render_with_signature_and_resolver(
        &mut self,
        signature: Option<&str>,
        id: TypeId,
        precedence: Precedence,
        resolver: Option<&mut dyn TypeNameResolver>,
    ) -> QueryResult<Option<SmolStr>> {
        let arena = Arena::new();
        let core = self.queries.checked_core()?;
        let mut printer = Printer::new(
            &arena,
            self.queries,
            &core,
            &self.checked.names,
            &mut self.names,
            self.config.show_rigid_kinds,
            self.config.show_forall_kinds,
            self.config.fully_qualified_names,
            self.config.quote_record_labels,
            resolver,
        );

        let document = if let Some(name) = signature {
            printer.signature(name, id)?
        } else {
            printer.traverse(precedence, id)?
        };

        if printer.had_unresolved {
            return Ok(None);
        }

        let mut output = SmolStrBuilder::new();
        document
            .render_fmt(self.config.width, &mut output)
            .expect("critical failure: failed to render type");
        Ok(Some(output.finish()))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Top,
    Constraint,
    Function,
    Bundle,
    Application,
    Atom,
}

struct Printer<'arena, 'context, 'names, 'resolver, Q>
where
    Q: PrettyQueries + ?Sized,
{
    arena: &'arena Arena<'arena>,
    queries: &'context Q,
    core: &'context CheckedCore,
    names: &'context FxHashMap<Name, SmolStrId>,
    pretty_names: &'names mut PrettyNames,
    show_rigid_kinds: bool,
    show_forall_kinds: bool,
    fully_qualified_names: bool,
    quote_record_labels: bool,
    resolver: Option<&'resolver mut dyn TypeNameResolver>,
    had_unresolved: bool,
}

impl<'arena, 'context, 'names, 'resolver, Q> Printer<'arena, 'context, 'names, 'resolver, Q>
where
    Q: PrettyQueries + ?Sized,
{
    #[allow(clippy::too_many_arguments)]
    fn new(
        arena: &'arena Arena<'arena>,
        queries: &'context Q,
        core: &'context CheckedCore,
        names: &'context FxHashMap<Name, SmolStrId>,
        pretty_names: &'names mut PrettyNames,
        show_rigid_kinds: bool,
        show_forall_kinds: bool,
        fully_qualified_names: bool,
        quote_record_labels: bool,
        resolver: Option<&'resolver mut dyn TypeNameResolver>,
    ) -> Printer<'arena, 'context, 'names, 'resolver, Q> {
        Printer {
            arena,
            queries,
            core,
            names,
            pretty_names,
            show_rigid_kinds,
            show_forall_kinds,
            fully_qualified_names,
            quote_record_labels,
            resolver,
            had_unresolved: false,
        }
    }

    fn lookup_type(&self, id: TypeId) -> &Type {
        self.queries.lookup_type(id)
    }

    fn lookup_forall_binder(&self, id: ForallBinderId) -> ForallBinder {
        self.queries.lookup_forall_binder(id)
    }

    fn lookup_smol_str(&self, id: SmolStrId) -> &smol_str::SmolStr {
        self.queries.lookup_smol_str(id)
    }

    fn lookup_type_item_metadata(
        &self,
        file_id: files::FileId,
        type_id: indexing::TypeItemId,
    ) -> QueryResult<Option<(String, TypeNamespace)>> {
        let indexed = self.queries.indexed(file_id)?;
        let item = &indexed.items[type_id];
        let Some(name) = item.name.as_ref() else {
            return Ok(None);
        };
        let namespace = if matches!(item.kind, IndexedTypeItemKind::Class { .. }) {
            TypeNamespace::Class
        } else {
            TypeNamespace::Type
        };
        Ok(Some((name.to_string(), namespace)))
    }

    fn lookup_module_name(&self, file_id: files::FileId) -> QueryResult<Option<SmolStr>> {
        let content = self.queries.content(file_id)?;
        let (parsed, _) = self.queries.parsed(file_id)?;
        Ok(parsed.module_name(&content))
    }

    fn display_type_name(
        &mut self,
        file_id: files::FileId,
        type_id: indexing::TypeItemId,
    ) -> QueryResult<Option<String>> {
        let Some((name, namespace)) = self.lookup_type_item_metadata(file_id, type_id)? else {
            return Ok(None);
        };

        if let Some(resolver) = &mut self.resolver {
            return match resolver.resolve_type(file_id, type_id, &name, namespace) {
                TypeNameResolution::Name(spelled) => Ok(Some(spelled.to_string())),
                TypeNameResolution::FallbackDefiningModule => {
                    let Some(module_name) = self.lookup_module_name(file_id)? else {
                        return Ok(Some(name));
                    };
                    Ok(Some(format!("{module_name}.{name}")))
                }
                TypeNameResolution::Unresolved => {
                    self.had_unresolved = true;
                    Ok(None)
                }
            };
        }

        if !self.fully_qualified_names {
            return Ok(Some(name));
        }

        let Some(module_name) = self.lookup_module_name(file_id)? else {
            return Ok(Some(name));
        };

        Ok(Some(format!("{module_name}.{name}")))
    }

    fn is_bundle_infix(&self, id: TypeId) -> bool {
        id == self.core.iris_effect.bundle
            && self
                .resolver
                .as_ref()
                .is_none_or(|resolver| resolver.bundle_rendering() == BundleRendering::Infix)
    }

    fn split_function_type(&self, id: TypeId) -> Option<(TypeId, TypeId)> {
        match *self.lookup_type(id) {
            Type::Function(argument, result) => Some((argument, result)),
            Type::Application(function, result) => {
                if let Type::Application(constructor, argument) = *self.lookup_type(function)
                    && constructor == self.core.prim.function
                {
                    return Some((argument, result));
                }
                None
            }
            _ => None,
        }
    }

    fn parens_if(&self, condition: bool, doc: Doc<'arena>) -> Doc<'arena> {
        if condition { self.arena.text("(").append(doc).append(self.arena.text(")")) } else { doc }
    }

    fn signature(&mut self, name: &str, id: TypeId) -> QueryResult<Doc<'arena>> {
        let signature = self.traverse(Precedence::Top, id)?;
        let head = self.arena.text(format!("{name} ::"));
        Ok(breakable_continuation(self.arena, head, signature))
    }

    fn traverse(&mut self, precedence: Precedence, id: TypeId) -> QueryResult<Doc<'arena>> {
        let queries = self.queries;
        let document = match queries.lookup_type(id) {
            &Type::Application(function, argument) => {
                self.traverse_application(precedence, function, argument)?
            }

            &Type::KindApplication(function, argument) => {
                self.traverse_kind_application(precedence, function, argument)?
            }

            &Type::Forall(binder_id, inner) => {
                self.traverse_forall(precedence, binder_id, inner)?
            }

            &Type::Constrained(constraint, inner) => {
                self.traverse_constrained(precedence, constraint, inner)?
            }

            &Type::ConstraintOrigin(constraint, _) => self.traverse(precedence, constraint)?,

            &Type::Function(argument, result) => {
                self.traverse_function(precedence, argument, result)?
            }

            &Type::Kinded(inner, kind) => {
                let inner = self.traverse(Precedence::Application, inner)?;
                let kind = self.traverse(Precedence::Top, kind)?;
                let kinded = inner.append(self.arena.text(" :: ")).append(kind);
                self.parens_if(precedence > Precedence::Atom, kinded)
            }

            &Type::Constructor(..) if self.is_bundle_infix(id) => self.arena.text("(&)"),

            &Type::Constructor(file_id, type_id) | &Type::Synonym(file_id, type_id) => {
                let name = self
                    .display_type_name(file_id, type_id)?
                    .unwrap_or_else(|| "<InvalidName>".to_string());
                self.arena.text(name)
            }

            &Type::Integer(integer) => {
                let integer = queries.lookup_integer(integer);
                let negative = integer.sign() == Sign::Minus;
                let integer = self.arena.text(format!("{integer}"));
                self.parens_if(negative, integer)
            }

            &Type::String(kind, string) => match kind {
                StringKind::String => {
                    let string = queries.lookup_string_literal(string);
                    self.arena.text(lowering::literal::encode_normal_string(string))
                }
                StringKind::RawString => {
                    let string = queries.lookup_string_literal(string);
                    let string = string.to_utf8().unwrap_or_else(|_| {
                        unreachable!("invariant violated: raw string contains a lone surrogate")
                    });
                    self.arena.text(format!("\"\"\"{string}\"\"\""))
                }
            },

            &Type::Row(row_id) => {
                let row = queries.lookup_row_type(row_id);
                if row.fields.is_empty() && row.tail.is_none() {
                    return Ok(self.arena.text("()"));
                }
                self.format_row(&row.fields, row.tail)?
            }

            &Type::Rigid(name, _, kind) => {
                let text = self.pretty_names.display_name(self.queries, self.names, name);
                if self.show_rigid_kinds && kind != self.core.prim.t {
                    let kind = self.traverse(Precedence::Top, kind)?;
                    self.arena
                        .text(format!("({text} :: "))
                        .append(kind)
                        .append(self.arena.text(")"))
                } else {
                    self.arena.text(text.to_string())
                }
            }

            &Type::Unification(unification_id) => self.arena.text(format!("?{unification_id}")),

            &Type::Free(name_id) => {
                let name = self.lookup_smol_str(name_id);
                self.arena.text(format!("{name}"))
            }

            &Type::Unknown(name_id) => {
                let name = self.lookup_smol_str(name_id);
                self.arena.text(format!("?[{name}]"))
            }
        };
        Ok(document)
    }
}

impl<'arena, 'context, 'names, 'resolver, Q> Printer<'arena, 'context, 'names, 'resolver, Q>
where
    Q: PrettyQueries + ?Sized,
{
    fn traverse_application(
        &mut self,
        precedence: Precedence,
        mut function: TypeId,
        argument: TypeId,
    ) -> QueryResult<Doc<'arena>> {
        if function == self.core.prim.record {
            return self.format_record_application(argument);
        }

        let mut arguments = vec![argument];

        while let Type::Application(inner_function, argument) = *self.lookup_type(function) {
            function = inner_function;
            arguments.push(argument);
        }

        if let &[result, argument] = arguments.as_slice()
            && function == self.core.prim.function
        {
            return self.traverse_function(precedence, argument, result);
        }

        if let &[right, left] = arguments.as_slice()
            && self.is_bundle_infix(function)
        {
            let left = self.traverse(Precedence::Application, left)?;
            let right = self.traverse(Precedence::Bundle, right)?;
            let bundle = left.append(self.arena.text(" &")).append(self.arena.line()).append(right);
            return Ok(self.parens_if(precedence > Precedence::Bundle, bundle.group()));
        }

        let function = self.traverse(Precedence::Application, function)?;

        let arguments = arguments.iter().rev();
        let arguments = arguments.map(|&argument| self.traverse(Precedence::Atom, argument));
        let arguments = arguments.collect::<QueryResult<Vec<_>>>()?;

        let arguments = self.arena.intersperse(arguments, self.arena.line());
        let application = breakable_continuation(self.arena, function, arguments);
        Ok(self.parens_if(precedence > Precedence::Application, application))
    }

    fn format_record_application(&mut self, argument: TypeId) -> QueryResult<Doc<'arena>> {
        let queries = self.queries;
        match queries.lookup_type(argument) {
            &Type::Row(row_id) => {
                let row = queries.lookup_row_type(row_id);
                self.format_record(&row.fields, row.tail)
            }
            _ => {
                let inner = self.traverse(Precedence::Top, argument)?;
                Ok(self.arena.text("{| ").append(inner).append(self.arena.text(" }")))
            }
        }
    }

    fn traverse_kind_application(
        &mut self,
        precedence: Precedence,
        mut function: TypeId,
        argument: TypeId,
    ) -> QueryResult<Doc<'arena>> {
        let mut arguments = vec![argument];

        while let Type::KindApplication(inner_function, argument) = *self.lookup_type(function) {
            function = inner_function;
            arguments.push(argument);
        }

        let function = self.traverse(Precedence::Application, function)?;

        let arguments = arguments.iter().rev();
        let arguments = arguments.map(|&argument| self.traverse(Precedence::Atom, argument));
        let arguments = arguments.collect::<QueryResult<Vec<_>>>()?;

        let arguments = arguments.into_iter().map(|argument| self.arena.text("@").append(argument));
        let arguments = self.arena.intersperse(arguments, self.arena.line());
        let application = breakable_continuation(self.arena, function, arguments);
        Ok(self.parens_if(precedence > Precedence::Application, application))
    }
}

impl<'arena, 'context, 'names, 'resolver, Q> Printer<'arena, 'context, 'names, 'resolver, Q>
where
    Q: PrettyQueries + ?Sized,
{
    fn traverse_forall(
        &mut self,
        precedence: Precedence,
        binder_id: ForallBinderId,
        mut inner: TypeId,
    ) -> QueryResult<Doc<'arena>> {
        let binder = self.lookup_forall_binder(binder_id);
        let mut binders = vec![binder];

        while let Type::Forall(next_binder_id, next_inner) = *self.lookup_type(inner) {
            binders.push(self.lookup_forall_binder(next_binder_id));
            inner = next_inner;
        }

        let binders = binders.iter().map(|binder| {
            let name = self.pretty_names.display_name(self.queries, self.names, binder.name);
            let text = if binder.visible { format!("@{name}") } else { name.to_string() };
            if self.show_forall_kinds && binder.kind != self.core.prim.t {
                let kind = self.traverse(Precedence::Top, binder.kind)?;
                Ok(self
                    .arena
                    .text(format!("({} :: ", text))
                    .append(kind)
                    .append(self.arena.text(")"))
                    .group())
            } else {
                Ok(self.arena.text(text))
            }
        });

        let binders = binders.collect::<QueryResult<Vec<_>>>()?;

        let mut binders = binders.into_iter();
        let binders = if let Some(first) = binders.next() {
            binders.fold(first, |builder, binder| {
                builder.append(self.arena.line().append(binder).nest(2).group())
            })
        } else {
            self.arena.nil()
        };

        let header = self.arena.text("forall ").append(binders).append(self.arena.text("."));
        let inner = self.traverse(Precedence::Top, inner)?;
        let forall = breakable_continuation(self.arena, header, inner);

        Ok(self.parens_if(precedence > Precedence::Top, forall))
    }

    fn traverse_constrained(
        &mut self,
        precedence: Precedence,
        constraint: TypeId,
        mut inner: TypeId,
    ) -> QueryResult<Doc<'arena>> {
        let mut constraints = vec![constraint];

        while let Type::Constrained(constraint, next_inner) = *self.lookup_type(inner) {
            constraints.push(constraint);
            inner = next_inner;
        }

        let constraints = constraints.iter();
        let constraints =
            constraints.map(|&constraint| self.traverse(Precedence::Application, constraint));
        let constraints = constraints.collect::<QueryResult<Vec<_>>>()?;

        let inner = self.traverse(Precedence::Constraint, inner)?;

        let arrow = self.arena.text(" =>").append(self.arena.line());
        let constraints = constraints.into_iter().fold(self.arena.nil(), |builder, constraint| {
            builder.append(constraint).append(arrow.clone())
        });

        let constraints = constraints.append(inner).group();
        Ok(self.parens_if(precedence > Precedence::Constraint, constraints))
    }

    fn traverse_function(
        &mut self,
        precedence: Precedence,
        argument: TypeId,
        mut result: TypeId,
    ) -> QueryResult<Doc<'arena>> {
        let mut arguments = vec![argument];

        while let Some((argument, next_result)) = self.split_function_type(result) {
            result = next_result;
            arguments.push(argument);
        }

        let arguments = arguments.iter();
        let arguments = arguments.map(|&argument| self.traverse(Precedence::Application, argument));
        let arguments = arguments.collect::<QueryResult<Vec<_>>>()?;

        let result = self.traverse(Precedence::Function, result)?;

        let arrow = self.arena.text(" ->").append(self.arena.line());
        let arguments = arguments.into_iter().fold(self.arena.nil(), |builder, argument| {
            builder.append(argument).append(arrow.clone())
        });

        let function = arguments.append(result).group();
        Ok(self.parens_if(precedence > Precedence::Function, function))
    }
}

impl<'arena, 'context, 'names, 'resolver, Q> Printer<'arena, 'context, 'names, 'resolver, Q>
where
    Q: PrettyQueries + ?Sized,
{
    fn format_record(
        &mut self,
        fields: &[RowField],
        tail: Option<TypeId>,
    ) -> QueryResult<Doc<'arena>> {
        if fields.is_empty() && tail.is_none() {
            return Ok(self.arena.text("{}"));
        }
        let body = self.format_row_body(fields, tail)?;
        Ok(self
            .arena
            .text("{ ")
            .append(body)
            .append(self.arena.line())
            .append(self.arena.text("}"))
            .group())
    }

    fn format_row(
        &mut self,
        fields: &[RowField],
        tail: Option<TypeId>,
    ) -> QueryResult<Doc<'arena>> {
        let body = self.format_row_body(fields, tail)?;
        Ok(self
            .arena
            .text("( ")
            .append(body)
            .append(self.arena.line())
            .append(self.arena.text(")"))
            .group())
    }

    fn format_row_body(
        &mut self,
        fields: &[RowField],
        tail: Option<TypeId>,
    ) -> QueryResult<Doc<'arena>> {
        if fields.is_empty() {
            return if let Some(tail) = tail {
                let tail = self.traverse(Precedence::Top, tail)?;
                Ok(self.arena.text("| ").append(tail))
            } else {
                Ok(self.arena.nil())
            };
        }

        let fields = fields.iter().map(|field| {
            let field_type = self.traverse(Precedence::Top, field.id)?;
            let label = if self.quote_record_labels && !is_unquoted_record_label(&field.label) {
                lowering::literal::encode_normal_string(&lowering::StringLiteral::from(
                    field.label.as_str(),
                ))
            } else {
                field.label.to_string()
            };
            Ok((label, field_type))
        });

        let fields = fields.collect::<QueryResult<Vec<_>>>()?;

        let format_field =
            |arena: &'arena Arena<'arena>, label: String, field_type: Doc<'arena>| {
                let field_type = arena.line().append(field_type).nest(2).group();
                arena.text(format!("{label} ::")).append(field_type).align()
            };

        let mut fields = fields.into_iter();
        let (first_label, first_type) = fields.next().unwrap();
        let first = format_field(self.arena, first_label, first_type);

        let leading_comma = self.arena.hardline().append(self.arena.text(", "));
        let leading_comma = leading_comma.flat_alt(self.arena.text(", "));

        let fields = fields.fold(first, |builder, (label, field_type)| {
            builder
                .append(leading_comma.clone())
                .append(format_field(self.arena, label, field_type))
        });

        if let Some(tail) = tail {
            let tail = self.traverse(Precedence::Top, tail)?;
            let leading_pipe = self.arena.hardline().append(self.arena.text("| "));
            let leading_pipe = leading_pipe.flat_alt(self.arena.text(" | "));
            Ok(fields.append(leading_pipe).append(tail))
        } else {
            Ok(fields)
        }
    }
}

fn is_unquoted_record_label(label: &str) -> bool {
    if label == "_" {
        return false;
    }
    let mut characters = label.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    let valid_start = if first.is_ascii() {
        first.is_ascii_lowercase() || first == '_'
    } else {
        first.is_lowercase()
    };
    valid_start
        && characters.all(|character| {
            if character.is_ascii() {
                character.is_ascii_alphanumeric() || character == '_' || character == '\''
            } else {
                character.is_alphanumeric()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smol_str(text: &str) -> SmolStr {
        SmolStr::new(text)
    }

    #[test]
    fn allocate_display_name_skips_reserved_suffixes() {
        let mut names = PrettyNames::new();

        assert_eq!(names.allocate_display_name(smol_str("t")), smol_str("t"));
        assert_eq!(names.allocate_display_name(smol_str("t1")), smol_str("t1"));
        assert_eq!(names.allocate_display_name(smol_str("t3")), smol_str("t3"));
        assert_eq!(names.allocate_display_name(smol_str("t2")), smol_str("t2"));
        assert_eq!(names.allocate_display_name(smol_str("t")), smol_str("t4"));
    }

    #[test]
    fn allocate_display_name_checks_actual_candidates() {
        let mut names = PrettyNames::new();

        assert_eq!(names.allocate_display_name(smol_str("t01")), smol_str("t01"));
        assert_eq!(names.allocate_display_name(smol_str("t")), smol_str("t"));
        assert_eq!(names.allocate_display_name(smol_str("t")), smol_str("t1"));
        assert_eq!(names.allocate_display_name(smol_str("t0")), smol_str("t0"));
        assert_eq!(names.allocate_display_name(smol_str("t0")), smol_str("t02"));
    }

    #[test]
    fn allocate_generated_display_name_starts_at_zero() {
        let mut names = PrettyNames::new();

        assert_eq!(names.allocate_generated_display_name(smol_str("t")), smol_str("t0"));
        assert_eq!(names.allocate_generated_display_name(smol_str("t")), smol_str("t1"));
    }

    #[test]
    fn allocate_generated_display_name_skips_source_names() {
        let mut names = PrettyNames::new();

        assert_eq!(names.allocate_display_name(smol_str("t0")), smol_str("t0"));
        assert_eq!(names.allocate_generated_display_name(smol_str("t")), smol_str("t1"));
        assert_eq!(names.allocate_display_name(smol_str("a")), smol_str("a"));
        assert_eq!(names.allocate_display_name(smol_str("a")), smol_str("a1"));
    }

    #[test]
    fn allocate_display_name_allows_max_suffix_candidate() {
        let mut names = PrettyNames::new();
        names.next_suffix.insert(smol_str("t"), NonZeroU32::new(u32::MAX).unwrap());

        assert_eq!(names.allocate_display_name(smol_str("t")), smol_str("t4294967295"));
    }

    #[test]
    #[should_panic(expected = "critical failure: exhausted suffixes")]
    fn allocate_display_name_reports_suffix_exhaustion() {
        let mut names = PrettyNames::new();
        names.next_suffix.insert(smol_str("t"), NonZeroU32::new(u32::MAX).unwrap());
        names.next_suffix.insert(smol_str("t4294967295"), FIRST_SUFFIX);

        names.allocate_display_name(smol_str("t"));
    }
}
