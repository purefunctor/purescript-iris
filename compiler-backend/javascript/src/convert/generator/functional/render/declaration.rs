use functional::tree::{Declaration, DeclarationKind, ExpressionKind, GlobalId};
use rustc_hash::FxHashSet;

use crate::error::ModuleResult;
use crate::writer::BindingCallTarget;

use super::function::render_named_function;
use super::inline::is_abstraction;
use super::syntax::constructor_expression;
use super::tail_call::render_global_tail_call_group;
use super::{Destination, FunctionContext, ModuleRenderer, declaration_source_name};

const INITIALIZER_CYCLE_MESSAGE: &str = "Top-level value initializer cycle";

pub(super) fn render_constructors(renderer: &mut ModuleRenderer<'_, '_, '_>) {
    let ModuleRenderer { generator, tree, writer, file } = renderer;
    let mut rendered = false;
    for declaration in generator.module.declarations.iter() {
        let DeclarationKind::Constructor { arity } = declaration.kind else {
            continue;
        };
        let _origin = tree.enter_origin(declaration.origin);
        let name = generator.global_name(declaration.global.id);
        let exported = file.declaration_is_inline_exported(generator, declaration);
        let expression = constructor_expression(tree, &declaration.global.item_name, arity);
        writer.constant(tree, name, expression, exported);
        rendered = true;
    }
    if rendered {
        writer.blank();
    }
}

pub(super) fn render_source_functions(
    renderer: &mut ModuleRenderer<'_, '_, '_>,
) -> ModuleResult<()> {
    let ModuleRenderer { generator, tree, writer, file } = renderer;
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
                    generator,
                    tree,
                    writer,
                    *file,
                    &generator.global_tail_call_groups[position],
                )?;
                writer.blank();
            }
            continue;
        }
        let _origin = tree.enter_origin(declaration.origin);
        let name = generator.global_name(declaration.global.id);
        let source_name = declaration_source_name(writer, declaration);
        let exported = file.declaration_is_inline_exported(generator, declaration);
        let mut context = FunctionContext::new(&generator.reserved_module_names);
        let mut function_renderer = generator.renderer(tree, writer, &mut context);
        render_named_function(&mut function_renderer, name, source_name, expression, exported)?;
        writer.blank();
    }
    Ok(())
}

pub(super) fn render_foreign_declarations(renderer: &mut ModuleRenderer<'_, '_, '_>) {
    let ModuleRenderer { generator, tree, writer, file } = renderer;
    let Some(foreign_import) = &generator.foreign_import else {
        return;
    };
    let mut rendered = false;
    for declaration in generator.module.declarations.iter() {
        if !matches!(declaration.kind, DeclarationKind::Foreign) {
            continue;
        }
        let _origin = tree.enter_origin(declaration.origin);
        let name = generator.global_name(declaration.global.id);
        let exported = file.declaration_is_inline_exported(generator, declaration);
        let object = tree.identifier(&foreign_import.namespace);
        let index = tree.string(declaration.global.item_name.as_str());
        let access = tree.index(object, index);
        writer.constant(tree, name, access, exported);
        rendered = true;
    }
    if rendered {
        writer.blank();
    }
}

pub(super) fn render_lazy_initializers(
    renderer: &mut ModuleRenderer<'_, '_, '_>,
) -> ModuleResult<()> {
    let ModuleRenderer { generator, tree, writer, .. } = renderer;
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
        let _origin = tree.enter_origin(declaration.origin);
        let name = tree.string(declaration.global.item_name.as_str());
        let runtime = tree.identifier(runtime);
        let binding = tree.member(runtime, "binding");
        let binding = tree.expression(binding);
        let name = tree.expression(name);
        let mut context = FunctionContext::new(&generator.reserved_module_names);
        writer.binding_call(BindingCallTarget::Constant(lazy_name), binding, name, |writer| {
            generator.render_expression(tree, writer, expression, Destination::Return, &mut context)
        })?;
        writer.blank();
    }
    Ok(())
}

pub(super) fn render_value_declarations(
    renderer: &mut ModuleRenderer<'_, '_, '_>,
    declarations: &[(&Declaration, bool)],
) -> ModuleResult<Vec<GlobalId>> {
    let ModuleRenderer { generator, tree, writer, file } = renderer;
    let mut rendered = false;
    let mut previous_was_generated = false;
    let mut initializer_cycle = Vec::new();
    for &(declaration, cyclic) in declarations {
        let DeclarationKind::Value(expression) = declaration.kind else {
            unreachable!("invariant violated: sorted JavaScript declaration is not a value")
        };
        if is_abstraction(&generator.module.storage[expression].kind)
            || !file.renders_value(generator, declaration)
        {
            continue;
        }

        let _origin = tree.enter_origin(declaration.origin);
        let name = generator.global_name(declaration.global.id);
        let exported = file.declaration_is_inline_exported(generator, declaration);
        let generated = matches!(declaration.global.id, GlobalId::Generated(_, _));
        if rendered && (!previous_was_generated || !generated) {
            writer.blank();
        }

        if cyclic {
            initializer_cycle.push(declaration.global.id);
            let source_name = declaration_source_name(writer, declaration);
            writer.constant_iife(name, source_name, exported, |writer| {
                writer.throw_error(INITIALIZER_CYCLE_MESSAGE);
            });
            rendered = true;
            previous_was_generated = generated;
            continue;
        }

        if let Some(lazy_name) = generator.lazy_global_names.get(&declaration.global.id) {
            let lazy = tree.identifier(lazy_name);
            let value = tree.call(lazy, Vec::new());
            writer.constant(tree, name, value, exported);
            rendered = true;
            previous_was_generated = generated;
            continue;
        }

        let mut context = FunctionContext::new(&generator.reserved_module_names);
        if let Some(value) = generator.try_inline_expression(tree, expression, &mut context)? {
            writer.constant(tree, name, value, exported);
        } else {
            let source_name = declaration_source_name(writer, declaration);
            writer.constant_iife(name, source_name, exported, |writer| {
                generator.render_expression(
                    tree,
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
        writer.blank();
    }
    Ok(initializer_cycle)
}
