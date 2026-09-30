use la_arena::RawIdx;

use crate::{IndexedModule, InstanceId, TermItemId, index_module};

fn index(source: &str) -> IndexedModule {
    let lexed = lexing::lex(source);
    let tokens = lexing::layout(&lexed);
    let (parsed, errors) = parsing::parse(&lexed, &tokens);
    assert!(errors.is_empty(), "test module must parse: {errors:?}");

    let root = parsed.syntax_node();
    let stabilized = stabilizing::stabilize_module(&root);
    index_module(source, &parsed.cst(), &stabilized)
}

fn instance_id(indexed: &IndexedModule, name: &str) -> InstanceId {
    let mut instances = indexed.items.iter_instances();
    let (_, instance) =
        instances.find(|(_, instance)| instance.name.as_deref() == Some(name)).unwrap();
    instance.id
}

#[test]
fn constructor_owners_preserve_type_and_constructor_ids() {
    let indexed = index(
        "module Main where\n\
         data First = FirstA | FirstB\n\
         newtype Second = Second Int\n\
         value = 0\n",
    );

    let first_type = indexed.names.types.lookup("First").unwrap();
    let second_type = indexed.names.types.lookup("Second").unwrap();
    let first_a = indexed.names.terms.lookup("FirstA").unwrap();
    let first_b = indexed.names.terms.lookup("FirstB").unwrap();
    let second = indexed.names.terms.lookup("Second").unwrap();
    let value = indexed.names.terms.lookup("value").unwrap();

    let first_constructors = indexed.data_constructors(first_type);
    let first_constructors = first_constructors.collect::<Vec<_>>();
    assert_eq!(first_constructors, [first_a, first_b]);
    assert_eq!(indexed.data_constructors(second_type).next(), Some(second));
    assert_eq!(indexed.constructor_type(first_a), Some(first_type));
    assert_eq!(indexed.constructor_type(first_b), Some(first_type));
    assert_eq!(indexed.constructor_type(second), Some(second_type));
    assert_eq!(indexed.constructor_type(value), None);
    let out_of_range = TermItemId::from_raw(RawIdx::from_u32(u32::MAX));
    assert_eq!(indexed.constructor_type(out_of_range), None);
}

#[test]
fn constructor_owner_requires_attachment_to_the_recovered_type() {
    let indexed = index(
        "module Main where\n\
         type Mismatch = Int\n\
         data Mismatch = Constructor\n",
    );

    let mismatch = indexed.names.types.lookup("Mismatch").unwrap();
    let constructor = indexed.names.terms.lookup("Constructor").unwrap();

    assert_eq!(indexed.data_constructors(mismatch).next(), None);
    assert_eq!(indexed.constructor_type(constructor), None);
}

#[test]
fn constructor_owner_requires_attachment_to_the_recovered_class() {
    let indexed = index(
        "module Main where\n\
         class Clash a\n\
         data Clash = Constructor\n",
    );

    let clash = indexed.names.types.lookup("Clash").unwrap();
    let constructor = indexed.names.terms.lookup("Constructor").unwrap();

    assert_eq!(indexed.data_constructors(clash).next(), None);
    assert_eq!(indexed.constructor_type(constructor), None);
}

#[test]
fn instance_chain_metadata_preserves_chain_and_source_order() {
    let indexed = index(
        "module Main where\n\
         foreign import data Subject :: Type\n\
         class Example a\n\
         instance first :: Example Subject\n\
         else instance middle :: Example Subject\n\
         else instance last :: Example Subject\n\
         instance standalone :: Example Subject\n",
    );

    let first = instance_id(&indexed, "first");
    let middle = instance_id(&indexed, "middle");
    let last = instance_id(&indexed, "last");
    let standalone = instance_id(&indexed, "standalone");
    let first_chain = indexed.pairs.instance_chain_id(first).unwrap();
    let standalone_chain = indexed.pairs.instance_chain_id(standalone).unwrap();

    assert_ne!(first_chain, standalone_chain);
    assert_eq!(indexed.pairs.instance_chain_metadata(first), Some((first_chain, 0)));
    assert_eq!(indexed.pairs.instance_chain_metadata(middle), Some((first_chain, 1)));
    assert_eq!(indexed.pairs.instance_chain_metadata(last), Some((first_chain, 2)));
    assert_eq!(indexed.pairs.instance_chain_metadata(standalone), Some((standalone_chain, 0)),);
    assert_eq!(indexed.pairs.instance_chain_position(first), Some(0));
    assert_eq!(indexed.pairs.instance_chain_position(middle), Some(1));
    assert_eq!(indexed.pairs.instance_chain_position(last), Some(2));
    assert_eq!(indexed.pairs.instance_chain_position(standalone), Some(0));
}

#[test]
fn item_sources_preserve_all_associations_in_table_order() {
    let pairs = [('z', 1), ('c', 3), ('a', 3), ('b', 3), ('x', 6)];
    for (item, expected) in [
        (0, vec![]),
        (1, vec!['z']),
        (2, vec![]),
        (3, vec!['c', 'a', 'b']),
        (6, vec!['x']),
        (7, vec![]),
    ] {
        let sources = crate::item_sources(&pairs, item).collect::<Vec<_>>();
        assert_eq!(sources, expected);
    }
    assert_eq!(crate::item_sources::<char, u32>(&[], 3).next(), None);
}
