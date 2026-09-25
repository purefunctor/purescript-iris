//! Implements core type structures.

pub mod constraint;
pub mod exhaustive;
pub mod fd;
pub mod fold;
pub mod generalise;
pub mod normalise;
pub mod pretty;
pub mod signature;
pub mod skolem;
pub mod substitute;
pub mod toolkit;
pub mod unification;
pub mod walk;
pub mod zonk;

use std::sync::Arc;

use files::FileId;
use indexing::{TermItemId, TypeItemId};
use smol_str::{SmolStr, SmolStrBuilder};

/// A globally unique identity for a rigid type variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Name {
    pub file: FileId,
    pub unique: u32,
    pub scope: Option<SkolemScope>,
}

impl Name {
    /// Renders this name as a stable textual variable like `t42`.
    pub fn as_text(self) -> SmolStr {
        let mut text = SmolStrBuilder::new();
        text.push('t');
        text.push_str(&self.unique.to_string());
        text.finish()
    }
}

/// A marker used to represent binding levels of variables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Depth(pub u32);

impl Depth {
    pub fn increment(self) -> Depth {
        Depth(self.0 + 1)
    }
}

/// A globally unique lexical scope introduced while checking an expected forall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkolemScope {
    pub file: FileId,
    pub unique: u32,
}

/// Carries information about a type variable under a forall.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ForallBinder {
    /// Whether this binder is visible to type applications.
    pub visible: bool,
    /// The unique identity attached to the type variable.
    pub name: Name,
    /// The kind of the type variable.
    pub kind: TypeId,
    /// The lexical scope owned by an expected forall during expression checking.
    pub scope: Option<SkolemScope>,
}

/// Represents a row type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowType {
    /// A stable-sorted list representing `Map<Label, NonEmptyList<Type>>`.
    pub fields: Arc<[RowField]>,
    /// The tail of an open row.
    pub tail: Option<TypeId>,
}

impl RowType {
    pub fn from_open(fields: Arc<[RowField]>, tail: TypeId) -> RowType {
        debug_assert!(!fields.is_empty(), "critical violation: empty open row");
        RowType { fields, tail: Some(tail) }
    }

    pub fn from_closed(fields: Arc<[RowField]>) -> RowType {
        RowType { fields, tail: None }
    }
}

/// A field in a row type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowField {
    /// The label of the row field.
    pub label: SmolStr,
    /// The [`Type`] of the row field.
    pub id: TypeId,
}

/// An application spine argument, classified by whether it came from
/// [`Type::KindApplication`] or [`Type::Application`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApplicationArgument {
    Kind(TypeId),
    Type(TypeId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedDataDeclaration {
    pub type_parameters: Arc<[ForallBinderId]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedSynonym {
    pub kind: TypeId,
    pub parameters: Arc<[ForallBinder]>,
    pub expansion: TypeId,
}

/// Represents a checked class declaration.
///
/// Member types are stored in [`CheckedModule::terms`] quantified and
/// constrained with [`Type::Forall`] and [`Type::Constrained`]:
///
/// ```purescript
/// eq :: forall a. Eq a => a -> a -> Boolean
/// ```
///
/// [`CheckedModule::terms`]: crate::CheckedModule::terms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckedSuperclass {
    /// Stable source identity of this occurrence in the defining file.
    pub source_id: lowering::TypeId,
    /// Constraint expressed in terms of the class head's rigids.
    pub constraint: TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckedClassMember {
    /// Stable identity of the generated class member selector.
    pub item_id: TermItemId,
    /// Declared member type before the class dictionary constraint is added.
    pub field_type: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedClass {
    /// Post-generalisation kind variable binders.
    pub kind_binders: Arc<[ForallBinderId]>,
    /// Post-generalisation type parameter binders.
    pub type_parameters: Arc<[ForallBinderId]>,
    /// Canonical class head, e.g. `Eq a` or `Foo @k a`.
    pub canonical: TypeId,
    /// Superclass occurrences from the class declaration.
    pub superclasses: Arc<[CheckedSuperclass]>,
    /// Functional dependencies.
    pub functional_dependencies: Arc<[fd::Fd]>,
    /// Class members in declaration order.
    pub members: Arc<[CheckedClassMember]>,
}

/// Represents a checked instance declaration head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckedInstance {
    /// Type class reference.
    pub resolution: (FileId, TypeItemId),
    /// The signature of the instance, e.g. `forall a. Eq a => Eq (Array a)`.
    ///
    /// This type is shared between the checking rules for [instance heads] and
    /// the [instance bodies]. Checking rules for instance bodies are syntax
    /// driven. In the example below, all instances of the type variable `a`
    /// resolve to the same [`Type::Rigid`].
    ///
    /// ```purescript
    /// instance Eq a => Eq (Array a) where
    ///    eq :: Array a -> Array a -> Boolean
    ///    eq = eqArrayImpl
    /// ```
    ///
    /// [instance heads]: checking::source::term_items::check_instance_declaration
    /// [instance bodies]: checking::source::term_items::check_instance_members
    pub signature: TypeId,
    /// Like [`CheckedInstance::signature`] but for constraint matching.
    ///
    /// Take for example:
    ///
    /// ```purescript
    /// newtype List a = List { head :: Int, tail :: Maybe (List a) }
    ///
    /// derive newtype instance Eq a => Eq (List a)
    /// ```
    ///
    /// The compiler aims to prove that there is an Eq instance for List's
    /// internal representation, producing the constraint `Eq (List a)`,
    /// making it self-recursive. During matching, `Eq (List a)` will
    /// select itself among the available `Eq` instances.
    ///
    /// By design, type variables in instance declarations are pattern
    /// variables during matching i.e. the constraint solver produces
    /// substitutions when matching arguments.
    ///
    /// These substitutions are used to instantiate variables in subgoals.
    /// For example, this enables matching `Eq (Array Int)` against
    /// `Eq a => Eq (Array a)` producing the substitution `a = Int` for
    /// the subgoal `Eq Int` to surface.
    ///
    /// Also by design, any pattern variables left unbound become unification
    /// variables, such that the constraint solver can fill them in later
    /// through unification and functional dependencies. Intermediate type
    /// variables in type-level programming come to mind:
    ///
    /// ```purescript
    /// instance Build 0 ()
    /// else instance
    ///   ( Add minusOne 1 currentId
    ///   , ToString currentId labelId
    ///   , Append "n" labelId actualLabel
    ///   , Build minusOne minusOneResult
    ///   , Cons actualLabel currentId minusOneResult finalResult
    ///   ) => Build currentId finalResult
    /// ```
    ///
    /// `Eq (List a)` matching against itself goes wrong because:
    /// 1. both usages of `a` are the same [`Name`], which triggers a fast path
    ///    in instance matching where the pattern variable `a` is not bound;
    /// 2. in effect, the pattern variable becomes a unification variable,
    ///    which propagates into the subgoal `Eq ?a`, which is never solved.
    ///
    /// By creating a separate [`Name`] for use as a pattern variable:
    /// 1. we avoid the fast path in instance matching, creating the
    ///    binding from the [matchable] `a` to the [signature] `a`;
    /// 2. in effect, the subgoal `Eq a` does not become `Eq ?a`,
    ///    removing the unsolved constraint error.
    ///
    /// [signature]: CheckedInstance::signature
    /// [matchable]: CheckedInstance::matchable
    pub matchable: TypeId,
}

/// The core type representation used by the checker after name resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
    /// Type application, `Array Int`.
    Application(TypeId, TypeId),
    /// Kind application, `Proxy @Int`.
    KindApplication(TypeId, TypeId),

    /// A universally quantified type, `forall a. a -> a`.
    Forall(ForallBinderId, TypeId),
    /// A constrained type, `Constraint => Constrained`.
    Constrained(TypeId, TypeId),
    /// A function type, `a -> b`.
    Function(TypeId, TypeId),
    /// A type with an explicit kind, `T :: K`.
    Kinded(TypeId, TypeId),

    /// A resolved type constructor reference.
    Constructor(FileId, TypeItemId),

    /// A type-level integer literal, `42`.
    Integer(i32),
    /// A type-level string literal, `"life"`.
    String(lowering::StringKind, lowering::StringLiteral),
    /// A row type, see [`RowType`].
    Row(RowTypeId),

    /// A bound skolem variable that can only unify with itself.
    Rigid(Name, Depth, TypeId),
    /// A unification variable that can be solved to another [`Type`].
    Unification(u32),

    /// A type variable that did not resolve to a binder.
    Free(SmolStrId),
    /// Recovery marker for exceptional type checker paths.
    Unknown(SmolStrId),
}

/// Immutable properties of an interned type.
///
/// Head flags describe the outermost node, while transitive flags describe
/// whether the node or any of its descendants has the property. Folds use
/// transitive flags to skip subtrees they cannot change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TypeFlags(u8);

impl TypeFlags {
    pub(crate) const MAY_NORMALISE: u8 = 1 << 0;
    pub(crate) const HAS_UNIFICATION: u8 = 1 << 1;
    pub(crate) const HAS_RIGID: u8 = 1 << 2;
    pub(crate) const HAS_NESTED_ROW: u8 = 1 << 3;

    const TRANSITIVE: u8 =
        TypeFlags::HAS_UNIFICATION | TypeFlags::HAS_RIGID | TypeFlags::HAS_NESTED_ROW;

    pub(crate) fn from_bits(bits: u8) -> TypeFlags {
        TypeFlags(bits)
    }

    /// The transitive flags to propagate into a type containing this one.
    pub(crate) fn transitive(self) -> u8 {
        self.0 & TypeFlags::TRANSITIVE
    }

    /// Whether head normalisation may change this type.
    pub fn may_normalise(self) -> bool {
        self.0 & TypeFlags::MAY_NORMALISE != 0
    }

    /// Whether zonking may change this type.
    ///
    /// Zonking replaces solved unification variables and flattens nested rows.
    pub fn may_zonk(self) -> bool {
        self.0 & (TypeFlags::HAS_UNIFICATION | TypeFlags::HAS_NESTED_ROW) != 0
    }

    /// Whether this type contains unification or rigid variables.
    pub fn has_variables(self) -> bool {
        self.0 & (TypeFlags::HAS_UNIFICATION | TypeFlags::HAS_RIGID) != 0
    }

    /// Whether substituting rigid variables may change this type.
    ///
    /// Unification variables are included since their solutions may contain
    /// the rigid variables being substituted.
    pub fn may_substitute(self) -> bool {
        self.0 & TypeFlags::TRANSITIVE != 0
    }
}

/// The role of a type parameter for safe coercions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Phantom,
    Representational,
    Nominal,
}

pub type ForallBinderId = interner::Id<ForallBinder>;
pub type RowTypeId = interner::Id<RowType>;
pub type SmolStrId = interner::Id<SmolStr>;
pub type TypeId = interner::Id<Type>;
