use files::ForeignSourceKind;
use functional::initializers::{
    cyclic_initializers, initializer_components, initializer_postorder,
};
use functional::tree::{
    Declaration, DeclarationKind, ExpressionKind, GlobalId, Module as FunctionalModule, Origin,
};
use itertools::Itertools;
use oxc_allocator::Allocator;
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::format_smolstr;
use std::cell::RefCell;
use std::rc::Rc;

use crate::convert::generator::names::NameAllocator;
use crate::error::{ModuleDiagnostic, ModuleResult};
use crate::module::{
    Module, ModuleFiles, STYLEX_THEME_FILENAME, foreign_filename, module_filename,
    runtime_filename, stylex_theme_module_filename,
};
use crate::program_text::ProgramText;
use crate::tree::Tree;
use crate::writer::Writer;

use super::declaration::{
    render_constructors, render_foreign_declarations, render_lazy_initializers,
    render_source_functions, render_value_declarations,
};
use super::references::collect_expression_globals;
use super::structure::{
    collect_module_references, cyclic_instance_initializers, has_local_lazy_initializers,
    has_runtime_operators,
};
use super::stylex::{collect_stylex_references, collect_stylex_theme};
use super::tail_call::{TailCallIdentity, TailCallProfile, global_profiles, tail_call_groups};
use super::{
    ForeignImport, FunctionContext, FunctionRenderer, Generator, ModuleRenderer, NamedImport,
    OutputFile, global_file,
};

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
            let local_binding_name = allocator.allocate(preferred.replace('.', "_"));
            let binding = NamedImport { exported_name: global.item_name, local_binding_name };
            external_named_imports.insert(global.id, binding);
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
        let stylex_theme = if has_stylex { collect_stylex_theme(module) } else { None };

        let has_foreign = module
            .declarations
            .iter()
            .any(|declaration| matches!(declaration.kind, DeclarationKind::Foreign));
        let foreign_import = has_foreign.then(|| ForeignImport {
            namespace: allocator.allocate("$foreign"),
            kind: foreign_kind.unwrap_or(ForeignSourceKind::JavaScript),
        });
        let lazy_globals = cyclic_instance_initializers(module);
        let requires_runtime = !lazy_globals.is_empty()
            || has_local_lazy_initializers(module)
            || has_runtime_operators(module);
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
            let dispatcher_name = |profiles: &[TailCallProfile]| {
                let mut dispatcher_names = profiles.iter().map(|profile| {
                    let TailCallIdentity::Global(global) = profile.identity else {
                        unreachable!("invariant violated: global tail-call group contains a local")
                    };
                    global_names[&global].as_str()
                });
                let dispatcher_suffix = dispatcher_names.join("_");
                allocator.allocate(format_smolstr!("$tail_{dispatcher_suffix}"))
            };
            for group in tail_call_groups(module, &profiles, dispatcher_name) {
                let position = global_tail_call_groups.len();
                for profile in &group.profiles {
                    let TailCallIdentity::Global(global) = profile.identity else {
                        unreachable!("invariant violated: global tail-call group contains a local")
                    };
                    global_tail_call_group_positions.insert(global, position);
                }
                global_tail_call_groups.push(group);
            }
        }
        let reserved_module_names = allocator.allocated_names().cloned().collect();
        let reserved_module_names = Rc::new(reserved_module_names);

        Generator {
            module,
            module_dependencies,
            global_names,
            external_module_namespaces,
            rendered_module_namespaces: RefCell::default(),
            external_named_imports,
            stylex_namespace,
            stylex_theme,
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
        let program_text = ProgramText::new();
        let mut tree = Tree::new(&allocator, ProgramText::clone(&program_text));
        let mut writer = Writer::new(&allocator, program_text);
        let declarations = sorted_value_declarations(&self);
        let initializer_cycle = {
            let mut renderer = ModuleRenderer {
                generator: &self,
                tree: &mut tree,
                writer: &mut writer,
                file: OutputFile::Index,
            };
            render_constructors(&mut renderer);
            render_source_functions(&mut renderer)?;
            render_foreign_declarations(&mut renderer);
            render_lazy_initializers(&mut renderer)?;
            let initializer_cycle = render_value_declarations(&mut renderer, &declarations)?;
            render_exports(&mut renderer);
            initializer_cycle
        };
        writer.prepend(|writer| {
            let mut renderer = ModuleRenderer {
                generator: &self,
                tree: &mut tree,
                writer,
                file: OutputFile::Index,
            };
            render_imports(&mut renderer);
        });
        let stylex_theme_source = if let Some(theme) = &self.stylex_theme {
            let program_text = ProgramText::new();
            let mut tree = Tree::new(&allocator, ProgramText::clone(&program_text));
            let mut writer = Writer::new(&allocator, program_text);
            let mut renderer = ModuleRenderer {
                generator: &self,
                tree: &mut tree,
                writer: &mut writer,
                file: OutputFile::StyleXTheme(theme),
            };
            render_imports(&mut renderer);
            let initializer_cycle = render_value_declarations(&mut renderer, &declarations)?;
            debug_assert!(
                initializer_cycle.is_empty(),
                "invariant violated: statically evaluable StyleX theme values form a cycle"
            );
            render_exports(&mut renderer);
            Some(writer.finish())
        } else {
            None
        };

        let dependencies = self.module.dependencies.iter().map(|dependency| dependency.file_id);
        let dependencies = dependencies.collect_vec();
        let diagnostics = if initializer_cycle.is_empty() {
            Vec::new()
        } else {
            vec![ModuleDiagnostic::InitializerCycle { declarations: initializer_cycle }]
        };
        let requires_runtime = self.runtime_namespace.is_some();
        let foreign_kind = self.foreign_import.as_ref().map(|foreign_import| foreign_import.kind);
        let (index_source, index_mappings) = writer.finish_with_mappings();
        let files =
            ModuleFiles::new(index_source, index_mappings, stylex_theme_source, foreign_kind);
        Ok(Module::new(
            self.module.file_id,
            self.module.name.to_string(),
            dependencies,
            diagnostics,
            requires_runtime,
            files,
        ))
    }

    pub(super) fn renderer<'a, 't>(
        &'a self,
        tree: &'a mut Tree<'t>,
        writer: &'a mut Writer<'t>,
        context: &'a mut FunctionContext,
    ) -> FunctionRenderer<'a, 'm, 't> {
        FunctionRenderer { generator: self, tree, writer, context }
    }
}

fn render_imports(renderer: &mut ModuleRenderer<'_, '_, '_>) {
    let ModuleRenderer { generator, tree, writer, file } = renderer;
    if let OutputFile::StyleXTheme(theme) = file {
        render_named_imports(generator, tree, writer, theme.imports.iter().copied());
        render_stylex_import(generator, tree, writer);
        writer.blank();
        return;
    }
    let rendered_module_namespaces = generator.rendered_module_namespaces.borrow();
    let mut dependencies = rendered_module_namespaces
        .iter()
        .map(|&file_id| (file_id, generator.module_dependency(file_id)))
        .collect_vec();
    dependencies.sort_by_key(|(_, dependency)| dependency.module_name.as_str());
    for (file_id, dependency) in dependencies {
        let _origin = tree.enter_origin(dependency.import.map(Origin::Import));
        let namespace = &generator.external_module_namespaces[&file_id];
        let path = format!("../{}", module_filename(&dependency.module_name));
        writer.import_namespace(namespace, &path);
    }
    render_named_imports(generator, tree, writer, generator.external_named_imports.keys().copied());
    if let Some(theme) = &generator.stylex_theme {
        let definitions = generator
            .module
            .declarations
            .iter()
            .filter(|declaration| theme.definitions.contains(&declaration.global.id))
            .collect_vec();
        let first_definition = definitions.first().and_then(|declaration| declaration.origin);
        let _origin = tree.enter_origin(first_definition);
        let bindings = definitions.iter().map(|declaration| {
            let local = generator.global_name(declaration.global.id);
            (declaration.global.item_name.as_str(), local)
        });
        let bindings = bindings.collect_vec();
        writer.import_named(&bindings, &format!("./{STYLEX_THEME_FILENAME}"));
    }
    render_stylex_import(generator, tree, writer);
    if let Some(foreign_import) = &generator.foreign_import {
        let first_foreign_declaration = generator
            .module
            .declarations
            .iter()
            .find(|declaration| matches!(declaration.kind, DeclarationKind::Foreign))
            .and_then(|declaration| declaration.origin);
        let _origin = tree.enter_origin(first_foreign_declaration);
        let path = format!("./{}", foreign_filename(foreign_import.kind));
        writer.import_namespace(&foreign_import.namespace, &path);
    }
    if let Some(namespace) = &generator.runtime_namespace {
        let path = format!("../{}", runtime_filename());
        writer.import_namespace(namespace, &path);
    }
    if !rendered_module_namespaces.is_empty()
        || generator.stylex_namespace.is_some()
        || !generator.external_named_imports.is_empty()
        || generator.foreign_import.is_some()
        || generator.runtime_namespace.is_some()
    {
        writer.blank();
    }
}

fn render_stylex_import(generator: &Generator<'_>, tree: &mut Tree<'_>, writer: &mut Writer<'_>) {
    let Some(namespace) = &generator.stylex_namespace else {
        return;
    };
    let _origin = tree.enter_origin(generator.module.stylex_import.map(Origin::Import));
    writer.import_namespace(namespace, "@stylexjs/stylex");
}

/// StyleX resolves an imported theme value only when the import names the defining module's
/// theme file. Other values are imported from the module itself, which re-exports its theme.
fn render_named_imports(
    generator: &Generator<'_>,
    tree: &mut Tree<'_>,
    writer: &mut Writer<'_>,
    globals: impl Iterator<Item = GlobalId>,
) {
    let mut groups = FxHashMap::<_, Vec<_>>::default();
    for global in globals {
        let binding = generator
            .external_named_imports
            .get(&global)
            .expect("invariant violated: imported StyleX value has no named import");
        let theme = generator.module.stylex_theme_imports.contains(&global);
        let group = groups.entry((global_file(global), theme)).or_default();
        group.push((binding.exported_name.as_str(), binding.local_binding_name.as_str()));
    }
    let mut groups = groups.into_iter().collect_vec();
    groups.sort_by_key(|&((file_id, theme), _)| {
        (generator.module_dependency(file_id).module_name.as_str(), theme)
    });
    for ((file_id, theme), mut bindings) in groups {
        let dependency = generator.module_dependency(file_id);
        let _origin = tree.enter_origin(dependency.import.map(Origin::Import));
        let filename = if theme {
            stylex_theme_module_filename(&dependency.module_name)
        } else {
            module_filename(&dependency.module_name)
        };
        bindings.sort_unstable();
        writer.import_named(&bindings, &format!("../{filename}"));
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
    let mut lexical_dependencies = vec![Vec::new(); values.len()];
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

        let mut globals = FxHashSet::default();
        collect_expression_globals(generator.module, expression, true, &mut globals);
        lexical_dependencies[position] =
            globals.iter().filter_map(|global| positions.get(global).copied()).collect_vec();
        lexical_dependencies[position].sort_unstable();
    }

    let cyclic = cyclic_initializers(&dependencies);
    let ordered = initializer_postorder(&dependencies);
    let mut strict_positions = vec![0; values.len()];
    for (rank, &position) in ordered.iter().enumerate() {
        strict_positions[position] = rank;
    }

    // A hoisted function can read another group's values when an initializer calls it.
    // Order lexical groups first, but preserve strict order within each group: delayed
    // references establish group membership, not proof of an eager initializer cycle.
    let mut components = initializer_components(&lexical_dependencies);
    for component in &mut components {
        component.sort_unstable_by_key(|&position| strict_positions[position]);
    }
    components
        .into_iter()
        .flatten()
        .map(|position| (values[position], cyclic[position]))
        .collect_vec()
}

fn render_exports(renderer: &mut ModuleRenderer<'_, '_, '_>) {
    let ModuleRenderer { generator, tree, writer, file } = renderer;
    let mut rendered = false;
    for declaration in generator.module.declarations.iter() {
        if !file.exports(generator, declaration)
            || file.declaration_is_inline_exported(generator, declaration)
        {
            continue;
        }
        let _origin = tree.enter_origin(declaration.origin);
        let local = generator.global_name(declaration.global.id);
        writer.export(local, &declaration.global.item_name);
        rendered = true;
    }
    if let OutputFile::Index = file {
        if let Some(theme) = &generator.stylex_theme {
            let definitions = generator
                .module
                .declarations
                .iter()
                .filter(|declaration| theme.definitions.contains(&declaration.global.id))
                .map(|declaration| declaration.global.item_name.to_string());
            writer.export_from(definitions.collect_vec(), &format!("./{STYLEX_THEME_FILENAME}"));
            rendered = true;
        }
        for exports in generator.module.surface.indirect.iter() {
            let specifiers = exports.globals.iter().map(|global| global.item_name.to_string());
            let dependency = generator.module_dependency(exports.file_id);
            let path = format!("../{}", module_filename(&dependency.module_name));
            writer.export_from(specifiers.collect_vec(), &path);
            rendered = true;
        }
    }
    if rendered {
        writer.blank();
    }
}

impl OutputFile<'_> {
    /// Whether this file renders the declaration, if it is a value: a module's theme definitions
    /// render only in its theme file, which renders nothing else except the values they read.
    pub(super) fn renders_value(
        self,
        generator: &Generator<'_>,
        declaration: &Declaration,
    ) -> bool {
        let id = declaration.global.id;
        match self {
            OutputFile::Index => {
                generator.stylex_theme.as_ref().is_none_or(|theme| !theme.definitions.contains(&id))
            }
            OutputFile::StyleXTheme(theme) => theme.declarations.contains(&id),
        }
    }

    /// A theme file exports only the theme definitions; the values they read stay private to it,
    /// while the module's own file re-exports the definitions.
    fn exports(self, generator: &Generator<'_>, declaration: &Declaration) -> bool {
        match self {
            OutputFile::StyleXTheme(theme) => theme.definitions.contains(&declaration.global.id),
            OutputFile::Index => declaration.exported && self.renders_value(generator, declaration),
        }
    }

    pub(super) fn declaration_is_inline_exported(
        self,
        generator: &Generator<'_>,
        declaration: &Declaration,
    ) -> bool {
        let local = generator.global_name(declaration.global.id);
        self.exports(generator, declaration) && local == declaration.global.item_name
    }
}
