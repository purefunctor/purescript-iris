//! Rendering functional trees as JavaScript modules.

mod case;
mod declaration;
mod effect;
mod effect_loop;
mod expression;
mod function;
mod inline;
mod module;
mod pattern;
mod references;
mod structure;
mod stylex;
mod syntax;
mod tail_call;

use std::cell::RefCell;
use std::rc::Rc;

use files::{FileId, ForeignSourceKind};
use functional::tree::{
    Binding, Declaration, ExpressionId as FunctionalExpressionId, GlobalId, LocalId,
    Module as FunctionalModule, ModuleDependency, Origin, Parameter, PatternId,
};
use rustc_hash::{FxHashMap, FxHashSet};
use smol_str::SmolStr;

use super::super::names::NameAllocator;
use crate::error::{ModuleError, UnsupportedState};
use crate::program_text::NameIndex;
use crate::tree::{ExpressionId, Tree};
use crate::writer::Writer;

use self::stylex::StyleXTheme;
use self::tail_call::{TailCallContext, TailCallGroup, TailCallProfile};

pub(crate) struct Generator<'m> {
    module: &'m FunctionalModule,
    module_dependencies: FxHashMap<FileId, &'m ModuleDependency>,
    global_names: FxHashMap<GlobalId, SmolStr>,
    external_module_namespaces: FxHashMap<FileId, SmolStr>,
    rendered_module_namespaces: RefCell<FxHashSet<FileId>>,
    external_named_imports: FxHashMap<GlobalId, NamedImport>,
    stylex_namespace: Option<SmolStr>,
    stylex_theme: Option<StyleXTheme>,
    foreign_import: Option<ForeignImport>,
    runtime_namespace: Option<SmolStr>,
    lazy_global_names: FxHashMap<GlobalId, SmolStr>,
    global_tail_call_groups: Vec<TailCallGroup>,
    global_tail_call_group_positions: FxHashMap<GlobalId, usize>,
    reserved_module_names: Rc<FxHashSet<SmolStr>>,
}

struct NamedImport {
    exported_name: SmolStr,
    local_binding_name: SmolStr,
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
    Reference(SmolStr),
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
    Discard,
    EffectDiscard,
    DiscardAndBreak { label: &'a str },
    EffectDiscardAndBreak { label: &'a str },
    Continue,
    EffectContinue,
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
    source_name: Option<NameIndex>,
    group: &'a TailCallGroup,
    profile: &'a TailCallProfile,
    state: usize,
}

struct AbstractionBinding<'a> {
    name: &'a str,
    source_name: Option<&'a SmolStr>,
    parameters: &'a [PatternId],
    body: FunctionalExpressionId,
    uncurried: bool,
}

struct ModuleRenderer<'a, 'm, 't> {
    generator: &'a Generator<'m>,
    tree: &'a mut Tree<'t>,
    writer: &'a mut Writer<'t>,
    file: OutputFile<'a>,
}

/// The files generated for a module.
#[derive(Clone, Copy)]
enum OutputFile<'a> {
    Index,
    StyleXTheme(&'a StyleXTheme),
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

    fn bind_reference(&mut self, parameter: &Parameter, name: SmolStr) {
        self.locals.insert(parameter.id, LocalBinding::Reference(name));
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
            Destination::Discard => Destination::EffectDiscard,
            Destination::DiscardAndBreak { label } => Destination::EffectDiscardAndBreak { label },
            Destination::Continue => Destination::EffectContinue,
            Destination::TailEffectThunkReturn
            | Destination::EffectReturn
            | Destination::EffectTailEffectReturn
            | Destination::EffectAssign(_)
            | Destination::EffectAssignAndBreak { .. }
            | Destination::EffectDiscard
            | Destination::EffectDiscardAndBreak { .. }
            | Destination::EffectContinue => {
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
            Destination::EffectDiscard => Destination::Discard,
            Destination::EffectDiscardAndBreak { label } => Destination::DiscardAndBreak { label },
            Destination::EffectContinue => Destination::Continue,
            Destination::Return
            | Destination::TailEffectThunkReturn
            | Destination::TailEffectReturn
            | Destination::Assign(_)
            | Destination::AssignAndBreak { .. }
            | Destination::Discard
            | Destination::DiscardAndBreak { .. }
            | Destination::Continue => {
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
                | Destination::EffectDiscard
                | Destination::EffectDiscardAndBreak { .. }
                | Destination::EffectContinue
        )
    }

    fn and_break(self, label: &'a str) -> Destination<'a> {
        match self {
            Destination::Assign(name) => Destination::AssignAndBreak { name, label },
            Destination::EffectAssign(name) => Destination::EffectAssignAndBreak { name, label },
            Destination::Discard => Destination::DiscardAndBreak { label },
            Destination::EffectDiscard => Destination::EffectDiscardAndBreak { label },
            Destination::Return
            | Destination::TailEffectThunkReturn
            | Destination::TailEffectReturn
            | Destination::EffectReturn
            | Destination::EffectTailEffectReturn
            | Destination::AssignAndBreak { .. }
            | Destination::EffectAssignAndBreak { .. }
            | Destination::DiscardAndBreak { .. }
            | Destination::EffectDiscardAndBreak { .. }
            | Destination::Continue
            | Destination::EffectContinue => {
                unreachable!("invariant violated: destination already ends its block")
            }
        }
    }

    fn is_discard(self) -> bool {
        matches!(
            self,
            Destination::Discard | Destination::DiscardAndBreak { .. } | Destination::Continue
        )
    }
}

impl Generator<'_> {
    fn global_name(&self, id: GlobalId) -> &str {
        self.global_names
            .get(&id)
            .map(SmolStr::as_str)
            .expect("invariant violated: JavaScript global has no allocated name")
    }

    fn binding_origin(&self, binding: &Binding) -> Option<Origin> {
        binding.origin.or(self.module.storage[binding.expression].origin)
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

fn declaration_source_name(writer: &Writer<'_>, declaration: &Declaration) -> Option<NameIndex> {
    let source = matches!(declaration.global.id, GlobalId::Term(_, _));
    source.then(|| writer.register_name(&declaration.global.item_name))
}

fn binding_source_name(binding: &Binding) -> Option<&SmolStr> {
    binding.origin.is_some().then_some(&binding.parameter.name)
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
