//! Module-level references and lazy binding requirements.

use functional::initializers::cyclic_initializers;
use functional::tree::{DeclarationKind, ExpressionKind, Global, GlobalId, Module};
use itertools::Itertools;
use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    collect_expression_globals, collect_expression_references, global_file, is_abstraction,
};

pub(super) fn collect_module_references(module: &Module) -> Vec<Global> {
    let mut seen = FxHashSet::default();
    let mut globals = Vec::new();
    for declaration in module.declarations.iter() {
        let DeclarationKind::Value(expression) = declaration.kind else {
            continue;
        };
        collect_expression_references(module, expression, &mut seen, &mut globals);
    }
    globals.into_iter().filter(|global| global_file(global.id) != module.file_id).collect()
}

pub(super) fn cyclic_instance_initializers(module: &Module) -> FxHashSet<GlobalId> {
    let initializers =
        module.declarations.iter().filter_map(|declaration| match declaration.kind {
            DeclarationKind::Value(expression)
                if !is_abstraction(&module.storage[expression].kind) =>
            {
                Some((declaration.global.id, expression))
            }
            DeclarationKind::Value(_)
            | DeclarationKind::Constructor { .. }
            | DeclarationKind::Foreign => None,
        });
    let initializers = initializers.collect_vec();

    let initializer_positions =
        initializers.iter().enumerate().map(|(position, (global_id, _))| (*global_id, position));
    let initializer_positions = initializer_positions.collect::<FxHashMap<_, _>>();

    let mut initializer_dependencies = vec![Vec::new(); initializers.len()];
    for (position, (_, expression)) in initializers.iter().enumerate() {
        let mut globals = FxHashSet::default();
        collect_expression_globals(module, *expression, true, &mut globals);
        let referenced_initializer_positions =
            globals.into_iter().filter_map(|global| initializer_positions.get(&global).copied());
        initializer_dependencies[position].extend(referenced_initializer_positions);
        initializer_dependencies[position].sort_unstable();
        initializer_dependencies[position].dedup();
    }

    let cyclic_positions = cyclic_initializers(&initializer_dependencies);
    let cyclic_initializers = initializers
        .iter()
        .zip(cyclic_positions)
        .filter_map(|((global_id, _), cyclic)| cyclic.then_some(*global_id))
        .collect::<FxHashSet<_>>();

    let mut global_ids = cyclic_initializers.iter();
    let all_initializers_are_instances =
        global_ids.all(|global_id| matches!(global_id, GlobalId::Instance(_)));
    if all_initializers_are_instances { cyclic_initializers } else { FxHashSet::default() }
}

pub(super) fn has_local_lazy_initializers(module: &Module) -> bool {
    module.storage.expressions().any(|(_, expression)| {
        matches!(
            &expression.kind,
            ExpressionKind::Let { recursive: true, bindings, .. }
                if !bindings
                    .iter()
                    .all(|binding| is_abstraction(&module.storage[binding.expression].kind))
        )
    })
}
