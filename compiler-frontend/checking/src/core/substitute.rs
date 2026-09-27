//! Implements name-based type substitution for the core representation.

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use building_types::QueryResult;

use crate::ExternalQueries;
use crate::context::CheckContext;
use crate::core::fold::{FoldAction, TypeFold, fold_type};
use crate::core::{Depth, ForallBinder, Name, RowTypeId, Type, TypeFlags, TypeId};
use crate::state::CheckState;

pub type NameToType = FxHashMap<Name, TypeId>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RigidReplacement {
    name: Name,
    depth: Depth,
    type_id: TypeId,
}

/// A name-based replacement of rigid variables with fresh rigid variables.
///
/// Unlike [`NameToType`], this representation cannot contain non-rigid
/// replacement types.
#[derive(Debug, Default)]
pub struct RigidRenaming {
    replacements: FxHashMap<Name, RigidReplacement>,
}

impl RigidRenaming {
    pub fn insert<Q>(&mut self, context: &CheckContext<Q>, original: Name, replacement: TypeId)
    where
        Q: ExternalQueries,
    {
        let Type::Rigid(name, depth, _) = *context.lookup_type(replacement) else {
            unreachable!("invariant violated: expected a rigid variable");
        };
        let replacement = RigidReplacement { name, depth, type_id: replacement };
        self.replacements.insert(original, replacement);
    }

    pub fn substitute<Q>(
        &self,
        state: &mut CheckState,
        context: &CheckContext<Q>,
        in_type: TypeId,
    ) -> QueryResult<TypeId>
    where
        Q: ExternalQueries,
    {
        fold_type(state, context, in_type, &mut SubstituteRigidName { renaming: self })
    }

    pub(crate) fn replacement(&self, original: Name) -> Option<(Name, Depth)> {
        self.replacements.get(&original).map(|replacement| (replacement.name, replacement.depth))
    }
}

/// Implements [`Name`]-based substitution for [`Type::Rigid`] variables.
///
/// Names are globally unique, removing the need for scope tracking and
/// removing the need for capture-avoiding substitutions. This property
/// is extremely useful for for instantiation.
pub struct SubstituteName<'a> {
    bindings: NameBindings<'a>,
}

enum NameBindings<'a> {
    One(Name, TypeId),
    Few(&'a [(Name, TypeId)]),
    Many(&'a NameToType),
}

impl SubstituteName<'_> {
    pub fn one<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        name: Name,
        replacement: TypeId,
        in_type: TypeId,
    ) -> QueryResult<TypeId>
    where
        Q: ExternalQueries,
    {
        let bindings = NameBindings::One(name, replacement);
        fold_type(state, context, in_type, &mut SubstituteName { bindings })
    }

    /// Substitutes a small number of bindings, such as those introduced by
    /// instantiating a quantifier chain, where scanning is cheaper than hashing.
    ///
    /// Later bindings take precedence, matching insertion into [`NameToType`].
    pub fn few<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        bindings: &[(Name, TypeId)],
        in_type: TypeId,
    ) -> QueryResult<TypeId>
    where
        Q: ExternalQueries,
    {
        SubstituteName::templated(state, context, NameBindings::Few(bindings), in_type)
    }

    pub fn many<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        bindings: &NameToType,
        in_type: TypeId,
    ) -> QueryResult<TypeId>
    where
        Q: ExternalQueries,
    {
        SubstituteName::templated(state, context, NameBindings::Many(bindings), in_type)
    }

    /// Substitutes by replaying the memoised [`SubstitutionTemplate`] for
    /// `in_type` when it has one, and by folding otherwise.
    fn templated<Q>(
        state: &mut CheckState,
        context: &CheckContext<Q>,
        bindings: NameBindings<'_>,
        in_type: TypeId,
    ) -> QueryResult<TypeId>
    where
        Q: ExternalQueries,
    {
        let flags = context.lookup_type_flags(in_type);
        if !flags.may_substitute() {
            return Ok(in_type);
        }
        if !flags.may_zonk()
            && let Some(template) = state.substitution_template(context, in_type)
        {
            return Ok(template.instantiate(context, &bindings));
        }
        fold_type(state, context, in_type, &mut SubstituteName { bindings })
    }
}

impl NameBindings<'_> {
    #[inline(always)]
    fn lookup(&self, name: Name) -> Option<TypeId> {
        match *self {
            NameBindings::One(original, replacement) => (original == name).then_some(replacement),
            NameBindings::Few(bindings) => bindings
                .iter()
                .rev()
                .find_map(|&(original, replacement)| (original == name).then_some(replacement)),
            NameBindings::Many(bindings) => bindings.get(&name).copied(),
        }
    }
}

impl TypeFold for SubstituteName<'_> {
    fn may_change(&self, flags: TypeFlags) -> bool {
        flags.may_substitute()
    }

    #[inline]
    fn transform<Q>(
        &mut self,
        _state: &mut CheckState,
        _context: &CheckContext<Q>,
        _id: TypeId,
        t: &Type,
    ) -> QueryResult<FoldAction>
    where
        Q: ExternalQueries,
    {
        if let Type::Rigid(name, _, _) = *t
            && let Some(replacement) = self.bindings.lookup(name)
        {
            return Ok(FoldAction::Replace(replacement));
        }

        Ok(FoldAction::Continue)
    }
}

/// A replayable plan of the nodes that substituting rigid variables into a
/// type may rebuild, listed in the order that folding would complete them.
///
/// Instantiating the same declared type repeatedly, such as an imported
/// function at each of its uses, otherwise looks up, normalises, and compares
/// every node on the path to a rigid variable each time.
///
/// Templates are only built for types that zonking cannot change. Without
/// unification variables, whose solutions change over time, or nested rows,
/// whose normalisation interns merged rows while folding, folding such a type
/// is purely structural. Interned types are immutable, so a template stays
/// valid for as long as its [`TypeId`], and replaying it performs the same
/// interning, in the same order, as folding with [`SubstituteName`].
pub struct SubstitutionTemplate {
    nodes: Vec<TemplateNode>,
}

/// The memoised state of the [`SubstitutionTemplate`] for a type.
pub enum SubstitutionTemplateEntry {
    /// The type was substituted into once, without building a template.
    Seen,
    Built(SubstitutionTemplate),
    /// The type contains a rigid variable whose kind contains another.
    Unsupported,
}

struct TemplateNode {
    original: TypeId,
    shape: TemplateShape,
}

/// A child of a [`TemplateNode`]: either a type that substitution cannot
/// change, or the index of an earlier node in the template.
#[derive(Clone, Copy)]
enum TemplateOperand {
    Fixed(TypeId),
    Node(u32),
}

#[derive(Clone, Copy)]
enum TemplatePair {
    Application,
    KindApplication,
    Constrained,
    Function,
    Kinded,
}

enum TemplateShape {
    Rigid(Name),
    Pair(TemplatePair, TemplateOperand, TemplateOperand),
    Forall(ForallBinder, TemplateOperand, TemplateOperand),
    Row(RowTypeId, Box<[TemplateOperand]>, Option<TemplateOperand>),
}

impl SubstitutionTemplate {
    /// Builds a template for a type that zonking cannot change.
    ///
    /// Returns [`None`] when a rigid variable's kind contains another rigid
    /// variable. Folding skips the kind of a rigid variable it substitutes,
    /// so a template rebuilding that kind first could intern types that the
    /// fold never would. Such kinds only arise in poly-kinded signatures,
    /// which are rare enough to fold instead.
    pub(crate) fn build<Q>(context: &CheckContext<Q>, id: TypeId) -> Option<SubstitutionTemplate>
    where
        Q: ExternalQueries,
    {
        let mut template = SubstitutionTemplate { nodes: Vec::new() };
        template.build_operand(context, id)?;
        Some(template)
    }

    fn build_operand<Q>(&mut self, context: &CheckContext<Q>, id: TypeId) -> Option<TemplateOperand>
    where
        Q: ExternalQueries,
    {
        let (t, flags) = context.lookup_type_with_flags(id);
        if !flags.has_rigid() {
            return Some(TemplateOperand::Fixed(id));
        }

        let shape = match *t {
            Type::Application(function, argument) => {
                self.build_pair(context, TemplatePair::Application, function, argument)?
            }
            Type::KindApplication(function, argument) => {
                self.build_pair(context, TemplatePair::KindApplication, function, argument)?
            }
            Type::Constrained(constraint, inner) => {
                self.build_pair(context, TemplatePair::Constrained, constraint, inner)?
            }
            Type::Function(argument, result) => {
                self.build_pair(context, TemplatePair::Function, argument, result)?
            }
            Type::Kinded(inner, kind) => {
                self.build_pair(context, TemplatePair::Kinded, inner, kind)?
            }
            Type::Forall(binder_id, inner) => {
                let binder = context.lookup_forall_binder(binder_id);
                let kind = self.build_operand(context, binder.kind)?;
                let inner = self.build_operand(context, inner)?;
                TemplateShape::Forall(binder, kind, inner)
            }
            Type::Row(row_id) => {
                let row = context.lookup_row_type(row_id);
                let fields = row.fields.iter().map(|field| self.build_operand(context, field.id));
                let fields = fields.collect::<Option<Box<[_]>>>()?;
                let tail = match row.tail {
                    Some(tail) => Some(self.build_operand(context, tail)?),
                    None => None,
                };
                TemplateShape::Row(row_id, fields, tail)
            }
            Type::Rigid(name, _, kind) => {
                if context.lookup_type_flags(kind).has_rigid() {
                    return None;
                }
                TemplateShape::Rigid(name)
            }
            Type::Constructor(..)
            | Type::Integer(_)
            | Type::String(..)
            | Type::Unification(_)
            | Type::Free(_)
            | Type::Unknown(_) => return Some(TemplateOperand::Fixed(id)),
        };

        let index = u32::try_from(self.nodes.len()).ok()?;
        self.nodes.push(TemplateNode { original: id, shape });
        Some(TemplateOperand::Node(index))
    }

    fn build_pair<Q>(
        &mut self,
        context: &CheckContext<Q>,
        pair: TemplatePair,
        first: TypeId,
        second: TypeId,
    ) -> Option<TemplateShape>
    where
        Q: ExternalQueries,
    {
        let first = self.build_operand(context, first)?;
        let second = self.build_operand(context, second)?;
        Some(TemplateShape::Pair(pair, first, second))
    }

    fn instantiate<Q>(&self, context: &CheckContext<Q>, bindings: &NameBindings<'_>) -> TypeId
    where
        Q: ExternalQueries,
    {
        let mut results: SmallVec<[TypeId; 32]> = SmallVec::with_capacity(self.nodes.len());

        // Operands resolve to their substituted type and whether it differs
        // from the original, since unchanged nodes are reused, not rebuilt.
        let resolve = |results: &[TypeId], operand: TemplateOperand| match operand {
            TemplateOperand::Fixed(id) => (id, false),
            TemplateOperand::Node(index) => {
                let index = index as usize;
                let result = results[index];
                (result, result != self.nodes[index].original)
            }
        };

        for node in &self.nodes {
            let result = match node.shape {
                TemplateShape::Rigid(name) => bindings.lookup(name).unwrap_or(node.original),
                TemplateShape::Pair(pair, first, second) => {
                    let (first, first_changed) = resolve(&results, first);
                    let (second, second_changed) = resolve(&results, second);
                    if !first_changed && !second_changed {
                        node.original
                    } else {
                        match pair {
                            TemplatePair::Application => context.intern_application(first, second),
                            TemplatePair::KindApplication => {
                                context.intern_kind_application(first, second)
                            }
                            TemplatePair::Constrained => context.intern_constrained(first, second),
                            TemplatePair::Function => context.intern_function(first, second),
                            TemplatePair::Kinded => context.intern_kinded(first, second),
                        }
                    }
                }
                TemplateShape::Forall(binder, kind, inner) => {
                    let (kind, kind_changed) = resolve(&results, kind);
                    let (inner, inner_changed) = resolve(&results, inner);
                    if !kind_changed && !inner_changed {
                        node.original
                    } else {
                        let binder = ForallBinder { kind, ..binder };
                        let binder_id = context.intern_forall_binder(binder);
                        context.intern_forall(binder_id, inner)
                    }
                }
                TemplateShape::Row(row_id, ref fields, tail) => {
                    let row = context.lookup_row_type(row_id);
                    let mut rebuilt = row.fields.to_vec();
                    let mut changed = false;
                    for (field, &operand) in rebuilt.iter_mut().zip(fields.iter()) {
                        let (id, field_changed) = resolve(&results, operand);
                        field.id = id;
                        changed |= field_changed;
                    }
                    let tail = tail.map(|tail| {
                        let (tail, tail_changed) = resolve(&results, tail);
                        changed |= tail_changed;
                        tail
                    });
                    if changed { context.intern_row(rebuilt, tail) } else { node.original }
                }
            };
            results.push(result);
        }

        // Templates are only built for types containing rigid variables,
        // and folding completes the root after all of its descendants.
        let Some(&result) = results.last() else {
            unreachable!("invariant violated: empty substitution template");
        };
        result
    }
}

struct SubstituteRigidName<'a> {
    renaming: &'a RigidRenaming,
}

impl TypeFold for SubstituteRigidName<'_> {
    fn may_change(&self, flags: TypeFlags) -> bool {
        flags.may_substitute()
    }

    fn transform<Q>(
        &mut self,
        _state: &mut CheckState,
        _context: &CheckContext<Q>,
        _id: TypeId,
        t: &Type,
    ) -> QueryResult<FoldAction>
    where
        Q: ExternalQueries,
    {
        if let Type::Rigid(name, _, _) = t
            && let Some(replacement) = self.renaming.replacements.get(name)
        {
            Ok(FoldAction::Replace(replacement.type_id))
        } else {
            Ok(FoldAction::Continue)
        }
    }
}
