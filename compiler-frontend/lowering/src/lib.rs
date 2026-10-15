#[macro_use]
mod recover;

mod algorithm;

pub mod error;
pub mod literal;
pub mod references;
pub mod scope;
pub mod source;
pub mod tree;

use std::hash::Hash;
use std::slice;
use std::sync::Arc;

pub use error::*;
pub use literal::StringLiteral;
pub use references::*;
pub use scope::*;
pub use source::*;
pub use tree::*;

use files::FileId;
use indexing::{
    DeriveItemId, IndexedModule, InstanceItemId, InstanceSourceItemId, TermItemId, TypeItemId,
};
use petgraph::algo::tarjan_scc;
use petgraph::prelude::DiGraphMap;
use resolving::ResolvedModule;
use rustc_hash::{FxBuildHasher, FxHashSet};
use stabilizing::StabilizedModule;
use syntax::cst;

#[derive(Debug, PartialEq, Eq)]
pub struct LoweredModule {
    pub tree: LoweredTree,
    pub references: ReferenceIndex,
    pub graph: LoweringGraph,
    pub nodes: LoweringGraphNodes,
    pub term_edges: FxHashSet<(TermItemId, TermItemId)>,
    pub type_edges: FxHashSet<(TypeItemId, TypeItemId)>,
    pub kind_edges: FxHashSet<(TypeItemId, TypeItemId)>,
    pub synonym_edges: FxHashSet<(TypeItemId, TypeItemId)>,
    pub errors: Vec<LoweringError>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GroupedModule {
    pub term_scc: Vec<Scc<TermItemId>>,
    pub type_scc: Vec<Scc<TypeItemId>>,
    pub instance_items: Vec<InstanceItemId>,
    pub derive_items: Vec<DeriveItemId>,
    pub instance_sources: Vec<InstanceSourceItemId>,
    pub cycle_errors: Vec<LoweringError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scc<T> {
    /// Non-recursive
    Base(T),
    /// Recursive
    Recursive(T),
    /// Mutually-recursive
    Mutual(Vec<T>),
}

impl<T> Scc<T> {
    pub fn as_slice(&self) -> &[T] {
        match self {
            Scc::Base(item) | Scc::Recursive(item) => slice::from_ref(item),
            Scc::Mutual(items) => items.as_slice(),
        }
    }

    pub fn is_recursive(&self) -> bool {
        matches!(self, Scc::Recursive(..) | Scc::Mutual(..))
    }
}

pub fn lower_module(
    file_id: FileId,
    source: &str,
    module: &cst::Module,
    prim: &ResolvedModule,
    stabilized: &StabilizedModule,
    indexed: &IndexedModule,
    resolved: &ResolvedModule,
) -> LoweredModule {
    let algorithm::State {
        tree,
        graph,
        nodes,
        term_edges,
        type_edges,
        kind_edges,
        synonym_edges,
        errors,
        ..
    } = algorithm::lower_module(file_id, source, module, prim, stabilized, indexed, resolved);

    let references = references::index_references(indexed, &tree);

    LoweredModule {
        tree,
        references,
        graph,
        nodes,
        term_edges,
        type_edges,
        kind_edges,
        synonym_edges,
        errors,
    }
}

pub fn group_module(indexed: &IndexedModule, lowered: &LoweredModule) -> GroupedModule {
    let term_nodes = || indexed.items.iter_terms().map(|(id, _)| id);
    let type_nodes = || indexed.items.iter_types().map(|(id, _)| id);

    let term_scc = compute_scc(term_nodes(), &lowered.term_edges);
    let type_scc = compute_scc(type_nodes(), &lowered.type_edges);
    let instance_items = indexed.items.iter_instances().map(|(id, _)| id).collect();
    let derive_items = indexed.items.iter_derives().map(|(id, _)| id).collect();
    let instance_sources = indexed.items.instance_sources().to_vec();

    let kind_cycles = find_cycles(type_nodes(), &lowered.kind_edges);
    let synonym_cycles = find_cycles(type_nodes(), &lowered.synonym_edges);

    let kind_cycles = kind_cycles
        .into_iter()
        .map(|group| LoweringError::RecursiveKinds(RecursiveGroup { group }));

    let synonym_cycles = synonym_cycles
        .into_iter()
        .map(|group| LoweringError::RecursiveSynonym(RecursiveGroup { group }));

    let cycle_errors = kind_cycles.chain(synonym_cycles).collect();
    GroupedModule {
        term_scc,
        type_scc,
        instance_items,
        derive_items,
        instance_sources,
        cycle_errors,
    }
}

fn compute_scc<N>(nodes: impl Iterator<Item = N>, edges: &FxHashSet<(N, N)>) -> Vec<Scc<N>>
where
    N: Copy + Ord + Hash,
{
    let graph = build_graph(nodes, edges);
    tarjan_scc(&graph).into_iter().map(|scc| into_scc(&graph, scc)).collect()
}

fn find_cycles<N>(nodes: impl Iterator<Item = N>, edges: &FxHashSet<(N, N)>) -> Vec<Arc<[N]>>
where
    N: Copy + Ord + Hash,
{
    let graph = build_graph(nodes, edges);

    let components = tarjan_scc(&graph).into_iter().filter_map(|scc| match scc.as_slice() {
        [single] if !graph.contains_edge(*single, *single) => None,
        _ => Some(Arc::from(scc)),
    });

    components.collect()
}

fn build_graph<N>(
    nodes: impl Iterator<Item = N>,
    edges: &FxHashSet<(N, N)>,
) -> DiGraphMap<N, (), FxBuildHasher>
where
    N: Copy + Ord + Hash,
{
    let mut graph = DiGraphMap::default();
    for node in nodes {
        graph.add_node(node);
    }
    for &(from, to) in edges {
        graph.add_edge(from, to, ());
    }
    graph
}

fn into_scc<N>(graph: &DiGraphMap<N, (), FxBuildHasher>, scc: Vec<N>) -> Scc<N>
where
    N: Copy + Ord + Hash,
{
    match scc[..] {
        [single] if !graph.contains_edge(single, single) => Scc::Base(single),
        [single] => Scc::Recursive(single),
        _ => Scc::Mutual(scc),
    }
}
