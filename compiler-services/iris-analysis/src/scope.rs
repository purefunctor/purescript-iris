use checking::core::Name;
use checking::tree::{
    DeclarationAbstraction, EquationSource, InstanceDeclaration, InstanceImplementation,
    InstanceMember, TermDeclarationKind,
};
use checking::{CheckedModule, Type, TypeId};
use indexing::IndexedModule;
use lowering::{
    GraphNode, GraphNodeId, ImplicitTypeVariable, InstanceItem, InstanceMemberGroup, LoweredModule,
    TermItemKind, TypeKind, TypeVariableResolution,
};
use rustc_hash::FxHashMap;
use smol_str::SmolStr;

use crate::AnalyzerQueries;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RigidScopeStatus {
    Active(SmolStr),
    Shadowed(SmolStr),
    NotInScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LexicalTypeBinding {
    source_name: SmolStr,
    resolution: TypeVariableResolution,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModuleTypeScopes {
    rigids: FxHashMap<Name, LexicalTypeBinding>,
}

impl ModuleTypeScopes {
    pub fn build(
        queries: &impl AnalyzerQueries,
        indexed: &IndexedModule,
        lowered: &LoweredModule,
        checked: &CheckedModule,
    ) -> ModuleTypeScopes {
        let mut rigids = FxHashMap::default();

        for (term_item_id, _) in indexed.items.iter_terms() {
            let Some(declaration_id) = checked.tree.lookup_term(term_item_id) else {
                continue;
            };
            let TermDeclarationKind::Value(value_declaration) = &checked.tree[declaration_id].kind
            else {
                continue;
            };
            if let Some(TermItemKind::Value { signature: Some(signature_id), .. }) =
                lowered.tree.get_term_item_kind(term_item_id)
            {
                collect_leading_forall_rigids(
                    queries,
                    lowered,
                    checked,
                    *signature_id,
                    &value_declaration.abstractions,
                    &mut rigids,
                );
            }
        }

        for (instance_item_id, _) in indexed.items.iter_instances() {
            let Some(declaration_id) = checked.tree.lookup_instance(instance_item_id) else {
                continue;
            };
            let Some(instance_item) = lowered.tree.get_instance_item(instance_item_id) else {
                continue;
            };
            let declaration = &checked.tree[declaration_id];
            let TermDeclarationKind::Instance(instance_declaration) = &declaration.kind else {
                continue;
            };
            collect_instance_head_rigids(
                queries,
                lowered,
                checked,
                instance_item,
                declaration.type_id,
                instance_declaration,
                &mut rigids,
            );
            if let InstanceImplementation::Members(members) = &instance_declaration.implementation {
                for member in members.iter() {
                    let signature_id = find_instance_member_group(instance_item, member)
                        .and_then(|group| group.signature);
                    if let Some(signature_id) = signature_id {
                        collect_leading_forall_rigids(
                            queries,
                            lowered,
                            checked,
                            signature_id,
                            &member.abstractions,
                            &mut rigids,
                        );
                    }
                }
            }
        }

        for (group_id, _) in lowered.tree.iter_let_binding_groups() {
            let Some(signature_id) =
                lowered.tree.get_let_binding(group_id).and_then(|binding| binding.signature)
            else {
                continue;
            };
            let Some(local_id) = checked.tree.lookup_let(group_id) else {
                continue;
            };
            let local_declaration = &checked.tree[local_id];
            collect_leading_forall_rigids(
                queries,
                lowered,
                checked,
                signature_id,
                &local_declaration.value.abstractions,
                &mut rigids,
            );
        }

        ModuleTypeScopes { rigids }
    }

    pub fn classify_rigid(
        &self,
        lowered: &LoweredModule,
        scope_node: Option<GraphNodeId>,
        name: Name,
    ) -> RigidScopeStatus {
        let Some(binding) = self.rigids.get(&name) else {
            return RigidScopeStatus::NotInScope;
        };
        let Some(scope_node) = scope_node else {
            return RigidScopeStatus::NotInScope;
        };
        match lowered.graph.resolve_type(scope_node, &binding.source_name) {
            Some(resolution) if resolution == binding.resolution => {
                RigidScopeStatus::Active(SmolStr::clone(&binding.source_name))
            }
            Some(_) => RigidScopeStatus::Shadowed(SmolStr::clone(&binding.source_name)),
            None => RigidScopeStatus::NotInScope,
        }
    }

    pub fn name_in_scope(
        &self,
        lowered: &LoweredModule,
        scope_node: Option<GraphNodeId>,
        name: &str,
    ) -> bool {
        scope_node.is_some_and(|node| lowered.graph.resolve_type(node, name).is_some())
    }
}

fn collect_leading_forall_rigids(
    queries: &impl AnalyzerQueries,
    lowered: &LoweredModule,
    checked: &CheckedModule,
    signature_id: lowering::TypeId,
    abstractions: &[DeclarationAbstraction],
    rigids: &mut FxHashMap<Name, LexicalTypeBinding>,
) {
    let mut rigid_by_binder_name = FxHashMap::default();
    for abstraction in abstractions {
        let DeclarationAbstraction::Type { binder, rigid } = *abstraction else {
            continue;
        };
        let binder = queries.lookup_forall_binder(binder);
        if let Type::Rigid(rigid_name, _, _) = *queries.lookup_type(rigid) {
            rigid_by_binder_name.insert(binder.name, rigid_name);
        }
    }

    let mut current = Some(signature_id);
    while let Some(type_id) = current {
        let Some(TypeKind::Forall { bindings, inner }) = lowered.tree.get_type_kind(type_id) else {
            break;
        };
        let Some(mut core_type) = checked.node_types.types.get(type_id).copied() else {
            break;
        };
        for binding in bindings.iter() {
            let Type::Forall(binder_id, next_core) = *queries.lookup_type(core_type) else {
                break;
            };
            let binder = queries.lookup_forall_binder(binder_id);
            if let Some(source_name) = &binding.name
                && let Some(&rigid_name) = rigid_by_binder_name.get(&binder.name)
            {
                rigids.insert(
                    rigid_name,
                    LexicalTypeBinding {
                        source_name: SmolStr::clone(source_name),
                        resolution: TypeVariableResolution::Forall(binding.id),
                    },
                );
            }
            core_type = next_core;
        }
        current = *inner;
    }
}

fn collect_instance_head_rigids(
    queries: &impl AnalyzerQueries,
    lowered: &LoweredModule,
    checked: &CheckedModule,
    instance_item: &InstanceItem,
    instance_signature: TypeId,
    instance_declaration: &InstanceDeclaration,
    rigids: &mut FxHashMap<Name, LexicalTypeBinding>,
) {
    let mut rigid_by_signature_name = FxHashMap::default();
    let mut current = instance_signature;
    let mut index = 0;
    while let Type::Forall(binder_id, inner) = *queries.lookup_type(current) {
        let binder = queries.lookup_forall_binder(binder_id);
        if let Some(&rigid) = instance_declaration.rigid_parameters.get(index)
            && let Type::Rigid(rigid_name, _, _) = *queries.lookup_type(rigid)
        {
            rigid_by_signature_name.insert(binder.name, rigid_name);
        }
        current = inner;
        index += 1;
    }

    let head_types = instance_item.arguments.iter().chain(instance_item.constraints.iter());
    let implicit_node = head_types.into_iter().find_map(|&type_id| {
        let node = lowered.nodes.type_node(type_id)?;
        lowered.graph.traverse(node).find_map(|(node_id, graph_node)| {
            matches!(graph_node, GraphNode::Implicit { .. }).then_some(node_id)
        })
    });

    if let Some(node_id) = implicit_node
        && let GraphNode::Implicit { bindings, .. } = &lowered.graph[node_id]
    {
        for (source_name, type_ids) in bindings.iter() {
            let Some(binding_id) = bindings.get(source_name) else {
                continue;
            };
            let signature_name = type_ids.iter().find_map(|&type_id| {
                let core_type = checked.node_types.types.get(type_id).copied()?;
                match *queries.lookup_type(core_type) {
                    Type::Rigid(name, _, _) => Some(name),
                    _ => None,
                }
            });
            if let Some(signature_name) = signature_name
                && let Some(&rigid_name) = rigid_by_signature_name.get(&signature_name)
            {
                let resolution = TypeVariableResolution::Implicit(ImplicitTypeVariable {
                    binding: false,
                    node: node_id,
                    id: binding_id,
                });
                rigids.insert(
                    rigid_name,
                    LexicalTypeBinding { source_name: SmolStr::new(source_name), resolution },
                );
            }
        }
    }
}

fn find_instance_member_group<'a>(
    instance_item: &'a InstanceItem,
    member: &InstanceMember,
) -> Option<&'a InstanceMemberGroup> {
    let first_equation_source = member.equations.first().map(|equation| equation.source);
    let exact = instance_item.members.iter().find(|group| {
        if group.resolution != Some(member.resolution) {
            return false;
        }
        let group_first =
            group.equations.first().and_then(|equation| equation.source).map(EquationSource::Item);
        group_first == first_equation_source
    });
    exact.or_else(|| {
        instance_item.members.iter().find(|group| group.resolution == Some(member.resolution))
    })
}
