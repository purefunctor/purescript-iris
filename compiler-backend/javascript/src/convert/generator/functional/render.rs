//! Rendering functional trees as JavaScript modules.

mod inline;
mod structure;
mod stylex;
mod syntax;
mod tail_call;

use std::rc::Rc;

use files::{FileId, ForeignSourceKind};
use functional::initializers::{cyclic_initializers, initializer_postorder};
use functional::optimize::{for_each_expression_child, local_uses};
use functional::tree::{
    Binding, CaseAlternative, Declaration, DeclarationKind, EffectExpression,
    ExpressionId as FunctionalExpressionId, ExpressionKind, Global, GlobalId, Guard,
    GuardedAlternative, LocalId, Module as FunctionalModule, ModuleDependency, Parameter,
    PatternId, PatternKind, RecordUpdate,
};
use itertools::Itertools;
use oxc_allocator::Allocator;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::{SmolStr, format_smolstr};

use super::super::names::NameAllocator;
use crate::error::{ModuleDiagnostic, ModuleError, ModuleResult, UnsupportedState};
use crate::module::{Module, module_filename, runtime_filename};
use crate::tree::{BinaryOperator, ExpressionId, ObjectProperty, Tree, UnaryOperator};
use crate::writer::{BindingCallTarget, Writer};

use self::inline::{is_abstraction, pattern_parameter};
use self::structure::{
    collect_module_references, cyclic_instance_initializers, has_local_lazy_initializers,
};
use self::stylex::collect_stylex_references;
use self::syntax::{
    binary_expression, combine_conditions, constructor_expression, curried_call_expression,
    literal_expression, synthesized_evidence_expression, unary_expression,
};
use self::tail_call::{
    TailCallContext, TailCallGroup, TailCallIdentity, TailCallProfile, global_profiles,
    local_profiles, tail_call_group,
};

const SOURCE_ERROR_MESSAGE: &str = "Generated code reached a source error";
const INITIALIZER_CYCLE_MESSAGE: &str = "Top-level value initializer cycle";

pub(crate) struct Generator<'m> {
    module: &'m FunctionalModule,
    module_dependencies: FxHashMap<FileId, &'m ModuleDependency>,
    global_names: FxHashMap<GlobalId, SmolStr>,
    external_module_namespaces: FxHashMap<FileId, SmolStr>,
    external_named_imports: FxHashMap<GlobalId, SmolStr>,
    external_references: Vec<Global>,
    stylex_namespace: Option<SmolStr>,
    foreign_import: Option<ForeignImport>,
    runtime_namespace: Option<SmolStr>,
    lazy_global_names: FxHashMap<GlobalId, SmolStr>,
    global_tail_call_groups: Vec<TailCallGroup>,
    global_tail_call_group_positions: FxHashMap<GlobalId, usize>,
    reserved_module_names: Rc<FxHashSet<SmolStr>>,
}

struct ForeignImport {
    namespace: SmolStr,
    kind: ForeignSourceKind,
}

#[derive(Debug)]
enum LocalBinding {
    Direct(SmolStr),
    Inline(ExpressionId),
    Lazy(SmolStr),
}

struct FunctionContext {
    allocator: NameAllocator,
    locals: FxHashMap<LocalId, LocalBinding>,
    tail_calls: Option<TailCallContext>,
}

#[derive(Clone, Copy)]
enum Destination<'a> {
    Return,
    TailEffectThunkReturn,
    TailEffectReturn,
    EffectReturn,
    EffectTailEffectReturn,
    Assign(&'a str),
    EffectAssign(&'a str),
    AssignAndBreak { name: &'a str, label: &'a str },
    EffectAssignAndBreak { name: &'a str, label: &'a str },
}

enum CapturedEffectAction {
    Expression(ExpressionId),
    Effect(Box<CapturedEffect>),
}

enum CapturedEffect {
    Pure { value: ExpressionId },
    Bind { action: CapturedEffectAction, parameter: Parameter, body: FunctionalExpressionId },
    Map { function: ExpressionId, action: CapturedEffectAction },
    Apply { function_action: CapturedEffectAction, argument_action: CapturedEffectAction },
}

#[derive(Default)]
struct PatternPlan {
    conditions: Vec<ExpressionId>,
    bindings: Vec<PatternBinding>,
}

enum PatternBinding {
    Variable { name: SmolStr, value: ExpressionId },
    Constructor { names: Vec<Option<SmolStr>>, value: ExpressionId },
}

// Pending expressions must be evaluated before rendering an eager later sibling.
struct RenderedExpression {
    value: ExpressionId,
    pending_evaluation: bool,
}

struct TailCallWrapper<'a> {
    name: &'a str,
    group: &'a TailCallGroup,
    profile: &'a TailCallProfile,
    state: usize,
}

struct CurriedTailCallWrapper<'a> {
    group: &'a TailCallGroup,
    profile: &'a TailCallProfile,
    state: usize,
    arguments: &'a [SmolStr],
    remaining: &'a [SmolStr],
}

struct TailCallWrapperResult<'a> {
    group: &'a TailCallGroup,
    profile: &'a TailCallProfile,
    state: usize,
    arguments: &'a [SmolStr],
}

struct CurriedParameter<'a> {
    pattern: PatternId,
    argument: &'a str,
    remaining: &'a [PatternId],
    body: FunctionalExpressionId,
}

struct UncurriedParameters<'a> {
    patterns: &'a [PatternId],
    arguments: &'a [SmolStr],
    position: usize,
    body: FunctionalExpressionId,
}

struct AbstractionBinding<'a> {
    name: &'a str,
    parameters: &'a [PatternId],
    body: FunctionalExpressionId,
    uncurried: bool,
}

struct GuardSequence<'a, 'd> {
    guards: &'a [Guard],
    position: usize,
    expression: FunctionalExpressionId,
    destination: Destination<'d>,
}

struct ModuleRenderer<'a, 'm, 't> {
    generator: &'a Generator<'m>,
    tree: &'a mut Tree<'t>,
    writer: &'a mut Writer<'t>,
}

struct FunctionRenderer<'a, 'm, 't> {
    generator: &'a Generator<'m>,
    tree: &'a mut Tree<'t>,
    writer: &'a mut Writer<'t>,
    context: &'a mut FunctionContext,
}

impl FunctionContext {
    fn new(reserved: &Rc<FxHashSet<SmolStr>>) -> FunctionContext {
        FunctionContext {
            allocator: NameAllocator::with_reserved(Rc::clone(reserved)),
            locals: FxHashMap::default(),
            tail_calls: None,
        }
    }

    fn allocate(&mut self, preferred: impl AsRef<str>) -> SmolStr {
        self.allocator.allocate(preferred)
    }

    fn bind_direct(&mut self, parameter: &Parameter, name: SmolStr) {
        self.locals.insert(parameter.id, LocalBinding::Direct(name));
    }

    fn bind_inline(&mut self, parameter: &Parameter, expression: ExpressionId) {
        self.locals.insert(parameter.id, LocalBinding::Inline(expression));
    }

    fn bind_lazy(&mut self, parameter: &Parameter, name: SmolStr) {
        self.locals.insert(parameter.id, LocalBinding::Lazy(name));
    }
}

impl<'a> Destination<'a> {
    fn effect(self) -> Destination<'a> {
        match self {
            Destination::Return => Destination::EffectReturn,
            Destination::TailEffectReturn => Destination::EffectTailEffectReturn,
            Destination::Assign(name) => Destination::EffectAssign(name),
            Destination::AssignAndBreak { name, label } => {
                Destination::EffectAssignAndBreak { name, label }
            }
            Destination::TailEffectThunkReturn
            | Destination::EffectReturn
            | Destination::EffectTailEffectReturn
            | Destination::EffectAssign(_)
            | Destination::EffectAssignAndBreak { .. } => {
                unreachable!("invariant violated: effect destination is already indirect")
            }
        }
    }

    fn value(self) -> Destination<'a> {
        match self {
            Destination::EffectReturn => Destination::Return,
            Destination::EffectTailEffectReturn => Destination::TailEffectReturn,
            Destination::EffectAssign(name) => Destination::Assign(name),
            Destination::EffectAssignAndBreak { name, label } => {
                Destination::AssignAndBreak { name, label }
            }
            Destination::Return
            | Destination::TailEffectThunkReturn
            | Destination::TailEffectReturn
            | Destination::Assign(_)
            | Destination::AssignAndBreak { .. } => {
                unreachable!("invariant violated: value destination is already direct")
            }
        }
    }

    fn is_effect(self) -> bool {
        matches!(
            self,
            Destination::EffectReturn
                | Destination::EffectTailEffectReturn
                | Destination::EffectAssign(_)
                | Destination::EffectAssignAndBreak { .. }
        )
    }
}

impl<'m> Generator<'m> {
    pub(crate) fn new(
        module: &'m FunctionalModule,
        foreign_kind: Option<ForeignSourceKind>,
    ) -> Generator<'m> {
        let mut allocator = NameAllocator::default();
        let mut global_names = FxHashMap::default();
        for declaration in module.declarations.iter() {
            let name = allocator.allocate(&declaration.global.item_name);
            global_names.insert(declaration.global.id, name);
        }

        let mut module_dependencies = FxHashMap::default();
        for dependency in module.dependencies.iter() {
            module_dependencies.entry(dependency.file_id).or_insert(dependency);
        }

        let external_references = collect_module_references(module);
        let mut expressions = module.storage.expressions();
        let has_stylex =
            expressions.any(|(_, expression)| matches!(expression.kind, ExpressionKind::StyleX(_)));
        let stylex_references =
            if has_stylex { collect_stylex_references(module) } else { Vec::new() };
        let stylex_reference_ids =
            stylex_references.iter().map(|global| global.id).collect::<FxHashSet<_>>();
        let mut external_named_imports = FxHashMap::default();
        for global in stylex_references {
            let file_id = global_file(global.id);
            let dependency = module_dependencies
                .get(&file_id)
                .expect("invariant violated: external global has no module dependency");
            let preferred = format_smolstr!("{}_{}", dependency.module_name, global.item_name);
            let name = allocator.allocate(preferred.replace('.', "_"));
            external_named_imports.insert(global.id, name);
        }
        let mut external_module_namespaces = FxHashMap::default();
        for global in
            external_references.iter().filter(|global| !stylex_reference_ids.contains(&global.id))
        {
            let file_id = global_file(global.id);
            let dependency = module_dependencies
                .get(&file_id)
                .expect("invariant violated: external global has no module dependency");
            external_module_namespaces
                .entry(file_id)
                .or_insert_with(|| allocator.allocate(dependency.module_name.replace('.', "_")));
        }

        let stylex_namespace = has_stylex.then(|| allocator.allocate("$stylex"));

        let has_foreign = module
            .declarations
            .iter()
            .any(|declaration| matches!(declaration.kind, DeclarationKind::Foreign));
        let foreign_import = has_foreign.then(|| ForeignImport {
            namespace: allocator.allocate("$foreign"),
            kind: foreign_kind.unwrap_or(ForeignSourceKind::JavaScript),
        });
        let lazy_globals = cyclic_instance_initializers(module);
        let requires_runtime = !lazy_globals.is_empty() || has_local_lazy_initializers(module);
        let runtime_namespace = requires_runtime.then(|| allocator.allocate("$runtime"));
        let lazy_global_names = lazy_globals.into_iter().map(|id| {
            let global_name = &global_names[&id];
            let lazy_name = allocator.allocate(format_smolstr!("$lazy_{global_name}"));
            (id, lazy_name)
        });
        let lazy_global_names = lazy_global_names.collect();
        let mut global_tail_call_groups = Vec::new();
        let mut global_tail_call_group_positions = FxHashMap::default();
        for profiles in global_profiles(module) {
            let Some(TailCallIdentity::Global(_)) =
                profiles.first().map(|profile| profile.identity)
            else {
                continue;
            };
            let mut dispatcher_names = profiles.iter().map(|profile| {
                let TailCallIdentity::Global(global) = profile.identity else {
                    unreachable!("invariant violated: global tail-call group contains a local")
                };
                global_names[&global].as_str()
            });
            let dispatcher_suffix = dispatcher_names.join("_");
            let preferred = format_smolstr!("$tail_{dispatcher_suffix}");
            let dispatcher_name = allocator.allocate(&preferred);
            let Some(group) = tail_call_group(module, profiles, dispatcher_name) else {
                continue;
            };
            let position = global_tail_call_groups.len();
            for profile in &group.profiles {
                let TailCallIdentity::Global(global) = profile.identity else {
                    unreachable!("invariant violated: global tail-call group contains a local")
                };
                global_tail_call_group_positions.insert(global, position);
            }
            global_tail_call_groups.push(group);
        }
        let reserved_module_names = allocator.allocated_names().cloned().collect();
        let reserved_module_names = Rc::new(reserved_module_names);

        Generator {
            module,
            module_dependencies,
            global_names,
            external_module_namespaces,
            external_named_imports,
            external_references,
            stylex_namespace,
            foreign_import,
            runtime_namespace,
            lazy_global_names,
            global_tail_call_groups,
            global_tail_call_group_positions,
            reserved_module_names,
        }
    }

    pub(crate) fn generate(self) -> ModuleResult<Module> {
        let allocator = Allocator::default();
        let mut tree = Tree::new(&allocator);
        let mut writer = Writer::new(&allocator);
        let initializer_cycle = {
            let mut renderer =
                ModuleRenderer { generator: &self, tree: &mut tree, writer: &mut writer };
            render_imports(&mut renderer);
            render_constructors(&mut renderer);
            render_source_functions(&mut renderer)?;
            render_foreign_declarations(&mut renderer);
            render_lazy_initializers(&mut renderer)?;
            let initializer_cycle = render_value_declarations(&mut renderer)?;
            render_exports(&mut renderer);
            initializer_cycle
        };

        let dependencies = self.module.dependencies.iter().map(|dependency| dependency.file_id);
        let dependencies = dependencies.collect_vec();
        let diagnostics = if initializer_cycle.is_empty() {
            vec![]
        } else {
            vec![ModuleDiagnostic::InitializerCycle { declarations: initializer_cycle }]
        };
        let requires_runtime = self.runtime_namespace.is_some();
        let source = writer.finish();
        Ok(Module::new(
            self.module.file_id,
            self.module.name.to_string(),
            source,
            dependencies,
            diagnostics,
            self.foreign_import.as_ref().map(|foreign_import| foreign_import.kind),
            requires_runtime,
        ))
    }

    fn renderer<'a, 't>(
        &'a self,
        tree: &'a mut Tree<'t>,
        writer: &'a mut Writer<'t>,
        context: &'a mut FunctionContext,
    ) -> FunctionRenderer<'a, 'm, 't> {
        FunctionRenderer { generator: self, tree, writer, context }
    }
}

fn render_imports(renderer: &mut ModuleRenderer<'_, '_, '_>) {
    let ModuleRenderer { generator, writer, .. } = renderer;
    let mut files = generator
        .external_module_namespaces
        .keys()
        .map(|&file_id| {
            let dependency = generator.module_dependency(file_id);
            (file_id, dependency.module_name.as_str())
        })
        .collect_vec();
    files.sort_by_key(|(_, module_name)| *module_name);
    for (file_id, module_name) in files {
        let namespace = &generator.external_module_namespaces[&file_id];
        let path = format!("../{}", module_filename(module_name));
        writer.import_namespace(namespace, &path);
    }
    let mut named_files = generator
        .external_named_imports
        .keys()
        .map(|global| global_file(*global))
        .collect::<FxHashSet<_>>()
        .into_iter()
        .collect_vec();
    named_files.sort_by_key(|file_id| generator.module_dependency(*file_id).module_name.as_str());
    for file_id in named_files {
        let dependency = generator.module_dependency(file_id);
        let path = format!("../{}", module_filename(&dependency.module_name));
        let bindings = generator
            .external_named_imports
            .iter()
            .filter(|(global, _)| global_file(**global) == file_id)
            .map(|(global, local)| {
                let imported = generator
                    .external_references
                    .iter()
                    .find(|reference| reference.id == *global)
                    .expect("invariant violated: named import has no external reference")
                    .item_name
                    .as_str();
                (imported, local.as_str())
            });
        let mut bindings = bindings.collect_vec();
        bindings.sort_unstable();
        writer.import_named(&bindings, &path);
    }
    if let Some(namespace) = &generator.stylex_namespace {
        writer.import_namespace(namespace, "@stylexjs/stylex");
    }
    if let Some(foreign_import) = &generator.foreign_import {
        let path = format!("./foreign.{}", foreign_import.kind.extension());
        writer.import_namespace(&foreign_import.namespace, &path);
    }
    if let Some(namespace) = &generator.runtime_namespace {
        let path = format!("../{}", runtime_filename());
        writer.import_namespace(namespace, &path);
    }
    if !generator.external_references.is_empty()
        || generator.stylex_namespace.is_some()
        || !generator.external_named_imports.is_empty()
        || generator.foreign_import.is_some()
        || generator.runtime_namespace.is_some()
    {
        writer.blank();
    }
}

fn render_constructors(renderer: &mut ModuleRenderer<'_, '_, '_>) {
    let ModuleRenderer { generator, tree, writer } = renderer;
    let mut rendered = false;
    for declaration in generator.module.declarations.iter() {
        let DeclarationKind::Constructor { arity } = declaration.kind else {
            continue;
        };
        let name = generator.global_name(declaration.global.id);
        let expression = constructor_expression(tree, &declaration.global.item_name, arity);
        let exported = generator.declaration_is_inline_exported(declaration);
        writer.constant(tree, name, expression, exported);
        rendered = true;
    }
    if rendered {
        writer.blank();
    }
}

fn render_source_functions(renderer: &mut ModuleRenderer<'_, '_, '_>) -> ModuleResult<()> {
    let generator = renderer.generator;
    let mut rendered_groups = FxHashSet::default();
    for declaration in generator.module.declarations.iter() {
        let DeclarationKind::Value(expression) = declaration.kind else {
            continue;
        };
        let kind = &generator.module.storage[expression].kind;
        if !matches!(
            kind,
            ExpressionKind::Abstraction { .. } | ExpressionKind::UncurriedAbstraction { .. }
        ) {
            continue;
        }
        if let Some(&position) =
            generator.global_tail_call_group_positions.get(&declaration.global.id)
        {
            if rendered_groups.insert(position) {
                render_global_tail_call_group(
                    renderer,
                    &generator.global_tail_call_groups[position],
                )?;
                renderer.writer.blank();
            }
            continue;
        }
        let name = generator.global_name(declaration.global.id);
        let exported = generator.declaration_is_inline_exported(declaration);
        let mut context = FunctionContext::new(&generator.reserved_module_names);
        let mut function_renderer =
            generator.renderer(renderer.tree, renderer.writer, &mut context);
        render_named_function(&mut function_renderer, name, expression, exported)?;
        renderer.writer.blank();
    }
    Ok(())
}

fn render_global_tail_call_group(
    renderer: &mut ModuleRenderer<'_, '_, '_>,
    group: &TailCallGroup,
) -> ModuleResult<()> {
    let generator = renderer.generator;
    let mut context = FunctionContext::new(&generator.reserved_module_names);
    if !group.is_singleton() {
        let state_name = context.allocate("$state");
        let argument_names = (0..group.maximum_arity)
            .map(|position| context.allocate(format_smolstr!("$argument{position}")))
            .collect::<Rc<[_]>>();
        let tail_calls =
            TailCallContext::new(group, state_name.clone(), Rc::clone(&argument_names));
        let mut dispatcher_parameters = vec![state_name];
        dispatcher_parameters.extend(argument_names.iter().cloned());
        context.tail_calls = Some(tail_calls);
        renderer.writer.function(
            &group.dispatcher_name,
            dispatcher_parameters,
            false,
            |writer| {
                generator.render_tail_call_dispatcher(renderer.tree, writer, group, &mut context)
            },
        )?;
        context.tail_calls = None;
    }

    for (state, profile) in group.profiles.iter().enumerate() {
        let TailCallIdentity::Global(global) = profile.identity else {
            unreachable!("invariant violated: global tail-call group contains a local")
        };
        let declaration = generator
            .module
            .declarations
            .iter()
            .find(|declaration| declaration.global.id == global)
            .expect("invariant violated: tail-call profile has no declaration");
        let name = generator.global_name(global);
        let exported = generator.declaration_is_inline_exported(declaration);
        generator.render_global_tail_call_wrapper(
            renderer.tree,
            renderer.writer,
            TailCallWrapper { name, group, profile, state },
            exported,
            &mut context,
        )?;
    }
    Ok(())
}

impl Generator<'_> {
    fn render_tail_call_dispatcher<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        group: &TailCallGroup,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let condition = tree.boolean(true);
        writer.while_loop(tree, condition, |tree, writer| {
            if group.is_singleton() {
                return self.render_tail_call_profile(tree, writer, &group.profiles[0], context);
            }
            let tail_calls = context
                .tail_calls
                .as_ref()
                .expect("invariant violated: tail-call dispatcher has no context");
            let state_name = tail_calls
                .state_name
                .as_ref()
                .expect("invariant violated: mutual tail-call dispatcher has no state");
            let current_state = tree.identifier(state_name);
            let cases = group.profiles.iter().enumerate().map(|(state, profile)| {
                let state = tree.number(state.to_string());
                let name = self.tail_call_profile_name(profile, context);
                (state, name)
            });
            let cases = cases.collect_vec();
            writer.switch(tree, current_state, cases, |position, tree, writer| {
                self.render_tail_call_profile(tree, writer, &group.profiles[position], context)
            })
        })
    }

    fn tail_call_profile_name(
        &self,
        profile: &TailCallProfile,
        context: &FunctionContext,
    ) -> SmolStr {
        match profile.identity {
            TailCallIdentity::Global(global) => self.global_names[&global].clone(),
            TailCallIdentity::Local(local) => match context.locals.get(&local) {
                Some(LocalBinding::Direct(name)) => name.clone(),
                Some(LocalBinding::Inline(_) | LocalBinding::Lazy(_)) | None => {
                    unreachable!("invariant violated: tail-call profile has no direct local name")
                }
            },
        }
    }

    fn render_tail_call_profile<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        profile: &TailCallProfile,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        self.render_tail_call_parameters(tree, writer, profile, 0, context)
    }

    fn render_tail_call_parameters<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        profile: &TailCallProfile,
        position: usize,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let Some(pattern) = profile.parameters.get(position).copied() else {
            let destination = if profile.effect_step {
                Destination::TailEffectThunkReturn
            } else {
                Destination::Return
            };
            return self.render_expression(tree, writer, profile.body, destination, context);
        };
        let argument = context
            .tail_calls
            .as_ref()
            .expect("invariant violated: tail-call profile has no context")
            .argument_names
            .get(position)
            .expect("invariant violated: tail-call profile exceeds dispatcher arity")
            .clone();
        // A closure created during an iteration must capture that iteration's value rather than
        // the mutable slot that advances the loop.
        let current_argument = context.allocate(format_smolstr!("$currentArgument{position}"));
        let value = tree.identifier(&argument);
        writer.constant(tree, &current_argument, value, false);
        let value = tree.identifier(&current_argument);
        let plan = self.pattern_plan(tree, pattern, value, Some(&current_argument), context)?;
        self.render_pattern_scope(tree, writer, plan, context, |tree, writer, context| {
            self.render_tail_call_parameters(tree, writer, profile, position + 1, context)
        })
    }

    fn render_global_tail_call_wrapper<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        wrapper: TailCallWrapper<'_>,
        exported: bool,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let TailCallWrapper { name, group, profile, state } = wrapper;
        let arguments = profile
            .parameters
            .iter()
            .map(|pattern| self.allocate_pattern_argument(*pattern, context))
            .collect_vec();
        if profile.uncurried {
            return writer.function(name, arguments.clone(), exported, |writer| {
                self.render_tail_call_wrapper_result(
                    tree,
                    writer,
                    TailCallWrapperResult { group, profile, state, arguments: &arguments },
                    context,
                )
            });
        }
        let Some((first, remaining)) = arguments.split_first() else {
            unreachable!("invariant violated: curried tail-call function has no parameters")
        };
        writer.function(name, vec![first.clone()], exported, |writer| {
            self.render_curried_tail_call_wrapper(
                tree,
                writer,
                CurriedTailCallWrapper { group, profile, state, arguments: &arguments, remaining },
                context,
            )
        })
    }

    fn render_local_tail_call_wrapper<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        wrapper: TailCallWrapper<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let TailCallWrapper { name, group, profile, state } = wrapper;
        let arguments = profile
            .parameters
            .iter()
            .map(|pattern| self.allocate_pattern_argument(*pattern, context))
            .collect_vec();
        if profile.uncurried {
            return writer.constant_arrow(name, arguments.clone(), |writer| {
                self.render_tail_call_wrapper_result(
                    tree,
                    writer,
                    TailCallWrapperResult { group, profile, state, arguments: &arguments },
                    context,
                )
            });
        }
        let Some((first, remaining)) = arguments.split_first() else {
            unreachable!("invariant violated: curried tail-call function has no parameters")
        };
        writer.constant_arrow(name, vec![first.clone()], |writer| {
            self.render_curried_tail_call_wrapper(
                tree,
                writer,
                CurriedTailCallWrapper { group, profile, state, arguments: &arguments, remaining },
                context,
            )
        })
    }

    fn render_curried_tail_call_wrapper<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        wrapper: CurriedTailCallWrapper<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let CurriedTailCallWrapper { group, profile, state, arguments, remaining } = wrapper;
        let Some((argument, remaining)) = remaining.split_first() else {
            return self.render_tail_call_wrapper_result(
                tree,
                writer,
                TailCallWrapperResult { group, profile, state, arguments },
                context,
            );
        };
        writer.return_arrow(vec![argument.clone()], |writer| {
            self.render_curried_tail_call_wrapper(
                tree,
                writer,
                CurriedTailCallWrapper { group, profile, state, arguments, remaining },
                context,
            )
        })
    }

    fn render_tail_call_wrapper_result<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        wrapper: TailCallWrapperResult<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let TailCallWrapperResult { group, profile, state, arguments } = wrapper;
        if group.is_singleton() && !profile.effect_step {
            return self.render_singleton_tail_call_loop(tree, writer, group, arguments, context);
        }
        if group.is_singleton() {
            self.render_singleton_effect_dispatcher(tree, writer, group, context)?;
        }

        let dispatcher = tree.identifier(&group.dispatcher_name);
        let state = tree.number(state.to_string());
        let mut values = vec![state];
        values.extend(arguments.iter().map(|argument| tree.identifier(argument)));
        for _ in arguments.len()..group.maximum_arity {
            values.push(tree.null());
        }
        let initial = tree.call(dispatcher, values);
        if !profile.effect_step {
            writer.return_expression(tree, initial);
            return Ok(());
        }

        let initial_name = context.allocate("$initialStep");
        writer.constant(tree, &initial_name, initial, false);
        writer.return_arrow(vec![], |writer| {
            self.render_tail_effect_loop(tree, writer, group, &initial_name, context)
        })
    }

    fn render_singleton_tail_call_loop<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        group: &TailCallGroup,
        arguments: &[SmolStr],
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        // Curried outer arguments can be captured by a reusable partial application. Copy every
        // argument before iterating so one invocation cannot mutate the next invocation's input.
        let mut argument_names = Vec::with_capacity(arguments.len());
        for (position, argument) in arguments.iter().enumerate() {
            let name = context.allocate(format_smolstr!("$argument{position}"));
            let value = tree.identifier(argument);
            writer.mutable_value(tree, &name, value);
            argument_names.push(name);
        }
        let tail_calls = TailCallContext::singleton(group, argument_names.into());
        let outer_tail_calls = context.tail_calls.replace(tail_calls);
        let result = self.render_tail_call_dispatcher(tree, writer, group, context);
        context.tail_calls = outer_tail_calls;
        result
    }

    fn render_singleton_effect_dispatcher<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        group: &TailCallGroup,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let state_name = context.allocate("$state");
        let argument_names = (0..group.maximum_arity)
            .map(|position| context.allocate(format_smolstr!("$argument{position}")))
            .collect::<Rc<[_]>>();
        let tail_calls =
            TailCallContext::new(group, state_name.clone(), Rc::clone(&argument_names));
        let mut dispatcher_parameters = vec![state_name];
        dispatcher_parameters.extend(argument_names.iter().cloned());
        let outer_tail_calls = context.tail_calls.replace(tail_calls);
        writer.constant_arrow(&group.dispatcher_name, dispatcher_parameters, |writer| {
            self.render_tail_call_dispatcher(tree, writer, group, context)
        })?;
        context.tail_calls = outer_tail_calls;
        Ok(())
    }

    fn render_tail_effect_loop<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        group: &TailCallGroup,
        initial_name: &str,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        // A step returns `[false, value]` when complete or `[true, state, ...arguments]` for a
        // tail call. Constructing the next step only after the current action has run preserves
        // Effect construction timing, while resetting to the captured initial step makes the
        // generated thunk safely reusable.
        let step_name = context.allocate("$step");
        let result_name = context.allocate("$result");
        writer.mutable(&step_name);
        let initial = tree.identifier(initial_name);
        writer.assign(tree, &step_name, initial);
        let condition = tree.boolean(true);
        writer.while_loop(tree, condition, |tree, writer| {
            let step = tree.identifier(&step_name);
            let result = tree.call(step, vec![]);
            writer.constant(tree, &result_name, result, false);
            let result = tree.identifier(&result_name);
            let marker_index = tree.number("0");
            let marker = tree.index(result, marker_index);
            let complete = tree.unary(UnaryOperator::LogicalNot, marker);
            writer.if_block(tree, complete, |tree, writer| {
                let result = tree.identifier(&result_name);
                let value_index = tree.number("1");
                let value = tree.index(result, value_index);
                writer.return_expression(tree, value);
            });

            let dispatcher = tree.identifier(&group.dispatcher_name);
            let values = (0..=group.maximum_arity).map(|position| {
                let result = tree.identifier(&result_name);
                let index = tree.number((position + 1).to_string());
                tree.index(result, index)
            });
            let values = values.collect_vec();
            let next_step = tree.call(dispatcher, values);
            writer.assign(tree, &step_name, next_step);
            Ok(())
        })
    }
}

fn render_named_function(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    name: &str,
    expression: FunctionalExpressionId,
    exported: bool,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    match &generator.module.storage[expression].kind {
        ExpressionKind::Abstraction { parameters, body } => {
            let (argument, parameter) = generator.first_argument(parameters, context);
            writer.function(name, vec![argument.clone()], exported, |writer| {
                if let Some(parameter) = parameter {
                    generator.render_curried_parameter(
                        tree,
                        writer,
                        CurriedParameter {
                            pattern: parameter,
                            argument: &argument,
                            remaining: &parameters[1..],
                            body: *body,
                        },
                        context,
                    )
                } else {
                    generator.render_expression(tree, writer, *body, Destination::Return, context)
                }
            })
        }
        ExpressionKind::UncurriedAbstraction { parameters, body } => {
            let arguments = parameters
                .iter()
                .map(|pattern| generator.allocate_pattern_argument(*pattern, context))
                .collect_vec();
            writer.function(name, arguments.clone(), exported, |writer| {
                generator.render_uncurried_parameters(
                    tree,
                    writer,
                    UncurriedParameters {
                        patterns: parameters,
                        arguments: &arguments,
                        position: 0,
                        body: *body,
                    },
                    context,
                )
            })
        }
        _ => unreachable!("invariant violated: named JavaScript function is not an abstraction"),
    }
}

impl Generator<'_> {
    fn first_argument(
        &self,
        parameters: &[PatternId],
        context: &mut FunctionContext,
    ) -> (SmolStr, Option<PatternId>) {
        match parameters.first().copied() {
            Some(pattern) => (self.allocate_pattern_argument(pattern, context), Some(pattern)),
            None => (SmolStr::new_static(""), None),
        }
    }

    fn render_curried_parameter<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        parameter: CurriedParameter<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let CurriedParameter { pattern, argument, remaining, body } = parameter;
        let value = tree.identifier(argument);
        let plan = self.pattern_plan(tree, pattern, value, Some(argument), context)?;
        self.render_pattern_scope(tree, writer, plan, context, |tree, writer, context| {
            let Some((pattern, remaining)) = remaining.split_first() else {
                return self.render_expression(tree, writer, body, Destination::Return, context);
            };
            let argument = self.allocate_pattern_argument(*pattern, context);
            writer.return_arrow(vec![argument.clone()], |writer| {
                self.render_curried_parameter(
                    tree,
                    writer,
                    CurriedParameter { pattern: *pattern, argument: &argument, remaining, body },
                    context,
                )
            })
        })
    }

    fn render_uncurried_parameters<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        parameters: UncurriedParameters<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let UncurriedParameters { patterns, arguments, position, body } = parameters;
        let Some(pattern) = patterns.get(position).copied() else {
            return self.render_expression(tree, writer, body, Destination::Return, context);
        };
        let argument = &arguments[position];
        let value = tree.identifier(argument);
        let plan = self.pattern_plan(tree, pattern, value, Some(argument), context)?;
        self.render_pattern_scope(tree, writer, plan, context, |tree, writer, context| {
            self.render_uncurried_parameters(
                tree,
                writer,
                UncurriedParameters { patterns, arguments, position: position + 1, body },
                context,
            )
        })
    }
}

fn render_foreign_declarations(renderer: &mut ModuleRenderer<'_, '_, '_>) {
    let ModuleRenderer { generator, tree, writer } = renderer;
    let Some(foreign_import) = &generator.foreign_import else {
        return;
    };
    let mut rendered = false;
    for declaration in generator.module.declarations.iter() {
        if !matches!(declaration.kind, DeclarationKind::Foreign) {
            continue;
        }
        let name = generator.global_name(declaration.global.id);
        let object = tree.identifier(&foreign_import.namespace);
        let index = tree.string(declaration.global.item_name.as_str());
        let access = tree.index(object, index);
        let exported = generator.declaration_is_inline_exported(declaration);
        writer.constant(tree, name, access, exported);
        rendered = true;
    }
    if rendered {
        writer.blank();
    }
}

fn render_lazy_initializers(renderer: &mut ModuleRenderer<'_, '_, '_>) -> ModuleResult<()> {
    let generator = renderer.generator;
    let Some(runtime) = &generator.runtime_namespace else {
        return Ok(());
    };
    for declaration in generator.module.declarations.iter() {
        let Some(lazy_name) = generator.lazy_global_names.get(&declaration.global.id) else {
            continue;
        };
        let DeclarationKind::Value(expression) = declaration.kind else {
            unreachable!("invariant violated: lazy JavaScript declaration is not a value")
        };
        let name = renderer.tree.string(declaration.global.item_name.as_str());
        let runtime = renderer.tree.identifier(runtime);
        let binding = renderer.tree.member(runtime, "binding");
        let binding = renderer.tree.expression(binding);
        let name = renderer.tree.expression(name);
        let mut context = FunctionContext::new(&generator.reserved_module_names);
        renderer.writer.binding_call(
            BindingCallTarget::Constant(lazy_name),
            binding,
            name,
            |writer| {
                generator.render_expression(
                    renderer.tree,
                    writer,
                    expression,
                    Destination::Return,
                    &mut context,
                )
            },
        )?;
        renderer.writer.blank();
    }
    Ok(())
}

fn render_value_declarations(
    renderer: &mut ModuleRenderer<'_, '_, '_>,
) -> ModuleResult<Vec<GlobalId>> {
    let generator = renderer.generator;
    let mut rendered = false;
    let mut previous_was_generated = false;
    let mut initializer_cycle = vec![];
    for (declaration, cyclic) in sorted_value_declarations(generator) {
        let DeclarationKind::Value(expression) = declaration.kind else {
            unreachable!("invariant violated: sorted JavaScript declaration is not a value")
        };
        if is_abstraction(&generator.module.storage[expression].kind) {
            continue;
        }

        let generated = matches!(declaration.global.id, GlobalId::Generated(_, _));
        if rendered && (!previous_was_generated || !generated) {
            renderer.writer.blank();
        }

        let name = generator.global_name(declaration.global.id);
        let exported = generator.declaration_is_inline_exported(declaration);

        if cyclic {
            initializer_cycle.push(declaration.global.id);
            renderer.writer.constant_iife(name, exported, |writer| {
                writer.throw_error(INITIALIZER_CYCLE_MESSAGE);
            });
            rendered = true;
            previous_was_generated = generated;
            continue;
        }

        if let Some(lazy_name) = generator.lazy_global_names.get(&declaration.global.id) {
            let lazy = renderer.tree.identifier(lazy_name);
            let value = renderer.tree.call(lazy, vec![]);
            renderer.writer.constant(renderer.tree, name, value, exported);
            rendered = true;
            previous_was_generated = generated;
            continue;
        }

        let mut context = FunctionContext::new(&generator.reserved_module_names);
        if let Some(value) =
            generator.try_inline_expression(renderer.tree, expression, &mut context)?
        {
            renderer.writer.constant(renderer.tree, name, value, exported);
        } else {
            renderer.writer.constant_iife(name, exported, |writer| {
                generator.render_expression(
                    renderer.tree,
                    writer,
                    expression,
                    Destination::Return,
                    &mut context,
                )
            })?;
        }

        rendered = true;
        previous_was_generated = generated;
    }
    if rendered {
        renderer.writer.blank();
    }
    Ok(initializer_cycle)
}

impl Generator<'_> {
    fn render_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        if matches!(
            destination,
            Destination::Return
                | Destination::TailEffectThunkReturn
                | Destination::EffectTailEffectReturn
        ) && let Some(tail_call) = context
            .tail_calls
            .as_ref()
            .and_then(|tail_calls| tail_calls.call(self.module, expression))
        {
            return self.render_tail_call(tree, writer, tail_call, destination, context);
        }

        match &self.module.storage[expression].kind {
            ExpressionKind::Error => {
                writer.throw_error(SOURCE_ERROR_MESSAGE);
                Ok(())
            }
            ExpressionKind::IfThenElse { condition, then, else_ } => {
                let condition = self.expression_value(tree, writer, *condition, context)?;
                writer.if_else_with_state(
                    tree,
                    condition,
                    context,
                    |tree, writer, context| {
                        self.render_expression(tree, writer, *then, destination, context)
                    },
                    |tree, writer, context| {
                        self.render_expression(tree, writer, *else_, destination, context)
                    },
                )
            }
            ExpressionKind::Case { scrutinees, alternatives } => {
                let mut renderer = self.renderer(tree, writer, context);
                render_case(&mut renderer, scrutinees, alternatives, destination)
            }
            ExpressionKind::Guarded { alternatives } => {
                let mut renderer = self.renderer(tree, writer, context);
                render_guarded(&mut renderer, alternatives, destination)
            }
            ExpressionKind::Let { recursive, bindings, body } => {
                let mut renderer = self.renderer(tree, writer, context);
                render_let(&mut renderer, *recursive, bindings)?;
                self.render_expression(tree, writer, *body, destination, context)
            }
            ExpressionKind::LetPattern { pattern, value, body } => {
                let source = *value;
                let value = self.rendered_expression(tree, writer, source, context)?;
                let value = self.materialize_pattern_value(tree, writer, source, value, context);
                let plan = self.pattern_plan(tree, *pattern, value, None, context)?;
                self.render_pattern_scope(tree, writer, plan, context, |tree, writer, context| {
                    self.render_expression(tree, writer, *body, destination, context)
                })
            }
            ExpressionKind::Effect { effect } if !destination.is_effect() => self
                .render_effect_expression_destination(tree, writer, effect, destination, context),
            _ => {
                if matches!(destination, Destination::TailEffectThunkReturn) {
                    return self
                        .render_tail_effect_thunk_destination(tree, writer, expression, context);
                }
                if destination.is_effect() {
                    return self.render_effect_destination(
                        tree,
                        writer,
                        expression,
                        destination.value(),
                        context,
                    );
                }
                let value = self.expression_value(tree, writer, expression, context)?;
                self.render_destination(tree, writer, value, destination);
                Ok(())
            }
        }
    }

    fn render_destination<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        value: ExpressionId,
        destination: Destination<'_>,
    ) {
        match destination {
            Destination::Return => writer.return_expression(tree, value),
            Destination::TailEffectReturn => {
                let complete = tree.boolean(false);
                let result = tree.array(vec![complete, value]);
                writer.return_expression(tree, result);
            }
            Destination::Assign(name) => {
                writer.assign(tree, name, value);
            }
            Destination::AssignAndBreak { name, label } => {
                writer.assign(tree, name, value);
                writer.break_label(label);
            }
            Destination::TailEffectThunkReturn
            | Destination::EffectReturn
            | Destination::EffectTailEffectReturn
            | Destination::EffectAssign(_)
            | Destination::EffectAssignAndBreak { .. } => {
                unreachable!("invariant violated: effect destination was not rendered directly")
            }
        }
    }

    fn render_effect_destination<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        if let ExpressionKind::Effect { effect } = &self.module.storage[expression].kind {
            let mut renderer = self.renderer(tree, writer, context);
            let effect = capture_effect(&mut renderer, effect)?;
            return execute_effect(&mut renderer, effect, destination);
        }

        let effect = self.expression_value(tree, writer, expression, context)?;
        let value = tree.call(effect, vec![]);
        self.render_destination(tree, writer, value, destination);
        Ok(())
    }

    fn render_effect_expression_destination<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        effect: &EffectExpression,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let mut renderer = self.renderer(tree, writer, context);
        let effect = capture_effect(&mut renderer, effect)?;
        let break_label = match destination {
            Destination::Return => {
                writer.return_arrow(vec![], |writer| {
                    let mut renderer = self.renderer(tree, writer, context);
                    execute_effect(&mut renderer, effect, Destination::Return)
                })?;
                None
            }
            Destination::TailEffectThunkReturn => {
                writer.return_arrow(vec![], |writer| {
                    let mut renderer = self.renderer(tree, writer, context);
                    execute_effect(&mut renderer, effect, Destination::TailEffectReturn)
                })?;
                None
            }
            Destination::Assign(name) => {
                writer.assign_arrow(name, vec![], |writer| {
                    let mut renderer = self.renderer(tree, writer, context);
                    execute_effect(&mut renderer, effect, Destination::Return)
                })?;
                None
            }
            Destination::AssignAndBreak { name, label } => {
                writer.assign_arrow(name, vec![], |writer| {
                    let mut renderer = self.renderer(tree, writer, context);
                    execute_effect(&mut renderer, effect, Destination::Return)
                })?;
                Some(label)
            }
            Destination::TailEffectReturn
            | Destination::EffectReturn
            | Destination::EffectTailEffectReturn
            | Destination::EffectAssign(_)
            | Destination::EffectAssignAndBreak { .. } => {
                unreachable!("invariant violated: effect destination returned an effect thunk")
            }
        };
        if let Some(label) = break_label {
            writer.break_label(label);
        }
        Ok(())
    }

    fn render_tail_effect_thunk_destination<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let mut renderer = self.renderer(tree, writer, context);
        let effect = capture_effect_value(&mut renderer, expression, "$effect")?;
        writer.return_arrow(vec![], |writer| {
            let value = tree.call(effect, vec![]);
            self.render_destination(tree, writer, value, Destination::TailEffectReturn);
            Ok(())
        })
    }

    fn render_tail_call<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        tail_call: tail_call::TailCall,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let target = tail_call.target;
        let tail_calls = context
            .tail_calls
            .as_ref()
            .expect("invariant violated: rendered tail call has no context");
        let state_name = tail_calls.state_name.clone();
        let argument_names = Rc::clone(&tail_calls.argument_names);

        if matches!(destination, Destination::EffectTailEffectReturn) {
            let marker = tree.boolean(true);
            let state = tree.number(target.state.to_string());
            let mut values = vec![marker, state];
            for argument in tail_call.arguments {
                let value = self.expression_value(tree, writer, argument, context)?;
                let value = if tree.expression_is_atomic(&value) {
                    value
                } else {
                    self.materialize_value(tree, writer, value, "$tailArgument", context)
                };
                values.push(value);
            }
            for _ in target.arity..argument_names.len() {
                values.push(tree.null());
            }
            let result = tree.array(values);
            writer.return_expression(tree, result);
            return Ok(());
        }

        for (name, argument) in argument_names.iter().zip(tail_call.arguments) {
            let value = self.expression_value(tree, writer, argument, context)?;
            if tree.expression_identifier(&value) == Some(name) {
                continue;
            }
            writer.assign(tree, name, value);
        }
        for name in argument_names.iter().skip(target.arity) {
            let null = tree.null();
            writer.assign(tree, name, null);
        }
        if let Some(state_name) = state_name {
            let state = tree.number(target.state.to_string());
            writer.assign(tree, &state_name, state);
        }
        writer.continue_loop();
        Ok(())
    }

    fn expression_value<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<ExpressionId> {
        let expression = self.rendered_expression(tree, writer, expression, context)?;
        Ok(expression.value)
    }

    fn rendered_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<RenderedExpression> {
        if let Some(expression) = self.try_inline_expression(tree, expression, context)? {
            return Ok(RenderedExpression { value: expression, pending_evaluation: true });
        }
        self.render_non_inline_expression(tree, writer, expression, context)
    }

    fn render_non_inline_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<RenderedExpression> {
        match &self.module.storage[expression].kind {
            ExpressionKind::Array { elements } => {
                let mut values = Vec::with_capacity(elements.len());
                for element in elements.iter() {
                    let value =
                        if let Some(value) = self.try_inline_expression(tree, *element, context)? {
                            RenderedExpression { value, pending_evaluation: true }
                        } else {
                            if self.expression_rendering_is_eager(*element, context) {
                                self.materialize_rendered_expressions(
                                    tree,
                                    writer,
                                    &mut values,
                                    "$element",
                                    context,
                                );
                            }
                            self.render_non_inline_expression(tree, writer, *element, context)?
                        };
                    values.push(value);
                }
                let values = values.into_iter().map(|value| value.value).collect_vec();
                let value = tree.array(values);
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Record { fields } => {
                let mut rendered_fields = Vec::with_capacity(fields.len());
                for field in fields.iter() {
                    let value = if let Some(value) =
                        self.try_inline_expression(tree, field.expression, context)?
                    {
                        RenderedExpression { value, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(field.expression, context) {
                            let values = rendered_fields.iter_mut().map(|(_, value)| value);
                            for value in values {
                                self.materialize_rendered_expression(
                                    tree, writer, value, "$field", context,
                                );
                            }
                        }
                        self.render_non_inline_expression(tree, writer, field.expression, context)?
                    };
                    rendered_fields.push((field.field.name.clone(), value));
                }
                let properties = rendered_fields
                    .into_iter()
                    .map(|(name, value)| ObjectProperty::Field { name, value: value.value });
                let value = tree.object(properties.collect_vec());
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::RecordUpdate { record, updates } => {
                let value =
                    self.record_update_expression(tree, writer, *record, updates, context)?;
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Project { record, field } => {
                let record = self.rendered_expression(tree, writer, *record, context)?;
                let value = tree.member(record.value, field.name.as_str());
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Unary { operator, value } => {
                let value = self.rendered_expression(tree, writer, *value, context)?;
                let value = unary_expression(tree, *operator, value.value);
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Binary { operator, left, right } => {
                let mut left = self.rendered_expression(tree, writer, *left, context)?;
                let right =
                    if let Some(value) = self.try_inline_expression(tree, *right, context)? {
                        RenderedExpression { value, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(*right, context) {
                            self.materialize_rendered_expression(
                                tree, writer, &mut left, "$left", context,
                            );
                        }
                        self.render_non_inline_expression(tree, writer, *right, context)?
                    };
                let value = binary_expression(tree, *operator, left.value, right.value);
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::Abstraction { parameters, body } => {
                let name = context.allocate("$closure");
                self.render_abstraction_binding(
                    tree,
                    writer,
                    AbstractionBinding { name: &name, parameters, body: *body, uncurried: false },
                    context,
                )?;
                let value = tree.identifier(name);
                Ok(RenderedExpression { value, pending_evaluation: false })
            }
            ExpressionKind::UncurriedAbstraction { parameters, body } => {
                let name = context.allocate("$closure");
                self.render_abstraction_binding(
                    tree,
                    writer,
                    AbstractionBinding { name: &name, parameters, body: *body, uncurried: true },
                    context,
                )?;
                let value = tree.identifier(name);
                Ok(RenderedExpression { value, pending_evaluation: false })
            }
            ExpressionKind::Application { function, arguments, synthetic } => {
                let mut function = self.rendered_expression(tree, writer, *function, context)?;
                if *synthetic {
                    tree.clear_call_purity(&function.value);
                }
                if arguments.is_empty() {
                    let value = if *synthetic {
                        tree.pure_call(function.value, vec![])
                    } else {
                        tree.call(function.value, vec![])
                    };
                    return Ok(RenderedExpression { value, pending_evaluation: true });
                }
                for (index, argument) in arguments.iter().enumerate() {
                    let argument = if let Some(value) =
                        self.try_inline_expression(tree, *argument, context)?
                    {
                        RenderedExpression { value, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(*argument, context) {
                            self.materialize_rendered_expression(
                                tree,
                                writer,
                                &mut function,
                                "$function",
                                context,
                            );
                        }
                        self.render_non_inline_expression(tree, writer, *argument, context)?
                    };
                    let outermost = index + 1 == arguments.len();
                    let value = if *synthetic && outermost {
                        tree.pure_call(function.value, vec![argument.value])
                    } else {
                        tree.call(function.value, vec![argument.value])
                    };
                    function = RenderedExpression { value, pending_evaluation: true };
                }
                Ok(function)
            }
            ExpressionKind::UncurriedApplication { function, arguments, synthetic } => {
                let mut function = self.rendered_expression(tree, writer, *function, context)?;
                if *synthetic {
                    tree.clear_call_purity(&function.value);
                }
                let mut values = Vec::with_capacity(arguments.len());
                for argument in arguments.iter() {
                    let value = if let Some(value) =
                        self.try_inline_expression(tree, *argument, context)?
                    {
                        RenderedExpression { value, pending_evaluation: true }
                    } else {
                        if self.expression_rendering_is_eager(*argument, context) {
                            self.materialize_rendered_expression(
                                tree,
                                writer,
                                &mut function,
                                "$function",
                                context,
                            );
                            self.materialize_rendered_expressions(
                                tree,
                                writer,
                                &mut values,
                                "$argument",
                                context,
                            );
                        }
                        self.render_non_inline_expression(tree, writer, *argument, context)?
                    };
                    values.push(value);
                }
                let values = values.into_iter().map(|value| value.value).collect_vec();
                let value = if *synthetic {
                    tree.pure_call(function.value, values)
                } else {
                    tree.call(function.value, values)
                };
                Ok(RenderedExpression { value, pending_evaluation: true })
            }
            ExpressionKind::StyleX(stylex) => {
                self.render_stylex_expression(tree, writer, stylex, context)
            }
            ExpressionKind::Effect { effect } => {
                let mut renderer = self.renderer(tree, writer, context);
                let value = effect_expression(&mut renderer, effect)?;
                Ok(RenderedExpression { value, pending_evaluation: false })
            }
            ExpressionKind::Let { recursive: false, bindings, body } => {
                let name = context.allocate("$result");
                let mut renderer = self.renderer(tree, writer, context);
                render_let(&mut renderer, false, bindings)?;
                let value = self.expression_value(tree, writer, *body, context)?;
                writer.constant(tree, &name, value, false);
                let value = tree.identifier(name);
                Ok(RenderedExpression { value, pending_evaluation: false })
            }
            ExpressionKind::Error
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. } => {
                let name = context.allocate("$result");
                writer.mutable(&name);
                self.render_expression(
                    tree,
                    writer,
                    expression,
                    Destination::Assign(&name),
                    context,
                )?;
                let value = tree.identifier(name);
                Ok(RenderedExpression { value, pending_evaluation: false })
            }
            ExpressionKind::Literal { .. }
            | ExpressionKind::Constructor { .. }
            | ExpressionKind::Global { .. }
            | ExpressionKind::Local { .. }
            | ExpressionKind::SynthesizedEvidence { .. }
            | ExpressionKind::TrivialEvidence => {
                unreachable!("invariant violated: atomic functional expression was not inlineable")
            }
        }
    }

    fn expression_rendering_is_eager(
        &self,
        expression: FunctionalExpressionId,
        context: &FunctionContext,
    ) -> bool {
        match &self.module.storage[expression].kind {
            ExpressionKind::Literal { .. }
            | ExpressionKind::Constructor { .. }
            | ExpressionKind::Global { .. }
            | ExpressionKind::Local { .. }
            | ExpressionKind::Abstraction { .. }
            | ExpressionKind::UncurriedAbstraction { .. }
            | ExpressionKind::SynthesizedEvidence { .. }
            | ExpressionKind::TrivialEvidence => false,
            ExpressionKind::Array { elements } => {
                elements.iter().any(|element| self.expression_rendering_is_eager(*element, context))
            }
            ExpressionKind::Record { fields } => fields
                .iter()
                .any(|field| self.expression_rendering_is_eager(field.expression, context)),
            ExpressionKind::RecordUpdate { record, updates } => {
                self.expression_rendering_is_eager(*record, context)
                    || record_updates_reuse_source(updates)
                        && !self.functional_expression_is_reusable(*record, context)
                    || self.record_updates_rendering_is_eager(updates, context)
            }
            ExpressionKind::Project { record, .. }
            | ExpressionKind::Unary { value: record, .. } => {
                self.expression_rendering_is_eager(*record, context)
            }
            ExpressionKind::Binary { left, right, .. } => {
                self.expression_rendering_is_eager(*left, context)
                    || self.expression_rendering_is_eager(*right, context)
            }
            ExpressionKind::Application { function, arguments, .. }
            | ExpressionKind::UncurriedApplication { function, arguments, .. } => {
                self.expression_rendering_is_eager(*function, context)
                    || arguments
                        .iter()
                        .any(|argument| self.expression_rendering_is_eager(*argument, context))
            }
            ExpressionKind::StyleX(stylex) => stylex
                .try_for_each_child(|child| {
                    if self.expression_rendering_is_eager(child, context) {
                        Err(())
                    } else {
                        Ok(())
                    }
                })
                .is_err(),
            ExpressionKind::Error
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. }
            | ExpressionKind::Effect { .. } => true,
        }
    }

    fn functional_expression_is_reusable(
        &self,
        expression: FunctionalExpressionId,
        context: &FunctionContext,
    ) -> bool {
        match &self.module.storage[expression].kind {
            ExpressionKind::Literal { .. } => true,
            ExpressionKind::Constructor { global } | ExpressionKind::Global { global } => {
                global_file(global.id) == self.module.file_id
                    && !self.lazy_global_names.contains_key(&global.id)
            }
            ExpressionKind::Local { parameter } => {
                matches!(
                    context.locals.get(&parameter.id),
                    Some(LocalBinding::Direct(_) | LocalBinding::Inline(_))
                )
            }
            ExpressionKind::Error
            | ExpressionKind::Array { .. }
            | ExpressionKind::Record { .. }
            | ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::Project { .. }
            | ExpressionKind::Unary { .. }
            | ExpressionKind::Binary { .. }
            | ExpressionKind::Abstraction { .. }
            | ExpressionKind::UncurriedAbstraction { .. }
            | ExpressionKind::Application { .. }
            | ExpressionKind::UncurriedApplication { .. }
            | ExpressionKind::StyleX(_)
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. }
            | ExpressionKind::Effect { .. }
            | ExpressionKind::SynthesizedEvidence { .. }
            | ExpressionKind::TrivialEvidence => false,
        }
    }

    fn record_updates_rendering_is_eager(
        &self,
        updates: &[RecordUpdate],
        context: &FunctionContext,
    ) -> bool {
        updates.iter().any(|update| self.record_update_rendering_is_eager(update, context))
    }

    fn record_update_rendering_is_eager(
        &self,
        update: &RecordUpdate,
        context: &FunctionContext,
    ) -> bool {
        match update {
            RecordUpdate::Leaf { expression, .. } => {
                self.expression_rendering_is_eager(*expression, context)
            }
            RecordUpdate::Branch { updates, .. } => {
                self.record_updates_rendering_is_eager(updates, context)
            }
        }
    }

    fn inline_expression(
        &self,
        tree: &mut Tree,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<Option<ExpressionId>> {
        let expression = match &self.module.storage[expression].kind {
            ExpressionKind::Literal { literal } => {
                literal_expression(tree, literal, self.module.file_id)?
            }
            ExpressionKind::Array { elements } => {
                let Some(elements) = elements
                    .iter()
                    .map(|element| self.inline_expression(tree, *element, context))
                    .collect::<ModuleResult<Option<Vec<_>>>>()?
                else {
                    return Ok(None);
                };
                tree.array(elements)
            }
            ExpressionKind::Record { fields } => {
                let mut properties = Vec::with_capacity(fields.len());
                for field in fields.iter() {
                    let Some(value) = self.inline_expression(tree, field.expression, context)?
                    else {
                        return Ok(None);
                    };
                    properties
                        .push(ObjectProperty::Field { name: field.field.name.clone(), value });
                }
                tree.object(properties)
            }
            ExpressionKind::Project { record, field } => {
                let Some(record) = self.inline_expression(tree, *record, context)? else {
                    return Ok(None);
                };
                tree.member(record, field.name.as_str())
            }
            ExpressionKind::Unary { operator, value } => {
                let Some(value) = self.inline_expression(tree, *value, context)? else {
                    return Ok(None);
                };
                unary_expression(tree, *operator, value)
            }
            ExpressionKind::Binary { operator, left, right } => {
                let Some(left) = self.inline_expression(tree, *left, context)? else {
                    return Ok(None);
                };
                let Some(right) = self.inline_expression(tree, *right, context)? else {
                    return Ok(None);
                };
                binary_expression(tree, *operator, left, right)
            }
            ExpressionKind::Constructor { global } => self.global_expression(tree, global)?,
            ExpressionKind::Global { global } => self.global_expression(tree, global)?,
            ExpressionKind::Local { parameter } => {
                local_expression(self, tree, parameter, context)?
            }
            ExpressionKind::Abstraction { parameters, body } => {
                let Some(expression) =
                    self.inline_abstraction(tree, parameters, *body, false, context)?
                else {
                    return Ok(None);
                };
                expression
            }
            ExpressionKind::UncurriedAbstraction { parameters, body } => {
                let Some(expression) =
                    self.inline_abstraction(tree, parameters, *body, true, context)?
                else {
                    return Ok(None);
                };
                expression
            }
            ExpressionKind::Application { function, arguments, synthetic } => {
                let Some(function) = self.inline_expression(tree, *function, context)? else {
                    return Ok(None);
                };
                if *synthetic {
                    tree.clear_call_purity(&function);
                }
                let Some(arguments) = arguments
                    .iter()
                    .map(|argument| self.inline_expression(tree, *argument, context))
                    .collect::<ModuleResult<Option<Vec<_>>>>()?
                else {
                    return Ok(None);
                };
                curried_call_expression(tree, function, arguments, *synthetic)
            }
            ExpressionKind::UncurriedApplication { function, arguments, synthetic } => {
                let Some(function) = self.inline_expression(tree, *function, context)? else {
                    return Ok(None);
                };
                if *synthetic {
                    tree.clear_call_purity(&function);
                }
                let Some(arguments) = arguments
                    .iter()
                    .map(|argument| self.inline_expression(tree, *argument, context))
                    .collect::<ModuleResult<Option<Vec<_>>>>()?
                else {
                    return Ok(None);
                };
                if *synthetic {
                    tree.pure_call(function, arguments)
                } else {
                    tree.call(function, arguments)
                }
            }
            ExpressionKind::StyleX(stylex) => {
                let Some(expression) = self.inline_stylex_expression(tree, stylex, context)? else {
                    return Ok(None);
                };
                expression
            }
            ExpressionKind::SynthesizedEvidence { evidence } => {
                synthesized_evidence_expression(tree, evidence)
            }
            ExpressionKind::TrivialEvidence => tree.object(vec![]),
            ExpressionKind::Error
            | ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. }
            | ExpressionKind::Effect { .. } => return Ok(None),
        };
        Ok(Some(expression))
    }

    fn try_inline_expression(
        &self,
        tree: &mut Tree,
        expression: FunctionalExpressionId,
        context: &mut FunctionContext,
    ) -> ModuleResult<Option<ExpressionId>> {
        if !self.expression_can_inline(expression) {
            return Ok(None);
        }
        let expression = self
            .inline_expression(tree, expression, context)?
            .expect("invariant violated: inline eligibility did not match expression rendering");
        Ok(Some(expression))
    }

    fn expression_can_inline(&self, expression: FunctionalExpressionId) -> bool {
        match &self.module.storage[expression].kind {
            ExpressionKind::Literal { .. }
            | ExpressionKind::Constructor { .. }
            | ExpressionKind::Global { .. }
            | ExpressionKind::Local { .. }
            | ExpressionKind::SynthesizedEvidence { .. }
            | ExpressionKind::TrivialEvidence => true,
            ExpressionKind::Array { elements } => {
                elements.iter().all(|element| self.expression_can_inline(*element))
            }
            ExpressionKind::Record { fields } => {
                fields.iter().all(|field| self.expression_can_inline(field.expression))
            }
            ExpressionKind::Project { record, .. }
            | ExpressionKind::Unary { value: record, .. } => self.expression_can_inline(*record),
            ExpressionKind::Binary { left, right, .. } => {
                self.expression_can_inline(*left) && self.expression_can_inline(*right)
            }
            ExpressionKind::Abstraction { parameters, body }
            | ExpressionKind::UncurriedAbstraction { parameters, body } => {
                parameters.iter().all(|pattern| self.pattern_can_inline(*pattern))
                    && self.expression_can_inline(*body)
            }
            ExpressionKind::Application { function, arguments, .. }
            | ExpressionKind::UncurriedApplication { function, arguments, .. } => {
                self.expression_can_inline(*function)
                    && arguments.iter().all(|argument| self.expression_can_inline(*argument))
            }
            ExpressionKind::StyleX(stylex) => stylex
                .try_for_each_child(|child| {
                    if self.expression_can_inline(child) { Ok(()) } else { Err(()) }
                })
                .is_ok(),
            ExpressionKind::Error
            | ExpressionKind::RecordUpdate { .. }
            | ExpressionKind::IfThenElse { .. }
            | ExpressionKind::Case { .. }
            | ExpressionKind::Guarded { .. }
            | ExpressionKind::Let { .. }
            | ExpressionKind::LetPattern { .. }
            | ExpressionKind::Effect { .. } => false,
        }
    }

    fn pattern_can_inline(&self, pattern: PatternId) -> bool {
        match &self.module.storage[pattern].kind {
            PatternKind::Variable(_) | PatternKind::Wildcard => true,
            PatternKind::Named { pattern, .. } => self.pattern_can_inline(*pattern),
            PatternKind::Literal(_)
            | PatternKind::Array(_)
            | PatternKind::Record(_)
            | PatternKind::Constructor { .. } => false,
        }
    }

    fn inline_abstraction(
        &self,
        tree: &mut Tree,
        parameters: &[PatternId],
        body: FunctionalExpressionId,
        uncurried: bool,
        context: &mut FunctionContext,
    ) -> ModuleResult<Option<ExpressionId>> {
        let mut arguments = Vec::with_capacity(parameters.len());
        for pattern in parameters {
            let argument = self.allocate_pattern_argument(*pattern, context);
            if !self.bind_inline_pattern(*pattern, &argument, context) {
                return Ok(None);
            }
            arguments.push(argument);
        }
        let Some(mut body) = self.inline_expression(tree, body, context)? else {
            return Ok(None);
        };
        if uncurried {
            body = tree.arrow(arguments, body);
        } else if arguments.is_empty() {
            body = tree.arrow(vec![], body);
        } else {
            for argument in arguments.into_iter().rev() {
                body = tree.arrow(vec![argument], body);
            }
        }
        Ok(Some(body))
    }

    fn bind_inline_pattern(
        &self,
        pattern: PatternId,
        argument: &str,
        context: &mut FunctionContext,
    ) -> bool {
        match &self.module.storage[pattern].kind {
            PatternKind::Variable(parameter) => {
                context.bind_direct(parameter, SmolStr::new(argument));
                true
            }
            PatternKind::Named { parameter, pattern } => {
                context.bind_direct(parameter, SmolStr::new(argument));
                self.bind_inline_pattern(*pattern, argument, context)
            }
            PatternKind::Wildcard => true,
            PatternKind::Literal(_)
            | PatternKind::Array(_)
            | PatternKind::Record(_)
            | PatternKind::Constructor { .. } => false,
        }
    }

    fn render_abstraction_binding<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        binding: AbstractionBinding<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let AbstractionBinding { name, parameters, body, uncurried } = binding;
        // A recursive call in a nested closure starts a distinct invocation; it cannot continue
        // the loop whose current iteration created that closure.
        let outer_tail_calls = context.tail_calls.take();
        let result = if uncurried {
            let arguments = parameters
                .iter()
                .map(|pattern| self.allocate_pattern_argument(*pattern, context))
                .collect_vec();
            writer.constant_arrow(name, arguments.clone(), |writer| {
                self.render_uncurried_parameters(
                    tree,
                    writer,
                    UncurriedParameters {
                        patterns: parameters,
                        arguments: &arguments,
                        position: 0,
                        body,
                    },
                    context,
                )
            })
        } else {
            let (argument, parameter) = self.first_argument(parameters, context);
            let arguments = if parameter.is_some() { vec![argument.clone()] } else { vec![] };
            writer.constant_arrow(name, arguments, |writer| {
                if let Some(parameter) = parameter {
                    self.render_curried_parameter(
                        tree,
                        writer,
                        CurriedParameter {
                            pattern: parameter,
                            argument: &argument,
                            remaining: &parameters[1..],
                            body,
                        },
                        context,
                    )
                } else {
                    self.render_expression(tree, writer, body, Destination::Return, context)
                }
            })
        };
        context.tail_calls = outer_tail_calls;
        result
    }
}

fn render_let(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    recursive: bool,
    bindings: &[Binding],
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    if recursive
        && bindings
            .iter()
            .all(|binding| is_abstraction(&generator.module.storage[binding.expression].kind))
    {
        let names =
            bindings.iter().map(|binding| context.allocate(&binding.parameter.name)).collect_vec();
        for (binding, name) in bindings.iter().zip(&names) {
            context.bind_direct(&binding.parameter, name.clone());
        }

        let profiles = local_profiles(generator.module, bindings);
        let mut dispatcher_names = profiles.iter().map(|profile| {
            let TailCallIdentity::Local(local) = profile.identity else {
                unreachable!("invariant violated: local tail-call group contains a global")
            };
            let position = bindings
                .iter()
                .position(|binding| binding.parameter.id == local)
                .expect("invariant violated: local tail-call profile has no binding");
            names[position].as_str()
        });
        let dispatcher_suffix = dispatcher_names.join("_");
        let dispatcher_name = context.allocate(format_smolstr!("$tail_{dispatcher_suffix}"));
        let optimized = if let Some(group) =
            tail_call_group(generator.module, profiles, dispatcher_name)
        {
            if !group.is_singleton() {
                let state_name = context.allocate("$state");
                let argument_names = (0..group.maximum_arity)
                    .map(|position| context.allocate(format_smolstr!("$argument{position}")))
                    .collect::<Rc<[_]>>();
                let tail_calls =
                    TailCallContext::new(&group, state_name.clone(), Rc::clone(&argument_names));
                let mut dispatcher_parameters = vec![state_name];
                dispatcher_parameters.extend(argument_names.iter().cloned());
                let outer_tail_calls = context.tail_calls.replace(tail_calls);
                writer.constant_arrow(&group.dispatcher_name, dispatcher_parameters, |writer| {
                    generator.render_tail_call_dispatcher(tree, writer, &group, context)
                })?;
                context.tail_calls = outer_tail_calls;
            }

            for (state, profile) in group.profiles.iter().enumerate() {
                let TailCallIdentity::Local(local) = profile.identity else {
                    unreachable!("invariant violated: local tail-call group contains a global")
                };
                let position = bindings
                    .iter()
                    .position(|binding| binding.parameter.id == local)
                    .expect("invariant violated: local tail-call profile has no binding");
                generator.render_local_tail_call_wrapper(
                    tree,
                    writer,
                    TailCallWrapper { name: &names[position], group: &group, profile, state },
                    context,
                )?;
            }

            let optimized = group.profiles.iter().map(|profile| profile.identity);
            optimized.collect::<FxHashSet<_>>()
        } else {
            FxHashSet::default()
        };

        for (binding, name) in bindings.iter().zip(&names) {
            let identity = TailCallIdentity::Local(binding.parameter.id);
            if optimized.contains(&identity) {
                continue;
            }
            match &generator.module.storage[binding.expression].kind {
                ExpressionKind::Abstraction { parameters, body } => {
                    generator.render_abstraction_binding(
                        tree,
                        writer,
                        AbstractionBinding { name, parameters, body: *body, uncurried: false },
                        context,
                    )?;
                }
                ExpressionKind::UncurriedAbstraction { parameters, body } => {
                    generator.render_abstraction_binding(
                        tree,
                        writer,
                        AbstractionBinding { name, parameters, body: *body, uncurried: true },
                        context,
                    )?;
                }
                _ => {
                    unreachable!("invariant violated: recursive closure group contains a value")
                }
            }
        }
        return Ok(());
    }
    if recursive {
        let mut renderer = generator.renderer(tree, writer, context);
        return render_lazy_let(&mut renderer, bindings);
    }
    for binding in bindings {
        let name = context.allocate(&binding.parameter.name);
        match &generator.module.storage[binding.expression].kind {
            ExpressionKind::Abstraction { parameters, body } => {
                context.bind_direct(&binding.parameter, name.clone());
                generator.render_abstraction_binding(
                    tree,
                    writer,
                    AbstractionBinding { name: &name, parameters, body: *body, uncurried: false },
                    context,
                )?;
            }
            ExpressionKind::UncurriedAbstraction { parameters, body } => {
                context.bind_direct(&binding.parameter, name.clone());
                generator.render_abstraction_binding(
                    tree,
                    writer,
                    AbstractionBinding { name: &name, parameters, body: *body, uncurried: true },
                    context,
                )?;
            }
            _ => {
                let value =
                    generator.expression_value(tree, writer, binding.expression, context)?;
                writer.constant(tree, &name, value, false);
                context.bind_direct(&binding.parameter, name);
            }
        }
    }
    Ok(())
}

fn render_lazy_let(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    bindings: &[Binding],
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    let runtime = generator
        .runtime_namespace
        .as_deref()
        .expect("invariant violated: recursive local values require the JavaScript runtime");
    let mut bindings = bindings.iter().collect_vec();
    bindings.sort_by_key(|binding| binding.source_order);
    let names =
        bindings.iter().map(|binding| context.allocate(&binding.parameter.name)).collect_vec();
    let accessors =
        names.iter().map(|name| context.allocate(format_smolstr!("$lazy_{name}"))).collect_vec();
    for ((binding, name), accessor) in bindings.iter().zip(&names).zip(&accessors) {
        let _ = name;
        context.bind_lazy(&binding.parameter, accessor.clone());
        writer.mutable(accessor);
    }
    for (binding, accessor) in bindings.iter().zip(&accessors) {
        let runtime = tree.identifier(runtime);
        let binding_function = tree.member(runtime, "binding");
        let source_name = tree.string(binding.parameter.name.as_str());
        let binding_function = tree.expression(binding_function);
        let source_name = tree.expression(source_name);
        writer.binding_call(
            BindingCallTarget::Assignment(accessor),
            binding_function,
            source_name,
            |writer| {
                generator.render_expression(
                    tree,
                    writer,
                    binding.expression,
                    Destination::Return,
                    context,
                )
            },
        )?;
    }
    for ((binding, name), accessor) in bindings.iter().zip(&names).zip(&accessors) {
        let accessor_expression = tree.identifier(accessor);
        let value = tree.call(accessor_expression, vec![]);
        writer.constant(tree, name, value, false);
        context.bind_direct(&binding.parameter, name.clone());
    }
    Ok(())
}

fn render_case(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    scrutinees: &[FunctionalExpressionId],
    alternatives: &[CaseAlternative],
    destination: Destination<'_>,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    let mut values = Vec::with_capacity(scrutinees.len());
    for scrutinee in scrutinees {
        let value = generator.rendered_expression(tree, writer, *scrutinee, context)?;
        let value = generator.materialize_pattern_value(tree, writer, *scrutinee, value, context);
        values.push(value);
    }
    let scrutinees = values;
    match destination {
        Destination::Return
        | Destination::TailEffectThunkReturn
        | Destination::TailEffectReturn
        | Destination::EffectReturn
        | Destination::EffectTailEffectReturn
        | Destination::AssignAndBreak { .. }
        | Destination::EffectAssignAndBreak { .. } => generator.render_case_alternatives(
            tree,
            writer,
            &scrutinees,
            alternatives,
            destination,
            context,
        ),
        Destination::Assign(name) => {
            let label = context.allocate("$case");
            writer.labeled_block(&label, |writer| {
                generator.render_case_alternatives(
                    tree,
                    writer,
                    &scrutinees,
                    alternatives,
                    Destination::AssignAndBreak { name, label: &label },
                    context,
                )
            })
        }
        Destination::EffectAssign(name) => {
            let label = context.allocate("$case");
            writer.labeled_block(&label, |writer| {
                generator.render_case_alternatives(
                    tree,
                    writer,
                    &scrutinees,
                    alternatives,
                    Destination::EffectAssignAndBreak { name, label: &label },
                    context,
                )
            })
        }
    }
}

impl Generator<'_> {
    fn render_case_alternatives<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        scrutinees: &[ExpressionId],
        alternatives: &[CaseAlternative],
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        for alternative in alternatives {
            let mut plan = PatternPlan::default();
            for (pattern, value) in alternative.patterns.iter().zip(scrutinees) {
                let value = tree.duplicate(value);
                self.extend_pattern_plan(tree, *pattern, value, None, context, &mut plan)?;
            }
            let condition = combine_conditions(tree, std::mem::take(&mut plan.conditions));
            if let Some(condition) = condition {
                writer.if_block(tree, condition, |tree, writer| {
                    self.render_pattern_bindings(tree, writer, plan.bindings);
                    self.render_case_alternative_expression(
                        tree,
                        writer,
                        alternative.expression,
                        destination,
                        context,
                    )
                })?;
            } else if matches!(
                self.module.storage[alternative.expression].kind,
                ExpressionKind::Guarded { .. }
            ) {
                writer.block(|writer| {
                    self.render_pattern_bindings(tree, writer, plan.bindings);
                    self.render_case_alternative_expression(
                        tree,
                        writer,
                        alternative.expression,
                        destination,
                        context,
                    )
                })?;
            } else {
                self.render_pattern_bindings(tree, writer, plan.bindings);
                self.render_expression(tree, writer, alternative.expression, destination, context)?;
                return Ok(());
            }
        }
        self.render_pattern_failure(writer);
        Ok(())
    }

    fn render_case_alternative_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: FunctionalExpressionId,
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        if let ExpressionKind::Guarded { alternatives } = &self.module.storage[expression].kind {
            self.render_guard_alternatives(tree, writer, alternatives, destination, context)
        } else {
            self.render_expression(tree, writer, expression, destination, context)
        }
    }
}

fn render_guarded(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    alternatives: &[GuardedAlternative],
    destination: Destination<'_>,
) -> ModuleResult<()> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    match destination {
        Destination::Return
        | Destination::TailEffectThunkReturn
        | Destination::TailEffectReturn
        | Destination::EffectReturn
        | Destination::EffectTailEffectReturn
        | Destination::AssignAndBreak { .. }
        | Destination::EffectAssignAndBreak { .. } => {
            generator.render_guard_alternatives(
                tree,
                writer,
                alternatives,
                destination,
                context,
            )?;
            generator.render_pattern_failure(writer);
            Ok(())
        }
        Destination::Assign(name) => {
            let label = context.allocate("$guard");
            writer.labeled_block(&label, |writer| {
                generator.render_guard_alternatives(
                    tree,
                    writer,
                    alternatives,
                    Destination::AssignAndBreak { name, label: &label },
                    context,
                )?;
                generator.render_pattern_failure(writer);
                Ok(())
            })
        }
        Destination::EffectAssign(name) => {
            let label = context.allocate("$guard");
            writer.labeled_block(&label, |writer| {
                generator.render_guard_alternatives(
                    tree,
                    writer,
                    alternatives,
                    Destination::EffectAssignAndBreak { name, label: &label },
                    context,
                )?;
                generator.render_pattern_failure(writer);
                Ok(())
            })
        }
    }
}

impl Generator<'_> {
    fn render_guard_alternatives<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        alternatives: &[GuardedAlternative],
        destination: Destination<'_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        for alternative in alternatives {
            self.render_guards(
                tree,
                writer,
                GuardSequence {
                    guards: &alternative.guards,
                    position: 0,
                    expression: alternative.expression,
                    destination,
                },
                context,
            )?;
        }
        Ok(())
    }

    fn render_guards<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        sequence: GuardSequence<'_, '_>,
        context: &mut FunctionContext,
    ) -> ModuleResult<()> {
        let GuardSequence { guards, position, expression, destination } = sequence;
        let Some(guard) = guards.get(position) else {
            return self.render_expression(tree, writer, expression, destination, context);
        };
        match guard {
            Guard::Boolean(condition) => {
                let condition = self.expression_value(tree, writer, *condition, context)?;
                writer.if_block(tree, condition, |tree, writer| {
                    self.render_guards(
                        tree,
                        writer,
                        GuardSequence { guards, position: position + 1, expression, destination },
                        context,
                    )
                })
            }
            Guard::Pattern { expression: value, pattern } => {
                let source = *value;
                let value = self.rendered_expression(tree, writer, source, context)?;
                let value = self.materialize_pattern_value(tree, writer, source, value, context);
                let mut plan = self.pattern_plan(tree, *pattern, value, None, context)?;
                let condition = combine_conditions(tree, std::mem::take(&mut plan.conditions));
                if let Some(condition) = condition {
                    writer.if_block(tree, condition, |tree, writer| {
                        self.render_pattern_bindings(tree, writer, plan.bindings);
                        self.render_guards(
                            tree,
                            writer,
                            GuardSequence {
                                guards,
                                position: position + 1,
                                expression,
                                destination,
                            },
                            context,
                        )
                    })
                } else {
                    self.render_pattern_bindings(tree, writer, plan.bindings);
                    self.render_guards(
                        tree,
                        writer,
                        GuardSequence { guards, position: position + 1, expression, destination },
                        context,
                    )
                }
            }
        }
    }

    fn render_pattern_scope<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        mut plan: PatternPlan,
        context: &mut FunctionContext,
        render: impl FnOnce(&mut Tree<'t>, &mut Writer<'t>, &mut FunctionContext) -> ModuleResult<()>,
    ) -> ModuleResult<()> {
        let condition = combine_conditions(tree, std::mem::take(&mut plan.conditions));
        if let Some(condition) = condition {
            writer.if_else(
                tree,
                condition,
                |tree, writer| {
                    self.render_pattern_bindings(tree, writer, plan.bindings);
                    render(tree, writer, context)
                },
                |_, writer| {
                    self.render_pattern_failure(writer);
                    Ok(())
                },
            )
        } else {
            self.render_pattern_bindings(tree, writer, plan.bindings);
            render(tree, writer, context)
        }
    }

    fn render_pattern_bindings<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        bindings: Vec<PatternBinding>,
    ) {
        for binding in bindings {
            match binding {
                PatternBinding::Variable { name, value } => {
                    writer.constant(tree, &name, value, false);
                }
                PatternBinding::Constructor { names, value } => {
                    writer.constant_object_pattern(tree, &names, value);
                }
            }
        }
    }

    fn render_pattern_failure(&self, writer: &mut Writer<'_>) {
        writer.throw_error("Pattern match failure");
    }

    fn pattern_plan(
        &self,
        tree: &mut Tree,
        pattern: PatternId,
        value: ExpressionId,
        root_name: Option<&str>,
        context: &mut FunctionContext,
    ) -> ModuleResult<PatternPlan> {
        let mut plan = PatternPlan::default();
        self.extend_pattern_plan(tree, pattern, value, root_name, context, &mut plan)?;
        Ok(plan)
    }

    fn extend_pattern_plan(
        &self,
        tree: &mut Tree,
        pattern: PatternId,
        value: ExpressionId,
        root_name: Option<&str>,
        context: &mut FunctionContext,
        plan: &mut PatternPlan,
    ) -> ModuleResult<()> {
        match &self.module.storage[pattern].kind {
            PatternKind::Variable(parameter) => {
                self.bind_pattern_parameter(parameter, value, root_name, context, plan);
            }
            PatternKind::Named { parameter, pattern } => {
                let binding = tree.duplicate(&value);
                self.bind_pattern_parameter(parameter, binding, root_name, context, plan);
                self.extend_pattern_plan(tree, *pattern, value, root_name, context, plan)?;
            }
            PatternKind::Wildcard => {}
            PatternKind::Literal(literal) => {
                let literal = literal_expression(tree, literal, self.module.file_id)?;
                plan.conditions.push(tree.binary(BinaryOperator::StrictEqual, value, literal));
            }
            PatternKind::Array(patterns) => {
                let array = tree.identifier("Array");
                let is_array = tree.member(array, "isArray");
                let argument = tree.duplicate(&value);
                let is_array = tree.call(is_array, vec![argument]);
                plan.conditions.push(is_array);
                let source = tree.duplicate(&value);
                let length = tree.member(source, "length");
                let expected = tree.number(patterns.len().to_string());
                plan.conditions.push(tree.binary(BinaryOperator::StrictEqual, length, expected));
                for (index, pattern) in patterns.iter().enumerate() {
                    let index = tree.number(index.to_string());
                    let source = tree.duplicate(&value);
                    let element = tree.index(source, index);
                    self.extend_pattern_plan(tree, *pattern, element, None, context, plan)?;
                }
            }
            PatternKind::Record(fields) => {
                for field in fields.iter() {
                    let source = tree.duplicate(&value);
                    let field_value = tree.member(source, field.field.name.as_str());
                    self.extend_pattern_plan(
                        tree,
                        field.pattern,
                        field_value,
                        None,
                        context,
                        plan,
                    )?;
                }
            }
            PatternKind::Constructor { global, arguments } => {
                let expected = tree.string(global.item_name.as_str());
                if arguments.is_empty() {
                    let source = tree.duplicate(&value);
                    plan.conditions.push(tree.binary(
                        BinaryOperator::StrictEqual,
                        source,
                        expected,
                    ));
                } else {
                    let source = tree.duplicate(&value);
                    let tag = tree.member(source, "tag");
                    plan.conditions.push(tree.binary(BinaryOperator::StrictEqual, tag, expected));
                }

                let mut argument_names = Vec::with_capacity(arguments.len());
                for pattern in arguments.iter() {
                    let name = pattern_parameter(&self.module.storage, *pattern)
                        .map(|parameter| context.allocate(&parameter.name));
                    argument_names.push(name);
                }
                let binding_position = plan.bindings.len();

                for (index, (pattern, name)) in
                    arguments.iter().zip(argument_names.iter()).enumerate()
                {
                    let field = format!("_{}", index + 1);
                    let source = tree.duplicate(&value);
                    let argument = tree.member(source, field);
                    self.extend_pattern_plan(
                        tree,
                        *pattern,
                        argument,
                        name.as_deref(),
                        context,
                        plan,
                    )?;
                }
                if argument_names.iter().any(Option::is_some) {
                    plan.bindings.insert(
                        binding_position,
                        PatternBinding::Constructor { names: argument_names, value },
                    );
                }
            }
        }
        Ok(())
    }

    fn bind_pattern_parameter(
        &self,
        parameter: &Parameter,
        value: ExpressionId,
        root_name: Option<&str>,
        context: &mut FunctionContext,
        plan: &mut PatternPlan,
    ) {
        if let Some(root_name) = root_name {
            context.bind_direct(parameter, SmolStr::new(root_name));
        } else {
            let name = context.allocate(&parameter.name);
            context.bind_direct(parameter, name.clone());
            plan.bindings.push(PatternBinding::Variable { name, value });
        }
    }

    fn allocate_pattern_argument(
        &self,
        pattern: PatternId,
        context: &mut FunctionContext,
    ) -> SmolStr {
        let preferred = pattern_parameter(&self.module.storage, pattern)
            .map(|parameter| parameter.name.as_str())
            .unwrap_or("$argument");
        context.allocate(preferred)
    }

    fn materialize_pattern_value<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        source: FunctionalExpressionId,
        value: RenderedExpression,
        context: &mut FunctionContext,
    ) -> ExpressionId {
        if !value.pending_evaluation
            || matches!(
                self.module.storage[source].kind,
                ExpressionKind::Literal { .. }
                    | ExpressionKind::Constructor { .. }
                    | ExpressionKind::Global { .. }
                    | ExpressionKind::Local { .. }
            )
        {
            return value.value;
        }
        let name = context.allocate("$scrutinee");
        writer.constant(tree, &name, value.value, false);
        tree.identifier(name)
    }

    fn materialize_rendered_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expression: &mut RenderedExpression,
        preferred: &str,
        context: &mut FunctionContext,
    ) {
        if rendered_expression_is_reusable(tree, expression) {
            return;
        }
        let name = context.allocate(preferred);
        let identifier = tree.identifier(&name);
        let value = std::mem::replace(&mut expression.value, identifier);
        writer.constant(tree, &name, value, false);
        expression.pending_evaluation = false;
    }

    fn materialize_rendered_expressions<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        expressions: &mut [RenderedExpression],
        preferred: &str,
        context: &mut FunctionContext,
    ) {
        for expression in expressions {
            self.materialize_rendered_expression(tree, writer, expression, preferred, context);
        }
    }

    fn materialize_value<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        value: ExpressionId,
        preferred: &str,
        context: &mut FunctionContext,
    ) -> ExpressionId {
        let name = context.allocate(preferred);
        writer.constant(tree, &name, value, false);
        tree.identifier(name)
    }

    fn record_update_expression<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        record: FunctionalExpressionId,
        updates: &[RecordUpdate],
        context: &mut FunctionContext,
    ) -> ModuleResult<ExpressionId> {
        let record = self.rendered_expression(tree, writer, record, context)?;
        let record_is_reusable = rendered_expression_is_reusable(tree, &record);
        self.record_updates(tree, writer, record.value, record_is_reusable, updates, context)
    }

    fn record_updates<'t>(
        &self,
        tree: &mut Tree<'t>,
        writer: &mut Writer<'t>,
        mut record: ExpressionId,
        record_is_reusable: bool,
        updates: &[RecordUpdate],
        context: &mut FunctionContext,
    ) -> ModuleResult<ExpressionId> {
        let mut properties = Vec::with_capacity(updates.len() + 1);
        let record = if record_updates_reuse_source(updates) {
            if !record_is_reusable {
                record = self.materialize_value(tree, writer, record, "$record", context);
            }
            properties.push(ObjectProperty::Spread(tree.duplicate(&record)));
            Some(record)
        } else {
            properties.push(ObjectProperty::Spread(record));
            None
        };
        for update in updates {
            if self.record_update_rendering_is_eager(update, context) {
                let value = tree.object(std::mem::take(&mut properties));
                let value = self.materialize_value(tree, writer, value, "$record", context);
                properties.push(ObjectProperty::Spread(value));
            }
            match update {
                RecordUpdate::Leaf { field, expression } => {
                    let value = self.rendered_expression(tree, writer, *expression, context)?;
                    properties.push(ObjectProperty::Field {
                        name: field.name.clone(),
                        value: value.value,
                    });
                }
                RecordUpdate::Branch { field, updates } => {
                    // Nested updates revisit their original source paths. Reusing the path here
                    // preserves each observable property read while the root record remains stable.
                    let record =
                        record.as_ref().expect("invariant violated: nested update has no source");
                    let source = tree.duplicate(record);
                    let nested = tree.member(source, field.name.as_str());
                    let value =
                        self.record_updates(tree, writer, nested, true, updates, context)?;
                    properties.push(ObjectProperty::Field { name: field.name.clone(), value });
                }
            }
        }
        Ok(tree.object(properties))
    }
}

fn rendered_expression_is_reusable(tree: &Tree, expression: &RenderedExpression) -> bool {
    !expression.pending_evaluation || tree.expression_is_atomic(&expression.value)
}

fn record_updates_reuse_source(updates: &[RecordUpdate]) -> bool {
    updates.iter().any(|update| matches!(update, RecordUpdate::Branch { .. }))
}

fn effect_expression(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    effect: &EffectExpression,
) -> ModuleResult<ExpressionId> {
    let effect = capture_effect(renderer, effect)?;
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    let effect_name = context.allocate("$effect");
    writer.constant_arrow(&effect_name, vec![], |writer| {
        let mut renderer = generator.renderer(tree, writer, context);
        execute_effect(&mut renderer, effect, Destination::Return)
    })?;
    Ok(tree.identifier(effect_name))
}

fn capture_effect(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    effect: &EffectExpression,
) -> ModuleResult<CapturedEffect> {
    match effect {
        EffectExpression::Pure(value) => {
            let value = capture_effect_value(renderer, *value, "$value")?;
            Ok(CapturedEffect::Pure { value })
        }
        EffectExpression::Bind { action, parameter, body } => {
            let action = capture_effect_action(renderer, *action, "$action")?;
            Ok(CapturedEffect::Bind { action, parameter: parameter.clone(), body: *body })
        }
        EffectExpression::Map { function, action } => {
            let function = capture_effect_value(renderer, *function, "$function")?;
            let action = capture_effect_action(renderer, *action, "$action")?;
            Ok(CapturedEffect::Map { function, action })
        }
        EffectExpression::Apply { function_action, argument_action } => {
            let function_action =
                capture_effect_action(renderer, *function_action, "$functionAction")?;
            let argument_action =
                capture_effect_action(renderer, *argument_action, "$argumentAction")?;
            Ok(CapturedEffect::Apply { function_action, argument_action })
        }
    }
}

fn capture_effect_action(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    expression: FunctionalExpressionId,
    preferred_name: &str,
) -> ModuleResult<CapturedEffectAction> {
    let generator = renderer.generator;
    if let ExpressionKind::Effect { effect } = &generator.module.storage[expression].kind {
        let effect = capture_effect(renderer, effect)?;
        return Ok(CapturedEffectAction::Effect(Box::new(effect)));
    }

    let expression = capture_effect_value(renderer, expression, preferred_name)?;
    Ok(CapturedEffectAction::Expression(expression))
}

fn capture_effect_value(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    expression: FunctionalExpressionId,
    preferred_name: &str,
) -> ModuleResult<ExpressionId> {
    let FunctionRenderer { generator, tree, writer, context } = renderer;
    let expression_is_reusable = generator.functional_expression_is_reusable(expression, context);
    let value = generator.rendered_expression(tree, writer, expression, context)?;
    if expression_is_reusable || rendered_expression_is_reusable(tree, &value) {
        return Ok(value.value);
    }
    let name = context.allocate(preferred_name);
    writer.constant(tree, &name, value.value, false);
    Ok(tree.identifier(name))
}

fn execute_effect(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    effect: CapturedEffect,
    destination: Destination<'_>,
) -> ModuleResult<()> {
    match effect {
        CapturedEffect::Pure { value } => {
            renderer.generator.render_destination(
                renderer.tree,
                renderer.writer,
                value,
                destination,
            );
            Ok(())
        }
        CapturedEffect::Bind { action, parameter, body } => {
            if let CapturedEffectAction::Effect(effect) = &action
                && let CapturedEffect::Pure { value } = effect.as_ref()
                && local_uses(&renderer.generator.module.storage, body, parameter.id) <= 1
            {
                let value = renderer.tree.duplicate(value);
                renderer.context.bind_inline(&parameter, value);
            } else {
                let (_, parameter_name) = execute_effect_action(renderer, action, &parameter.name)?;
                renderer.context.bind_direct(&parameter, parameter_name);
            }
            renderer.generator.render_expression(
                renderer.tree,
                renderer.writer,
                body,
                destination.effect(),
                renderer.context,
            )
        }
        CapturedEffect::Map { function, action } => {
            let value = execute_effect_action_value(renderer, action, "$value")?;
            let result = renderer.tree.call(function, vec![value]);
            renderer.generator.render_destination(
                renderer.tree,
                renderer.writer,
                result,
                destination,
            );
            Ok(())
        }
        CapturedEffect::Apply { function_action, argument_action } => {
            let function = if matches!(&argument_action, CapturedEffectAction::Effect(_)) {
                execute_effect_action(renderer, function_action, "$function")?.0
            } else {
                execute_effect_action_value(renderer, function_action, "$function")?
            };
            let argument = execute_effect_action_value(renderer, argument_action, "$argument")?;
            let result = renderer.tree.call(function, vec![argument]);
            renderer.generator.render_destination(
                renderer.tree,
                renderer.writer,
                result,
                destination,
            );
            Ok(())
        }
    }
}

fn execute_effect_action_value(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    action: CapturedEffectAction,
    preferred_name: &str,
) -> ModuleResult<ExpressionId> {
    match action {
        CapturedEffectAction::Expression(action) => Ok(renderer.tree.call(action, vec![])),
        CapturedEffectAction::Effect(effect) => {
            let name = renderer.context.allocate(preferred_name);
            renderer.writer.mutable(&name);
            execute_effect(renderer, *effect, Destination::Assign(&name))?;
            Ok(renderer.tree.identifier(name))
        }
    }
}

fn execute_effect_action(
    renderer: &mut FunctionRenderer<'_, '_, '_>,
    action: CapturedEffectAction,
    preferred_name: &str,
) -> ModuleResult<(ExpressionId, SmolStr)> {
    let name = renderer.context.allocate(preferred_name);
    match action {
        CapturedEffectAction::Expression(action) => {
            let value = renderer.tree.call(action, vec![]);
            renderer.writer.constant(renderer.tree, &name, value, false);
        }
        CapturedEffectAction::Effect(effect) => {
            if let CapturedEffect::Pure { value } = *effect {
                renderer.writer.constant(renderer.tree, &name, value, false);
            } else {
                renderer.writer.mutable(&name);
                execute_effect(renderer, *effect, Destination::Assign(&name))?;
            }
        }
    }
    Ok((renderer.tree.identifier(&name), name))
}

fn local_expression(
    generator: &Generator<'_>,
    tree: &mut Tree,
    parameter: &Parameter,
    context: &FunctionContext,
) -> ModuleResult<ExpressionId> {
    match context.locals.get(&parameter.id) {
        Some(LocalBinding::Direct(name)) => Ok(tree.identifier(name)),
        Some(LocalBinding::Inline(expression)) => Ok(tree.duplicate(expression)),
        Some(LocalBinding::Lazy(name)) => {
            let accessor = tree.identifier(name);
            Ok(tree.call(accessor, vec![]))
        }
        None => Err(generator
            .unsupported(UnsupportedState::MissingLocal { name: parameter.name.to_string() })),
    }
}

impl Generator<'_> {
    fn global_expression(&self, tree: &mut Tree, global: &Global) -> ModuleResult<ExpressionId> {
        let file_id = global_file(global.id);
        if file_id == self.module.file_id {
            if let Some(lazy_name) = self.lazy_global_names.get(&global.id) {
                let lazy = tree.identifier(lazy_name);
                return Ok(tree.call(lazy, vec![]));
            }
            let name = self.global_names.get(&global.id).ok_or_else(|| {
                self.unsupported(UnsupportedState::MissingGlobal {
                    name: global.item_name.to_string(),
                })
            })?;
            Ok(tree.identifier(name))
        } else {
            if let Some(name) = self.external_named_imports.get(&global.id) {
                return Ok(tree.identifier(name));
            }
            let namespace = self
                .external_module_namespaces
                .get(&file_id)
                .expect("invariant violated: external JavaScript global has no module namespace");
            let namespace = tree.identifier(namespace);
            Ok(tree.member(namespace, global.item_name.as_str()))
        }
    }
}

fn sorted_value_declarations<'m>(generator: &'m Generator<'_>) -> Vec<(&'m Declaration, bool)> {
    let values = generator
        .module
        .declarations
        .iter()
        .filter(|declaration| matches!(declaration.kind, DeclarationKind::Value(_)));
    let mut values = values.collect_vec();
    // An ordinary initializer can call a source function whose body uses generated evidence.
    // Prefer generated declarations whenever explicit dependencies leave their order unconstrained.
    values.sort_by_key(|declaration| !matches!(declaration.global.id, GlobalId::Generated(_, _)));
    let positions = values
        .iter()
        .enumerate()
        .map(|(position, declaration)| (declaration.global.id, position))
        .collect::<FxHashMap<_, _>>();
    let mut dependencies = vec![Vec::new(); values.len()];
    for (position, declaration) in values.iter().enumerate() {
        let DeclarationKind::Value(expression) = declaration.kind else {
            unreachable!("invariant violated: expected value declaration")
        };
        let mut globals = FxHashSet::default();
        collect_expression_globals(generator.module, expression, false, &mut globals);
        for global in globals {
            let Some(&dependency) = positions.get(&global) else {
                continue;
            };
            let source_is_lazy = generator.lazy_global_names.contains_key(&declaration.global.id);
            let dependency_is_lazy =
                generator.lazy_global_names.contains_key(&values[dependency].global.id);
            if source_is_lazy && dependency_is_lazy {
                continue;
            }
            dependencies[position].push(dependency);
        }
        dependencies[position].sort_unstable();
        dependencies[position].dedup();
    }

    let cyclic = cyclic_initializers(&dependencies);
    let ordered = initializer_postorder(&dependencies);
    ordered.into_iter().map(|position| (values[position], cyclic[position])).collect_vec()
}

fn render_exports(renderer: &mut ModuleRenderer<'_, '_, '_>) {
    let ModuleRenderer { generator, writer, .. } = renderer;
    let mut rendered = false;
    for declaration in generator.module.declarations.iter() {
        if !declaration.exported {
            continue;
        }
        let local = generator.global_name(declaration.global.id);
        if local == declaration.global.item_name {
            continue;
        }
        writer.export(local, &declaration.global.item_name);
        rendered = true;
    }
    for exports in generator.module.surface.indirect.iter() {
        let specifiers = exports.globals.iter().map(|global| global.item_name.to_string());
        let dependency = generator.module_dependency(exports.file_id);
        let path = format!("../{}", module_filename(&dependency.module_name));
        writer.re_export(specifiers.collect_vec(), &path);
        rendered = true;
    }
    if rendered {
        writer.blank();
    }
}

impl Generator<'_> {
    fn declaration_is_inline_exported(&self, declaration: &Declaration) -> bool {
        let local = self.global_name(declaration.global.id);
        declaration.exported && local == declaration.global.item_name
    }

    fn global_name(&self, id: GlobalId) -> &str {
        self.global_names
            .get(&id)
            .map(SmolStr::as_str)
            .expect("invariant violated: JavaScript global has no allocated name")
    }

    fn module_dependency(&self, file_id: FileId) -> &ModuleDependency {
        self.module_dependencies
            .get(&file_id)
            .expect("invariant violated: referenced module has no dependency metadata")
    }

    fn unsupported(&self, state: UnsupportedState) -> ModuleError {
        ModuleError::Unsupported { file_id: self.module.file_id, state }
    }
}

fn global_file(id: GlobalId) -> FileId {
    match id {
        GlobalId::Term(file_id, _) | GlobalId::Generated(file_id, _) => file_id,
        GlobalId::Instance(
            functional::tree::InstanceIdentity::Declared(file_id, _)
            | functional::tree::InstanceIdentity::Derived(file_id, _),
        ) => file_id,
    }
}

fn collect_expression_references(
    module: &FunctionalModule,
    expression: FunctionalExpressionId,
    seen: &mut FxHashSet<GlobalId>,
    globals: &mut Vec<Global>,
) {
    match &module.storage[expression].kind {
        ExpressionKind::Error
        | ExpressionKind::Literal { .. }
        | ExpressionKind::Local { .. }
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => {}
        ExpressionKind::Constructor { global } | ExpressionKind::Global { global } => {
            if seen.insert(global.id) {
                globals.push(global.clone());
            }
        }
        ExpressionKind::Array { elements } => {
            for expression in elements.iter() {
                collect_expression_references(module, *expression, seen, globals);
            }
        }
        ExpressionKind::Record { fields } => {
            for field in fields.iter() {
                collect_expression_references(module, field.expression, seen, globals);
            }
        }
        ExpressionKind::RecordUpdate { record, updates } => {
            collect_expression_references(module, *record, seen, globals);
            collect_update_references(module, updates, seen, globals);
        }
        ExpressionKind::Project { record, .. } | ExpressionKind::Unary { value: record, .. } => {
            collect_expression_references(module, *record, seen, globals);
        }
        ExpressionKind::Binary { left, right, .. } => {
            collect_expression_references(module, *left, seen, globals);
            collect_expression_references(module, *right, seen, globals);
        }
        ExpressionKind::Abstraction { body, .. }
        | ExpressionKind::UncurriedAbstraction { body, .. } => {
            collect_expression_references(module, *body, seen, globals);
        }
        ExpressionKind::Application { function, arguments, .. }
        | ExpressionKind::UncurriedApplication { function, arguments, .. } => {
            collect_expression_references(module, *function, seen, globals);
            for argument in arguments.iter() {
                collect_expression_references(module, *argument, seen, globals);
            }
        }
        kind @ ExpressionKind::StyleX(_) => {
            for_each_expression_child(kind, |child| {
                collect_expression_references(module, child, seen, globals);
            });
        }
        ExpressionKind::IfThenElse { condition, then, else_ } => {
            collect_expression_references(module, *condition, seen, globals);
            collect_expression_references(module, *then, seen, globals);
            collect_expression_references(module, *else_, seen, globals);
        }
        ExpressionKind::Case { scrutinees, alternatives } => {
            for scrutinee in scrutinees.iter() {
                collect_expression_references(module, *scrutinee, seen, globals);
            }
            for alternative in alternatives.iter() {
                for pattern in alternative.patterns.iter() {
                    collect_pattern_references(module, *pattern, seen, globals);
                }
                collect_expression_references(module, alternative.expression, seen, globals);
            }
        }
        ExpressionKind::Guarded { alternatives } => {
            collect_guarded_references(module, alternatives, seen, globals);
        }
        ExpressionKind::Let { bindings, body, .. } => {
            for binding in bindings.iter() {
                collect_expression_references(module, binding.expression, seen, globals);
            }
            collect_expression_references(module, *body, seen, globals);
        }
        ExpressionKind::LetPattern { pattern, value, body } => {
            collect_pattern_references(module, *pattern, seen, globals);
            collect_expression_references(module, *value, seen, globals);
            collect_expression_references(module, *body, seen, globals);
        }
        ExpressionKind::Effect { effect } => match effect {
            EffectExpression::Pure(value) => {
                collect_expression_references(module, *value, seen, globals);
            }
            EffectExpression::Bind { action, body, .. } => {
                collect_expression_references(module, *action, seen, globals);
                collect_expression_references(module, *body, seen, globals);
            }
            EffectExpression::Map { function, action } => {
                collect_expression_references(module, *function, seen, globals);
                collect_expression_references(module, *action, seen, globals);
            }
            EffectExpression::Apply { function_action, argument_action } => {
                collect_expression_references(module, *function_action, seen, globals);
                collect_expression_references(module, *argument_action, seen, globals);
            }
        },
    }
}

fn collect_update_references(
    module: &FunctionalModule,
    updates: &[RecordUpdate],
    seen: &mut FxHashSet<GlobalId>,
    globals: &mut Vec<Global>,
) {
    for update in updates {
        match update {
            RecordUpdate::Leaf { expression, .. } => {
                collect_expression_references(module, *expression, seen, globals);
            }
            RecordUpdate::Branch { updates, .. } => {
                collect_update_references(module, updates, seen, globals);
            }
        }
    }
}

fn collect_pattern_references(
    module: &FunctionalModule,
    pattern: PatternId,
    seen: &mut FxHashSet<GlobalId>,
    globals: &mut Vec<Global>,
) {
    match &module.storage[pattern].kind {
        PatternKind::Variable(_) | PatternKind::Wildcard | PatternKind::Literal(_) => {}
        PatternKind::Named { pattern, .. } => {
            collect_pattern_references(module, *pattern, seen, globals);
        }
        PatternKind::Array(patterns) => {
            for pattern in patterns.iter() {
                collect_pattern_references(module, *pattern, seen, globals);
            }
        }
        PatternKind::Record(fields) => {
            for field in fields.iter() {
                collect_pattern_references(module, field.pattern, seen, globals);
            }
        }
        PatternKind::Constructor { global, arguments } => {
            if seen.insert(global.id) {
                globals.push(global.clone());
            }
            for pattern in arguments.iter() {
                collect_pattern_references(module, *pattern, seen, globals);
            }
        }
    }
}

fn collect_guarded_references(
    module: &FunctionalModule,
    alternatives: &[GuardedAlternative],
    seen: &mut FxHashSet<GlobalId>,
    globals: &mut Vec<Global>,
) {
    for alternative in alternatives {
        for guard in alternative.guards.iter() {
            match guard {
                Guard::Boolean(expression) => {
                    collect_expression_references(module, *expression, seen, globals);
                }
                Guard::Pattern { expression, pattern } => {
                    collect_expression_references(module, *expression, seen, globals);
                    collect_pattern_references(module, *pattern, seen, globals);
                }
            }
        }
        collect_expression_references(module, alternative.expression, seen, globals);
    }
}

fn collect_expression_globals(
    module: &FunctionalModule,
    expression: FunctionalExpressionId,
    descend_abstractions: bool,
    globals: &mut FxHashSet<GlobalId>,
) {
    match &module.storage[expression].kind {
        ExpressionKind::Global { global } | ExpressionKind::Constructor { global } => {
            globals.insert(global.id);
        }
        ExpressionKind::Abstraction { body, .. }
        | ExpressionKind::UncurriedAbstraction { body, .. } => {
            if descend_abstractions {
                collect_expression_globals(module, *body, descend_abstractions, globals);
            }
        }
        ExpressionKind::Let { recursive, bindings, body } => {
            let lazy_values = *recursive
                && !bindings
                    .iter()
                    .all(|binding| is_abstraction(&module.storage[binding.expression].kind));
            if descend_abstractions || lazy_values {
                for binding in bindings.iter() {
                    collect_expression_globals(
                        module,
                        binding.expression,
                        descend_abstractions,
                        globals,
                    );
                }
            } else if !*recursive {
                for binding in bindings.iter() {
                    collect_expression_globals(module, binding.expression, false, globals);
                }
            }
            collect_expression_globals(module, *body, descend_abstractions, globals);
        }
        _ => collect_expression_children(module, expression, descend_abstractions, globals),
    }
}

fn collect_expression_children(
    module: &FunctionalModule,
    expression: FunctionalExpressionId,
    descend_abstractions: bool,
    globals: &mut FxHashSet<GlobalId>,
) {
    if descend_abstractions {
        let mut seen = FxHashSet::default();
        let mut references = vec![];
        collect_expression_references(module, expression, &mut seen, &mut references);
        globals.extend(references.into_iter().map(|global| global.id));
        return;
    }
    match &module.storage[expression].kind {
        ExpressionKind::Error
        | ExpressionKind::Literal { .. }
        | ExpressionKind::Constructor { .. }
        | ExpressionKind::Global { .. }
        | ExpressionKind::Local { .. }
        | ExpressionKind::SynthesizedEvidence { .. }
        | ExpressionKind::TrivialEvidence => {}
        ExpressionKind::Array { elements } => {
            for expression in elements.iter() {
                collect_expression_globals(module, *expression, false, globals);
            }
        }
        ExpressionKind::Record { fields } => {
            for field in fields.iter() {
                collect_expression_globals(module, field.expression, false, globals);
            }
        }
        ExpressionKind::RecordUpdate { record, updates } => {
            collect_expression_globals(module, *record, false, globals);
            collect_update_globals(module, updates, false, globals);
        }
        ExpressionKind::Project { record, .. } | ExpressionKind::Unary { value: record, .. } => {
            collect_expression_globals(module, *record, false, globals);
        }
        ExpressionKind::Binary { left, right, .. } => {
            collect_expression_globals(module, *left, false, globals);
            collect_expression_globals(module, *right, false, globals);
        }
        ExpressionKind::Abstraction { .. } | ExpressionKind::UncurriedAbstraction { .. } => {}
        ExpressionKind::Application { function, arguments, .. }
        | ExpressionKind::UncurriedApplication { function, arguments, .. } => {
            collect_expression_globals(module, *function, false, globals);
            for argument in arguments.iter() {
                collect_expression_globals(module, *argument, false, globals);
            }
        }
        kind @ ExpressionKind::StyleX(_) => {
            for_each_expression_child(kind, |child| {
                collect_expression_globals(module, child, false, globals);
            });
        }
        ExpressionKind::IfThenElse { condition, then, else_ } => {
            collect_expression_globals(module, *condition, false, globals);
            collect_expression_globals(module, *then, false, globals);
            collect_expression_globals(module, *else_, false, globals);
        }
        ExpressionKind::Case { scrutinees, alternatives } => {
            for expression in scrutinees.iter() {
                collect_expression_globals(module, *expression, false, globals);
            }
            for alternative in alternatives.iter() {
                collect_expression_globals(module, alternative.expression, false, globals);
            }
        }
        ExpressionKind::Guarded { alternatives } => {
            for alternative in alternatives.iter() {
                for guard in alternative.guards.iter() {
                    let expression = match guard {
                        Guard::Boolean(expression) | Guard::Pattern { expression, .. } => {
                            *expression
                        }
                    };
                    collect_expression_globals(module, expression, false, globals);
                }
                collect_expression_globals(module, alternative.expression, false, globals);
            }
        }
        ExpressionKind::Let { .. } => unreachable!("let expressions are handled by the caller"),
        ExpressionKind::LetPattern { value, body, .. } => {
            collect_expression_globals(module, *value, false, globals);
            collect_expression_globals(module, *body, false, globals);
        }
        ExpressionKind::Effect { effect } => match effect {
            EffectExpression::Pure(value) => {
                collect_expression_globals(module, *value, false, globals);
            }
            EffectExpression::Bind { action, .. } => {
                collect_expression_globals(module, *action, false, globals);
            }
            EffectExpression::Map { function, action } => {
                collect_expression_globals(module, *function, false, globals);
                collect_expression_globals(module, *action, false, globals);
            }
            EffectExpression::Apply { function_action, argument_action } => {
                collect_expression_globals(module, *function_action, false, globals);
                collect_expression_globals(module, *argument_action, false, globals);
            }
        },
    }
}

fn collect_update_globals(
    module: &FunctionalModule,
    updates: &[RecordUpdate],
    descend_abstractions: bool,
    globals: &mut FxHashSet<GlobalId>,
) {
    for update in updates {
        match update {
            RecordUpdate::Leaf { expression, .. } => {
                collect_expression_globals(module, *expression, descend_abstractions, globals);
            }
            RecordUpdate::Branch { updates, .. } => {
                collect_update_globals(module, updates, descend_abstractions, globals);
            }
        }
    }
}
