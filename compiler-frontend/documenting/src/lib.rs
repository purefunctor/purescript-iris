mod annotation;

use std::sync::Arc;

use rustc_hash::FxHashMap;

use indexing::{DeriveItemId, InstanceItemId, TermItemId, TypeItemId};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DocumentedModule {
    pub documentation: String,
    pub terms: FxHashMap<TermItemId, String>,
    pub types: FxHashMap<TypeItemId, String>,
    pub instances: FxHashMap<InstanceItemId, String>,
    pub derives: FxHashMap<DeriveItemId, String>,
}

pub fn document_module(
    source: &str,
    parsed: &parsing::ParsedModule,
    stabilized: &stabilizing::StabilizedModule,
    indexed: &indexing::IndexedModule,
) -> Arc<DocumentedModule> {
    let root = parsed.syntax_node();

    let annotations = annotation::Annotations::new(source, &root);
    let documentation = annotation::module_documentation(source, parsed);

    let terms = indexed.items.iter_terms().map(|(id, item)| {
        let documentation = annotation::term_documentation(stabilized, &annotations, item);
        (id, documentation)
    });
    let terms = terms.collect();

    let types = indexed.items.iter_types().map(|(id, item)| {
        let documentation = annotation::type_documentation(stabilized, &annotations, item);
        (id, documentation)
    });
    let types = types.collect();

    let instances = indexed.items.iter_instances().map(|(id, item)| {
        let documentation = annotation::instance_documentation(stabilized, &annotations, item.id);
        (id, documentation)
    });
    let instances = instances.collect();

    let derives = indexed.items.iter_derives().map(|(id, item)| {
        let documentation = annotation::derive_documentation(stabilized, &annotations, item.id);
        (id, documentation)
    });
    let derives = derives.collect();

    Arc::new(DocumentedModule { documentation, terms, types, instances, derives })
}
