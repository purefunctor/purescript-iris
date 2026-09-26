mod recursive;

use std::mem;
use std::sync::Arc;

use files::FileId;
use indexing::{
    DeriveItemId, EquationSourceId, IndexedDeriveItem, IndexedInstanceItem, IndexedModule,
    IndexedTermItem, IndexedTermItemKind, IndexedTypeItem, IndexedTypeItemKind, InstanceItemId,
    TermItemId, TypeItemId, TypeRoleId,
};
use indexmap::IndexMap;
use itertools::Itertools;
use petgraph::prelude::DiGraphMap;
use resolving::ResolvedModule;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use smol_str::SmolStr;
use stabilizing::{ExpectId, StabilizedModule};
use syntax::ast::AstNode;
use syntax::cst;

use crate::error::*;
use crate::scope::*;
use crate::source::*;
use crate::tree::*;

#[derive(Default)]
pub(crate) struct State {
    pub(crate) tree: LoweredTree,
    pub(crate) graph: LoweringGraph,
    pub(crate) nodes: LoweringGraphNodes,
    pub(crate) graph_scope: Option<GraphNodeId>,

    pub(crate) current_term: Option<TermItemId>,
    pub(crate) current_type: Option<TypeItemId>,

    pub(crate) current_kind: Option<TypeItemId>,
    pub(crate) current_synonym: Option<TypeItemId>,

    pub(crate) term_edges: FxHashSet<(TermItemId, TermItemId)>,
    pub(crate) type_edges: FxHashSet<(TypeItemId, TypeItemId)>,
    pub(crate) kind_edges: FxHashSet<(TypeItemId, TypeItemId)>,
    pub(crate) synonym_edges: FxHashSet<(TypeItemId, TypeItemId)>,
    let_binding_contexts: Vec<LetBindingContext>,

    pub(crate) in_constraint: bool,

    pub(crate) errors: Vec<LoweringError>,
}

type ItemGraph<T> = DiGraphMap<T, (), FxBuildHasher>;

struct LetBindingContext {
    scope: GraphNodeId,
    current_binding: Option<LetBindingNameGroupId>,
    dependencies: ItemGraph<LetBindingNameGroupId>,
}

struct Context<'c> {
    file_id: FileId,
    root: &'c syntax::SyntaxNode,
    source: &'c str,
    prim: &'c ResolvedModule,
    stabilized: &'c StabilizedModule,
    indexed: &'c IndexedModule,
    resolved: &'c ResolvedModule,
}

impl State {
    fn with_scope<T>(&mut self, mut f: impl FnMut(&mut State) -> T) -> T {
        let graph_scope = self.graph_scope;
        let result = f(self);
        self.graph_scope = graph_scope;
        result
    }

    fn begin_term(&mut self, id: TermItemId) {
        self.current_term = Some(id);
        self.current_type = None;
    }

    fn begin_type(&mut self, id: TypeItemId) {
        self.current_term = None;
        self.current_type = Some(id);
        self.current_synonym = None;
    }

    fn begin_instance_or_derive(&mut self) {
        self.current_term = None;
        self.current_type = None;
        self.current_synonym = None;
    }

    fn begin_synonym(&mut self, id: TypeItemId) {
        self.current_synonym = Some(id);
    }

    fn end_synonym(&mut self) {
        self.current_synonym = None;
    }

    fn begin_kind(&mut self, id: TypeItemId) {
        self.current_kind = Some(id);
    }

    fn end_kind(&mut self) {
        self.current_kind = None;
    }

    fn alloc_let_binding(&mut self, group: LetBindingNameGroup) -> LetBindingNameGroupId {
        self.tree.let_binding_groups.alloc(group)
    }

    fn associate_binder_kind(&mut self, id: BinderId, kind: BinderKind) {
        self.tree.binders.insert(id, kind);
        let Some(node) = self.graph_scope else { return };
        self.nodes.binder_node.insert(id, node);
    }

    fn associate_expression_kind(&mut self, id: ExpressionId, kind: ExpressionKind) {
        self.tree.expressions.insert(id, kind);
        let Some(node) = self.graph_scope else { return };
        self.nodes.expression_node.insert(id, node);
    }

    fn associate_type_kind(&mut self, id: TypeId, kind: TypeKind) {
        self.tree.types.insert(id, kind);
        let Some(node) = self.graph_scope else { return };
        self.nodes.type_node.insert(id, node);
    }

    fn associate_record_pun(
        &mut self,
        id: RecordPunId,
        resolution: Option<TermVariableResolution>,
    ) {
        if let Some(resolution) = resolution {
            self.tree.expression_puns.insert(id, resolution);
        }
        let Some(node) = self.graph_scope else { return };
        self.nodes.record_pun_node.insert(id, node);
    }

    fn associate_do_statement(&mut self, id: DoStatementId, statement: DoStatement) {
        self.tree.do_statements.insert(id, statement);
    }

    fn associate_let_binding_name(&mut self, id: LetBindingNameGroupId, info: LetBindingName) {
        self.tree.let_binding_names.insert(id, info);
        let Some(node) = self.graph_scope else { return };
        self.nodes.let_node.insert(id, node);
    }

    fn insert_binder(&mut self, name: &str, id: BinderId) {
        let Some(node) = self.graph_scope else { return };
        let GraphNode::Binder { binders, .. } = &mut self.graph.inner[node] else { return };

        let name = SmolStr::from(name);
        binders.insert(name, id);
    }

    fn insert_record_pun(&mut self, name: &str, id: RecordPunId) {
        let Some(node) = self.graph_scope else { return };
        let GraphNode::Binder { puns, .. } = &mut self.graph.inner[node] else { return };

        let name = SmolStr::from(name);
        puns.insert(name, id);
        self.nodes.record_pun_node.insert(id, node);
    }

    fn insert_bound_variable(&mut self, name: &str, id: TypeVariableBindingId) {
        let Some(node) = self.graph_scope else { return };
        let GraphNode::Forall { bindings, .. } = &mut self.graph.inner[node] else { return };

        let name = SmolStr::from(name);
        bindings.insert(name, id);
        self.nodes.type_variable_binding_node.insert(id, node);
    }

    fn push_binder_scope(&mut self) -> Option<GraphNodeId> {
        let parent = mem::take(&mut self.graph_scope);
        let binders = FxHashMap::default();
        let puns = FxHashMap::default();
        let id = self.graph.inner.alloc(GraphNode::Binder { parent, binders, puns });
        self.graph_scope.replace(id)
    }

    fn push_forall_scope(&mut self) -> Option<GraphNodeId> {
        let parent = mem::take(&mut self.graph_scope);
        let bindings = FxHashMap::default();
        let id = self.graph.inner.alloc(GraphNode::Forall { parent, bindings });
        self.graph_scope.replace(id)
    }

    fn push_implicit_scope(&mut self) -> Option<GraphNodeId> {
        let parent = mem::take(&mut self.graph_scope);
        let collecting = true;
        let bindings = ImplicitBindings::default();
        let id = self.graph.inner.alloc(GraphNode::Implicit { parent, collecting, bindings });
        self.graph_scope.replace(id)
    }

    fn finish_implicit_scope(&mut self) {
        let Some(id) = self.graph_scope else { return };
        let GraphNode::Implicit { collecting, .. } = &mut self.graph.inner[id] else { return };
        *collecting = false;
    }

    fn resolve_term_full(
        &mut self,
        context: &Context,
        qualifier: Option<&str>,
        name: &str,
    ) -> Option<TermVariableResolution> {
        if qualifier.is_some() {
            self.resolve_term_reference(context, qualifier, name)
                .map(|(file_id, item_id)| TermVariableResolution::Reference(file_id, item_id))
        } else {
            self.resolve_term_local(name).or_else(|| {
                self.resolve_term_reference(context, qualifier, name)
                    .map(|(file_id, item_id)| TermVariableResolution::Reference(file_id, item_id))
            })
        }
    }

    fn resolve_term_reference(
        &mut self,
        context: &Context,
        qualifier: Option<&str>,
        name: &str,
    ) -> Option<(FileId, TermItemId)> {
        let (file_id, term_id) = context.lookup_term(qualifier, name)?;

        if context.file_id == file_id
            && let Some(current_id) = self.current_term
        {
            self.term_edges.insert((current_id, term_id));
        }

        Some((file_id, term_id))
    }

    fn resolve_term_local(&mut self, name: &str) -> Option<TermVariableResolution> {
        let id = self.graph_scope?;
        self.graph.traverse(id).find_map(|(node_id, graph)| match graph {
            GraphNode::Binder { binders, puns, .. } => {
                if let Some(r) = binders.get(name) {
                    return Some(TermVariableResolution::Binder(*r));
                }
                if let Some(r) = puns.get(name) {
                    return Some(TermVariableResolution::RecordPun(*r));
                }
                None
            }
            GraphNode::Let { bindings, .. } => {
                let target_id = *bindings.get(name)?;

                let context =
                    self.let_binding_contexts.iter_mut().find(|context| context.scope == node_id);
                if let Some(context) = context
                    && let Some(source_id) = context.current_binding
                {
                    context.dependencies.add_edge(source_id, target_id, ());
                }

                Some(TermVariableResolution::Let(target_id))
            }
            _ => None,
        })
    }

    fn resolve_type_reference(
        &mut self,
        context: &Context,
        qualifier: Option<&str>,
        name: &str,
    ) -> Option<(FileId, TypeItemId)> {
        let (file_id, type_id) = context.lookup_type(qualifier, name)?;

        if context.file_id == file_id
            && let Some(current_id) = self.current_type
        {
            self.type_edges.insert((current_id, type_id));

            if let Some(synonym_id) = self.current_synonym
                && let IndexedTypeItemKind::Synonym { .. } = context.indexed.items[type_id].kind
            {
                self.synonym_edges.insert((synonym_id, type_id));
            }

            if let Some(kind_id) = self.current_kind {
                self.kind_edges.insert((kind_id, type_id));
            }
        }

        Some((file_id, type_id))
    }

    fn resolve_class_reference(
        &mut self,
        context: &Context,
        qualifier: Option<&str>,
        name: &str,
    ) -> Option<(FileId, TypeItemId)> {
        let (file_id, type_id) = context.lookup_class(qualifier, name)?;

        if context.file_id == file_id
            && let Some(current_id) = self.current_type
        {
            self.type_edges.insert((current_id, type_id));

            if let Some(synonym_id) = self.current_synonym
                && let IndexedTypeItemKind::Synonym { .. } = context.indexed.items[type_id].kind
            {
                self.synonym_edges.insert((synonym_id, type_id));
            }

            if let Some(kind_id) = self.current_kind {
                self.kind_edges.insert((kind_id, type_id));
            }
        }

        Some((file_id, type_id))
    }

    fn resolve_type_variable(&mut self, id: TypeId, name: &str) -> Option<TypeVariableResolution> {
        let node = self.graph_scope?;
        if let GraphNode::Implicit { collecting, bindings, .. } = &mut self.graph.inner[node] {
            if let Some(id) = bindings.get(name) {
                Some(TypeVariableResolution::Implicit(ImplicitTypeVariable {
                    binding: false,
                    node,
                    id,
                }))
            } else if *collecting {
                let id = bindings.bind(name, id);
                Some(TypeVariableResolution::Implicit(ImplicitTypeVariable {
                    binding: true,
                    node,
                    id,
                }))
            } else {
                None
            }
        } else {
            self.graph.traverse(node).find_map(|(node, graph)| match graph {
                GraphNode::Forall { bindings, .. } => {
                    bindings.get(name).copied().map(TypeVariableResolution::Forall)
                }
                GraphNode::Implicit { bindings, .. } => {
                    let id = bindings.get(name)?;
                    Some(TypeVariableResolution::Implicit(ImplicitTypeVariable {
                        binding: false,
                        node,
                        id,
                    }))
                }
                _ => None,
            })
        }
    }
}

impl Context<'_> {
    fn lookup_term<Q, N>(&self, qualifier: Option<Q>, name: N) -> Option<(FileId, TermItemId)>
    where
        Q: AsRef<str>,
        N: AsRef<str>,
    {
        let qualifier = qualifier.as_ref().map(Q::as_ref);
        let name = name.as_ref();
        self.resolved.lookup_term(self.prim, qualifier, name)
    }

    fn lookup_type<Q, N>(&self, qualifier: Option<Q>, name: N) -> Option<(FileId, TypeItemId)>
    where
        Q: AsRef<str>,
        N: AsRef<str>,
    {
        let qualifier = qualifier.as_ref().map(Q::as_ref);
        let name = name.as_ref();
        self.resolved.lookup_type(self.prim, qualifier, name)
    }

    fn lookup_class<Q, N>(&self, qualifier: Option<Q>, name: N) -> Option<(FileId, TypeItemId)>
    where
        Q: AsRef<str>,
        N: AsRef<str>,
    {
        let qualifier = qualifier.as_ref().map(Q::as_ref);
        let name = name.as_ref();
        self.resolved.lookup_class(self.prim, qualifier, name)
    }
}

pub(super) fn lower_module(
    file_id: FileId,
    source: &str,
    module: &cst::Module,
    prim: &ResolvedModule,
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    resolved: &ResolvedModule,
) -> State {
    let mut state = State::default();

    let root = module.syntax();
    let context = Context { file_id, root, source, prim, stabilized, indexed, resolved };

    for (id, item) in context.indexed.items.iter_terms() {
        state.with_scope(|state| {
            state.begin_term(id);
            lower_term_item(state, &context, id, item);
        });
    }

    for (id, item) in context.indexed.items.iter_instances() {
        state.with_scope(|state| {
            state.begin_instance_or_derive();
            lower_instance_item(state, &context, id, item);
        });
    }

    for (id, item) in context.indexed.items.iter_derives() {
        state.with_scope(|state| {
            state.begin_instance_or_derive();
            lower_derive_item(state, &context, id, item);
        });
    }

    for (id, item) in context.indexed.items.iter_types() {
        state.with_scope(|state| {
            state.begin_type(id);
            lower_type_item(state, &context, id, item);
        })
    }

    state
}

fn lower_instance_class_reference(
    state: &mut State,
    context: &Context,
    head: &cst::InstanceHead,
) -> Option<(FileId, TypeItemId)> {
    let qualified = head.qualified()?;
    let (qualifier, name) =
        recursive::lower_qualified_name(context.source, &qualified, cst::QualifiedName::upper)?;
    let resolution = state.resolve_class_reference(context, qualifier.as_deref(), &name);

    if resolution.is_none() {
        let id = context.stabilized.lookup_cst(head).expect_id();
        state.errors.push(LoweringError::NotInScope(NotInScope::TypeClass { id }));
    }

    resolution
}

fn lower_term_item(
    state: &mut State,
    context: &Context,
    item_id: TermItemId,
    item: &IndexedTermItem,
) {
    match &item.kind {
        IndexedTermItemKind::ClassMember { .. } => (), // See lower_type_item

        IndexedTermItemKind::Constructor { .. } => (), // See lower_type_item

        IndexedTermItemKind::Foreign { id } => {
            let cst = context.stabilized.ast_ptr(*id).and_then(|cst| cst.try_to_node(context.root));

            let signature = cst.and_then(|cst| {
                let cst = cst.type_()?;
                Some(recursive::lower_type(state, context, &cst))
            });

            let kind = TermItemKind::Foreign { signature };
            state.tree.term_items.insert(item_id, kind);
        }

        IndexedTermItemKind::Operator { id } => {
            let cst = context.stabilized.ast_ptr(*id).and_then(|cst| cst.try_to_node(context.root));

            let associativity = cst.as_ref().and_then(|cst| {
                cst.infix()
                    .map(|_| Associativity::None)
                    .or_else(|| cst.infixl().map(|_| Associativity::Left))
                    .or_else(|| cst.infixr().map(|_| Associativity::Right))
            });

            let precedence = cst.as_ref().and_then(|cst| {
                let cst = cst.precedence()?;
                cst.text(context.source).parse().ok()
            });

            let resolution = cst.as_ref().and_then(|cst| {
                let cst = cst.qualified()?;
                let (qualifier, name) = None
                    .or_else(|| {
                        recursive::lower_qualified_name(
                            context.source,
                            &cst,
                            cst::QualifiedName::lower,
                        )
                    })
                    .or_else(|| {
                        recursive::lower_qualified_name(
                            context.source,
                            &cst,
                            cst::QualifiedName::upper,
                        )
                    })?;
                state.resolve_term_reference(context, qualifier.as_deref(), &name)
            });

            let kind = TermItemKind::Operator { associativity, precedence, resolution };
            state.tree.term_items.insert(item_id, kind);
        }

        IndexedTermItemKind::Value { signature, equations } => {
            let signature = signature.and_then(|id| {
                let cst =
                    context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))?;
                let cst = cst.signature()?;
                Some(recursive::lower_forall(state, context, &cst))
            });

            let equations = equations
                .iter()
                .filter_map(|id| {
                    let cst = context
                        .stabilized
                        .ast_ptr(*id)
                        .and_then(|cst| cst.try_to_node(context.root))?;
                    Some(recursive::lower_equation_like(
                        state,
                        context,
                        Some(EquationSourceId::Value(*id)),
                        cst,
                        cst::ValueEquation::function_binders,
                        cst::ValueEquation::guarded_expression,
                    ))
                })
                .collect();

            let kind = TermItemKind::Value { signature, equations };
            state.tree.term_items.insert(item_id, kind);
        }
    }
}

fn lower_instance_item(
    state: &mut State,
    context: &Context,
    item_id: InstanceItemId,
    item: &IndexedInstanceItem,
) {
    let cst = context.stabilized.ast_ptr(item.id).and_then(|cst| cst.try_to_node(context.root));

    let resolution = cst.as_ref().and_then(|cst| {
        let head = cst.instance_head()?;
        lower_instance_class_reference(state, context, &head)
    });

    state.push_implicit_scope();
    let arguments = recover! {
        let head = cst.as_ref()?.instance_head()?;
        head.children().map(|cst| recursive::lower_type(state, context, &cst)).collect()
    };

    state.in_constraint = true;
    let constraints = recover! {
        cst.as_ref()?
            .instance_constraints()?
            .children()
            .map(|cst| recursive::lower_type(state, context, &cst))
            .collect()
    };
    state.in_constraint = false;
    state.finish_implicit_scope();

    let members = recover! {
        let statements = cst.as_ref()?.instance_statements()?;
        lower_instance_statements(state, context, &statements, resolution)
    };

    let item = InstanceItem { constraints, resolution, arguments, members };
    state.tree.instance_items.insert(item_id, item);
}

fn lower_derive_item(
    state: &mut State,
    context: &Context,
    item_id: DeriveItemId,
    item: &IndexedDeriveItem,
) {
    let cst = context.stabilized.ast_ptr(item.id).and_then(|cst| cst.try_to_node(context.root));

    let newtype = cst.as_ref().map(|cst| cst.newtype_token().is_some()).unwrap_or(false);
    let resolution = cst.as_ref().and_then(|cst| {
        let head = cst.instance_head()?;
        lower_instance_class_reference(state, context, &head)
    });

    state.push_implicit_scope();
    let arguments = recover! {
        let head = cst.as_ref()?.instance_head()?;
        head.children().map(|cst| recursive::lower_type(state, context, &cst)).collect()
    };

    state.in_constraint = true;
    let constraints = recover! {
        cst.as_ref()?
            .instance_constraints()?
            .children()
            .map(|cst| recursive::lower_type(state, context, &cst))
            .collect()
    };
    state.in_constraint = false;
    state.finish_implicit_scope();

    let item = DeriveItem { newtype, constraints, resolution, arguments };
    state.tree.derive_items.insert(item_id, item);
}

fn lower_type_item(
    state: &mut State,
    context: &Context,
    item_id: TypeItemId,
    item: &IndexedTypeItem,
) {
    match &item.kind {
        IndexedTypeItemKind::Data { signature, equation, role, .. } => {
            state.begin_kind(item_id);

            let signature = signature.and_then(|id| {
                let cst =
                    context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))?;
                state.push_forall_scope();
                cst.type_().map(|t| recursive::lower_forall(state, context, &t))
            });

            state.end_kind();

            let declaration = equation.and_then(|id| {
                let cst =
                    context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))?;

                state.push_forall_scope();
                let variables = cst
                    .type_variables()
                    .map(|t| recursive::lower_type_variable_binding(state, context, &t, true))
                    .collect();

                Some(DataDeclaration { variables })
            });

            let roles = role.map(|id| lower_roles(context, id)).unwrap_or_default();

            let kind = TypeItemKind::Data { signature, declaration, roles };
            state.tree.type_items.insert(item_id, kind);

            lower_constructors(state, context, item_id);
        }

        IndexedTypeItemKind::Newtype { signature, equation, role, .. } => {
            state.begin_kind(item_id);

            let signature = signature.and_then(|id| {
                let cst =
                    context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))?;
                state.push_forall_scope();
                cst.type_().map(|t| recursive::lower_forall(state, context, &t))
            });

            state.end_kind();

            let declaration = equation.and_then(|id| {
                let cst =
                    context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))?;

                state.push_forall_scope();
                let variables = cst
                    .type_variables()
                    .map(|t| recursive::lower_type_variable_binding(state, context, &t, true))
                    .collect();

                Some(NewtypeDeclaration { variables })
            });

            let roles = role.map(|id| lower_roles(context, id)).unwrap_or_default();

            let kind = TypeItemKind::Newtype { signature, declaration, roles };
            state.tree.type_items.insert(item_id, kind);

            lower_constructors(state, context, item_id);
        }

        IndexedTypeItemKind::Synonym { signature, equation } => {
            state.begin_kind(item_id);

            let signature = signature.and_then(|id| {
                let cst =
                    context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))?;
                state.push_forall_scope();
                cst.type_().map(|t| recursive::lower_forall(state, context, &t))
            });

            state.end_kind();

            state.begin_synonym(item_id);

            let declaration = equation.and_then(|id| {
                let cst =
                    context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))?;

                state.push_forall_scope();
                let variables = cst
                    .children()
                    .map(|cst| recursive::lower_type_variable_binding(state, context, &cst, false))
                    .collect();

                let type_ = cst.type_().map(|cst| recursive::lower_type(state, context, &cst));

                Some(TypeSynonymDeclaration { variables, type_ })
            });

            state.end_synonym();

            let kind = TypeItemKind::Synonym { signature, declaration };
            state.tree.type_items.insert(item_id, kind);
        }

        IndexedTypeItemKind::Class { signature, declaration, .. } => {
            state.begin_kind(item_id);

            let signature = signature.and_then(|id| {
                let cst =
                    context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))?;
                state.push_forall_scope();
                cst.type_().map(|t| recursive::lower_forall(state, context, &t))
            });

            state.end_kind();

            let declaration = declaration.and_then(|id| {
                let cst =
                    context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))?;

                state.push_forall_scope();
                let variables: Arc<[_]> = recover! {
                    cst.class_head()?
                        .children()
                        .map(|cst| recursive::lower_type_variable_binding(state, context, &cst, true))
                        .collect()
                };

                state.in_constraint = true;
                let constraints = recover! {
                    cst.class_constraints()?
                        .children()
                        .map(|cst| recursive::lower_type(state, context, &cst))
                        .collect()
                };
                state.in_constraint = false;

                let variable_map: FxHashMap<&str, u8> = variables
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.name.as_deref().map(|n| (n, i as u8)))
                    .collect();

                let functional_dependencies = recover! {
                    let dependencies = cst
                        .class_functional_dependencies()?
                        .children()
                        .map(|dependency| {
                            lower_functional_dependency(
                                state,
                                context,
                                &variable_map,
                                &dependency,
                            )
                        });
                    dependencies.collect()
                };

                Some(ClassDeclaration { constraints, variables, functional_dependencies })
            });

            let kind = TypeItemKind::Class { signature, declaration };
            state.tree.type_items.insert(item_id, kind);

            lower_class_members(state, context, item_id);
        }

        IndexedTypeItemKind::Foreign { id, role } => {
            state.begin_kind(item_id);

            let cst = context.stabilized.ast_ptr(*id).and_then(|cst| cst.try_to_node(context.root));

            let signature = cst.as_ref().and_then(|cst| {
                let cst = cst.type_()?;
                Some(recursive::lower_type(state, context, &cst))
            });

            state.end_kind();

            let roles = role.map(|id| lower_roles(context, id)).unwrap_or_default();

            let kind = TypeItemKind::Foreign { signature, roles };
            state.tree.type_items.insert(item_id, kind);
        }

        IndexedTypeItemKind::Operator { id } => {
            let cst = context.stabilized.ast_ptr(*id).and_then(|cst| cst.try_to_node(context.root));

            let associativity = cst.as_ref().and_then(|cst| {
                cst.infix()
                    .map(|_| Associativity::None)
                    .or_else(|| cst.infixl().map(|_| Associativity::Left))
                    .or_else(|| cst.infixr().map(|_| Associativity::Right))
            });

            let precedence = cst.as_ref().and_then(|cst| {
                let cst = cst.precedence()?;
                cst.text(context.source).parse().ok()
            });

            state.begin_kind(item_id);

            let resolution = cst.as_ref().and_then(|cst| {
                let cst = cst.qualified()?;
                let (qualifier, name) = recursive::lower_qualified_name(
                    context.source,
                    &cst,
                    cst::QualifiedName::upper,
                )?;
                state.resolve_type_reference(context, qualifier.as_deref(), &name)
            });

            state.end_kind();

            let kind = TypeItemKind::Operator { associativity, precedence, resolution };
            state.tree.type_items.insert(item_id, kind);
        }
    }
}

fn lower_constructors(state: &mut State, context: &Context, id: TypeItemId) {
    for item_id in context.indexed.data_constructors(id) {
        let IndexedTermItemKind::Constructor { id, .. } = context.indexed.items[item_id].kind
        else {
            unreachable!("invariant violated: expected IndexedTermItemKind::Constructor");
        };

        let Some(cst) =
            context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))
        else {
            continue;
        };

        let arguments = cst.children().map(|t| recursive::lower_type(state, context, &t)).collect();

        let kind = TermItemKind::Constructor { arguments };
        state.tree.term_items.insert(item_id, kind);
    }
}

fn lower_class_members(state: &mut State, context: &Context, id: TypeItemId) {
    for item_id in context.indexed.class_members(id) {
        let IndexedTermItemKind::ClassMember { id, .. } = context.indexed.items[item_id].kind
        else {
            unreachable!("invariant violated: expected IndexedTermItemKind::ClassMember");
        };

        let Some(cst) =
            context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root))
        else {
            continue;
        };

        let signature = cst.type_().map(|t| recursive::lower_type(state, context, &t));

        let kind = TermItemKind::ClassMember { signature };
        state.tree.term_items.insert(item_id, kind);
    }
}

fn lower_instance_statements(
    state: &mut State,
    context: &Context,
    cst: &cst::InstanceStatements,
    class_resolution: Option<(FileId, TypeItemId)>,
) -> Arc<[InstanceMemberGroup]> {
    let children = cst.children().chunk_by(|statement| match statement {
        cst::InstanceMemberStatement::InstanceSignatureStatement(s) => s.name_token().map(|t| {
            let text = t.text(context.source);
            SmolStr::from(text)
        }),
        cst::InstanceMemberStatement::InstanceEquationStatement(e) => e.name_token().map(|t| {
            let text = t.text(context.source);
            SmolStr::from(text)
        }),
    });

    let mut in_scope: IndexMap<_, _, FxBuildHasher> = IndexMap::default();
    for (name, mut children) in children.into_iter() {
        let mut statements = Vec::new();
        let mut signature = None;
        let mut equations = Vec::new();

        if let Some(statement) = children.next() {
            let id = context.stabilized.lookup_cst(&statement).expect_id();
            statements.push(id);
            match statement {
                cst::InstanceMemberStatement::InstanceSignatureStatement(cst) => {
                    let id = context.stabilized.lookup_cst(&cst).expect_id();
                    signature = Some(id);
                }
                cst::InstanceMemberStatement::InstanceEquationStatement(cst) => {
                    let id = context.stabilized.lookup_cst(&cst).expect_id();
                    equations.push(id);
                }
            }
        }

        children.for_each(|statement| {
            let id = context.stabilized.lookup_cst(&statement).expect_id();
            statements.push(id);
            if let cst::InstanceMemberStatement::InstanceEquationStatement(cst) = statement {
                let id = context.stabilized.lookup_cst(&cst).expect_id();
                equations.push(id);
            }
        });

        if let Some(name) = name {
            in_scope.insert(name, (statements, signature, equations));
        }
    }

    let member_groups = in_scope.into_iter().map(|(name, (statements, signature, equations))| {
        // Resolve the class member using the class type ID
        let resolution = class_resolution.and_then(|(class_file, class_id)| {
            context.resolved.lookup_class_member(class_file, class_id, &name)
        });
        let statements: Arc<[InstanceMemberId]> = Arc::from(statements);

        state.with_scope(|state| {
            state.push_forall_scope();
            let signature = signature.and_then(|id| {
                let cst = context.stabilized.ast_ptr(id)?.try_to_node(context.root)?;
                cst.type_().map(|t| recursive::lower_forall(state, context, &t))
            });
            let equations = equations
                .iter()
                .filter_map(|&id| {
                    let cst = context.stabilized.ast_ptr(id)?.try_to_node(context.root)?;
                    Some(recursive::lower_equation_like(
                        state,
                        context,
                        Some(EquationSourceId::Instance(id)),
                        cst,
                        cst::InstanceEquationStatement::function_binders,
                        cst::InstanceEquationStatement::guarded_expression,
                    ))
                })
                .collect();
            InstanceMemberGroup { statements: statements.clone(), resolution, signature, equations }
        })
    });
    member_groups.collect()
}

fn lower_roles(context: &Context, id: TypeRoleId) -> Arc<[Role]> {
    let cst = context.stabilized.ast_ptr(id).and_then(|cst| cst.try_to_node(context.root));
    cst.map(|cst| {
        cst.children()
            .map(|cst| {
                if cst.nominal().is_some() {
                    Role::Nominal
                } else if cst.representational().is_some() {
                    Role::Representational
                } else if cst.phantom().is_some() {
                    Role::Phantom
                } else {
                    Role::Unknown
                }
            })
            .collect()
    })
    .unwrap_or_default()
}

fn lower_functional_dependency(
    state: &mut State,
    context: &Context,
    variable_map: &FxHashMap<&str, u8>,
    cst: &cst::FunctionalDependency,
) -> FunctionalDependency {
    match cst {
        cst::FunctionalDependency::FunctionalDependencyDetermined(dependency) => {
            let determined = dependency.children().filter_map(|variable| {
                lower_functional_dependency_variable(state, context, variable_map, variable)
            });
            let determined = determined.collect();
            FunctionalDependency { determiners: Arc::from([]), determined }
        }
        cst::FunctionalDependency::FunctionalDependencyDetermines(dependency) => {
            let determiners = dependency.determiners().filter_map(|variable| {
                lower_functional_dependency_variable(state, context, variable_map, variable)
            });
            let determiners = determiners.collect();
            let determined = dependency.determined().filter_map(|variable| {
                lower_functional_dependency_variable(state, context, variable_map, variable)
            });
            let determined = determined.collect();
            FunctionalDependency { determiners, determined }
        }
    }
}

fn lower_functional_dependency_variable(
    state: &mut State,
    context: &Context,
    variable_map: &FxHashMap<&str, u8>,
    variable: cst::TypeVariable,
) -> Option<u8> {
    let name = variable.name_token()?;
    let variable = cst::Type::cast(variable.syntax().clone())?;
    recursive::lower_type(state, context, &variable);
    variable_map.get(name.text(context.source)).copied()
}
