use std::iter;

use building::{QueryEngine, prim};
use files::Files;

fn snapshot_settings() -> insta::Settings {
    let mut settings = insta::Settings::clone_current();
    settings.set_omit_expression(true);
    settings
}

#[test]
fn test_basic_cycle() {
    let _settings = snapshot_settings().bind_to_scope();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let id = files.insert(
        "Main.purs",
        r#"
module Main where

type T = 123
t = 123

single a = single a

double1 a = double2 a
double2 a = double1 a

triple1 a = triple2 a
triple2 a = triple3 a
triple3 a = triple1 a

newtype Mu f a = Mu (f (Mu f a))

data Double1 = Double1 Double2
data Double2 = Double2 Double1

newtype Triple1 a = Triple1 Triple2
newtype Triple2 a = Triple2 Triple3
type Triple3 a = Triple1 a
"#,
    );
    let content = files.content(id);

    engine.set_content(id, content);

    let groups = engine.grouped(id).unwrap();

    let terms = &groups.term_scc;
    let types = &groups.type_scc;

    insta::assert_debug_snapshot!((terms, types));
}

#[test]
fn test_operator_cycle() {
    let _settings = snapshot_settings().bind_to_scope();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let id = files.insert(
        "Main.purs",
        r#"
module Main where

add a b = a + b

infix 5 add as +

type Add a b = a + b

infix 5 type Add as +
"#,
    );
    let content = files.content(id);

    engine.set_content(id, content);

    let groups = engine.grouped(id).unwrap();

    let terms = &groups.term_scc;
    let types = &groups.type_scc;

    insta::assert_debug_snapshot!((terms, types));
}

#[test]
fn test_non_cycle_ordering() {
    let _settings = snapshot_settings().bind_to_scope();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let id = files.insert(
        "Main.purs",
        r#"
module Main where

a _ = b 0
b _ = c 0
c _ = 0
"#,
    );
    let content = files.content(id);

    engine.set_content(id, content);

    let groups = engine.grouped(id).unwrap();

    let terms = &groups.term_scc;
    let types = &groups.type_scc;

    insta::assert_debug_snapshot!((terms, types));
}

#[test]
fn test_recursive_synonym_errors() {
    let _settings = snapshot_settings().bind_to_scope();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let id = files.insert(
        "Main.purs",
        r#"
module Main where

type F = G
type G = F

type H = H
"#,
    );
    let content = files.content(id);

    engine.set_content(id, content);

    let groups = engine.grouped(id).unwrap();

    insta::assert_debug_snapshot!(groups.cycle_errors);
}

#[test]
fn test_do_fn_not_in_scope() {
    let _settings = snapshot_settings().bind_to_scope();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let id = files.insert(
        "Main.purs",
        r#"
module Main where

x = do
  pure 1
"#,
    );
    let content = files.content(id);

    engine.set_content(id, content);

    let lowered = engine.lowered(id).unwrap();

    insta::assert_debug_snapshot!(lowered.errors);
}

#[test]
fn test_ado_fn_not_in_scope() {
    let _settings = snapshot_settings().bind_to_scope();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let id = files.insert(
        "Main.purs",
        r#"
module Main where

x = ado
  y <- pure 1
  in y
"#,
    );
    let content = files.content(id);

    engine.set_content(id, content);

    let lowered = engine.lowered(id).unwrap();

    insta::assert_debug_snapshot!(lowered.errors);
}

#[test]
fn test_negate_not_in_scope() {
    let _settings = snapshot_settings().bind_to_scope();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let id = files.insert(
        "Main.purs",
        r#"
module Main where

x = -1
"#,
    );
    let content = files.content(id);

    engine.set_content(id, content);

    let lowered = engine.lowered(id).unwrap();

    insta::assert_debug_snapshot!(lowered.errors);
}

#[test]
fn test_recursive_kinds_errors() {
    let _settings = snapshot_settings().bind_to_scope();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let id = files.insert(
        "Main.purs",
        r#"
module Main where

foreign import data A :: B -> Type
foreign import data B :: A -> Type

data C :: Proxy D -> Type
data D :: Proxy C -> Type

foreign import data Proxy :: forall k. k -> Type
"#,
    );
    let content = files.content(id);

    engine.set_content(id, content);

    let groups = engine.grouped(id).unwrap();

    insta::assert_debug_snapshot!(groups.cycle_errors);
}

#[test]
fn test_non_recursive_kinds() {
    let _settings = snapshot_settings().bind_to_scope();
    let mut engine = QueryEngine::default();
    let mut files = Files::default();
    prim::configure(&mut engine, &mut files);

    let id = files.insert(
        "Main.purs",
        r#"
module Main where

data A = A B
data B = B A

data C (a :: Proxy D)
data D (a :: Proxy C)

foreign import data Proxy :: forall k. k -> Type
"#,
    );
    let content = files.content(id);

    engine.set_content(id, content);

    let lowered = engine.lowered(id).unwrap();
    let groups = engine.grouped(id).unwrap();

    let errors: Vec<_> = iter::chain(&lowered.errors, &groups.cycle_errors).collect();
    insta::assert_debug_snapshot!(errors);
}
