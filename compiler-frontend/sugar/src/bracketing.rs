//! Creates binary operator trees from operator chains.
//!
//! There are no built-in binary operators in PureScript, as they are defined
//! in-source for existing binary functions. Consequently, this feature makes
//! it nearly impossible for a PureScript parser to determine associativity
//! and precedence for operators.
//!
//! This module implements the 'bracketing' algorithm, which turns operator
//! chains into proper binary operator trees. The algorithm is implemented
//! as a [pratt parser], and it's surprisingly quite simple too!
//!
//! ```text
//! 1 + 2 * 3 + 4
//!
//! 1, [(+ 2), (* 3), (+ 4)]
//!
//! ((1 + (2 * 3)) + 4)
//! ```
//!
//! [pratt parser]: https://matklad.github.io/2020/04/13/simple-but-powerful-pratt-parsing.html

use std::collections::hash_map::Entry;
use std::iter::Peekable;
use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::{TermItemId, TypeItemId};
use lowering::{
    Associativity, BinderId, BinderKind, ExpressionId, ExpressionKind, IsElement, LoweredModule,
    OperatorPair, TermItemKind, TermOperatorId, TypeId, TypeItemKind, TypeKind, TypeOperatorId,
};
use rustc_hash::FxHashMap;

// section: utilities

/// Helper trait for [`TermOperatorId`]  and [`TypeOperatorId`].
trait ForOperatorId {
    type ItemId;

    fn resolve_operator(lowered: &LoweredModule, id: Self) -> Option<(FileId, Self::ItemId)>;

    fn operator_info(lowered: &LoweredModule, id: Self::ItemId) -> Option<(Associativity, u8)>;
}

impl ForOperatorId for TermOperatorId {
    type ItemId = TermItemId;

    fn resolve_operator(lowered: &LoweredModule, id: Self) -> Option<(FileId, Self::ItemId)> {
        lowered.tree.get_term_operator(id)
    }

    fn operator_info(lowered: &LoweredModule, id: Self::ItemId) -> Option<(Associativity, u8)> {
        let Some(TermItemKind::Operator { associativity, precedence, .. }) =
            lowered.tree.get_term_item_kind(id)
        else {
            return None;
        };
        associativity.zip(*precedence)
    }
}

impl ForOperatorId for TypeOperatorId {
    type ItemId = TypeItemId;

    fn resolve_operator(lowered: &LoweredModule, id: Self) -> Option<(FileId, Self::ItemId)> {
        lowered.tree.get_type_operator(id)
    }

    fn operator_info(lowered: &LoweredModule, id: Self::ItemId) -> Option<(Associativity, u8)> {
        let Some(TypeItemKind::Operator { associativity, precedence, .. }) =
            lowered.tree.get_type_item_kind(id)
        else {
            return None;
        };
        associativity.zip(*precedence)
    }
}

struct BracketingContext<'a, Q>
where
    Q: crate::ExternalQueries,
{
    queries: &'a Q,
    lowered: &'a LoweredModule,
    lowered_by_file: FxHashMap<FileId, Arc<LoweredModule>>,
}

impl<'a, Q> BracketingContext<'a, Q>
where
    Q: crate::ExternalQueries,
{
    fn new(queries: &'a Q, lowered: &'a LoweredModule) -> BracketingContext<'a, Q> {
        BracketingContext { queries, lowered, lowered_by_file: FxHashMap::default() }
    }
}

/// Resolves an operator and its associativity and precedence.
fn operator_info<OperatorId, Q>(
    context: &mut BracketingContext<'_, Q>,
    id: OperatorId,
) -> Option<(Associativity, u8)>
where
    Q: crate::ExternalQueries,
    OperatorId: ForOperatorId,
{
    let (file_id, item_id) = OperatorId::resolve_operator(context.lowered, id)?;
    let queries = context.queries;
    let lowered = match context.lowered_by_file.entry(file_id) {
        Entry::Occupied(entry) => entry.into_mut(),
        Entry::Vacant(entry) => {
            let lowered = queries.lowered(file_id).ok()?;
            entry.insert(lowered)
        }
    };
    OperatorId::operator_info(lowered.as_ref(), item_id)
}

/// Translates [`Associativity`] and precedence into binding power.
///
/// Each precedence level occupies two binding power slots so that
/// adjacent precedence levels never overlap. Without the `2*p` scaling,
/// `infixr 3` and `infix 4` both produce a binding power of 5, which
/// prevents the Pratt parser from breaking out of a higher-precedence
/// recursive call when it encounters a lower-precedence operator.
fn binding_power(associativity: Associativity, precedence: u8) -> (u8, u8) {
    let base = precedence.saturating_mul(2);
    let bp_0 = base.saturating_add(1);
    let bp_1 = base.saturating_add(2);
    match associativity {
        Associativity::None => (bp_0, bp_0),
        Associativity::Left => (bp_0, bp_1),
        Associativity::Right => (bp_1, bp_0),
    }
}

// section: algorithm

/// Common entry point for bracketing.
fn bracket<Id, Q>(
    context: &mut BracketingContext<'_, Q>,
    item: Option<Id>,
    items: &[OperatorPair<Id>],
) -> BracketingResult<Id>
where
    Q: crate::ExternalQueries,
    Id: IsElement,
    Id::OperatorId: ForOperatorId,
{
    match items {
        [OperatorPair { id, element }] => {
            let id = id.ok_or(BracketingError::InvalidOperator)?;
            Ok(OperatorTree::Branch(
                id,
                [OperatorTree::Leaf(item), OperatorTree::Leaf(*element)].into(),
            ))
        }
        _ => {
            let mut items = items.iter().copied().peekable();
            bracket_loop(context, item, &mut items, 0, None, &mut None)
        }
    }
}

/// Core pratt parsing loop for bracketing.
///
/// `peeked` holds the information of the next operator once resolved, as an
/// operator that ends a recursive call is peeked again by its caller.
fn bracket_loop<Id, Q>(
    context: &mut BracketingContext<'_, Q>,
    item: Option<Id>,
    items: &mut Peekable<impl Iterator<Item = OperatorPair<Id>>>,
    minimum_binding_power: u8,
    previous_operator: Option<OperatorInfo<Id::OperatorId>>,
    peeked: &mut Option<OperatorInfo<Id::OperatorId>>,
) -> BracketingResult<Id>
where
    Q: crate::ExternalQueries,
    Id: IsElement,
    Id::OperatorId: ForOperatorId,
{
    let mut left = OperatorTree::Leaf(item);

    while let Some(OperatorPair { id, element }) = items.peek().copied() {
        let operator = match *peeked {
            Some(operator) => operator,
            None => {
                let id = id.ok_or(BracketingError::InvalidOperator)?;
                let (associativity, precedence) =
                    operator_info(context, id).ok_or(BracketingError::FailedToResolve(id))?;
                *peeked.insert(OperatorInfo { id, associativity, precedence })
            }
        };
        let OperatorInfo { id, associativity, precedence } = operator;

        let (left_binding_power, right_binding_power) = binding_power(associativity, precedence);
        if left_binding_power < minimum_binding_power {
            break;
        }

        if let Some(previous) = previous_operator
            && associativity == Associativity::None
            && previous.associativity == Associativity::None
        {
            return Err(BracketingError::NonAssociative { previous, operator });
        } else if let Some(previous) = previous_operator
            && previous.associativity != associativity
            && previous.precedence == precedence
        {
            return Err(BracketingError::MixedAssociativity { previous, operator });
        }

        items.next();
        *peeked = None;

        let right =
            bracket_loop(context, element, items, right_binding_power, Some(operator), peeked)?;

        left = OperatorTree::Branch(id, [left, right].into());
    }

    Ok(left)
}

// section: types

/// An operator tree, the result of bracketing.
#[derive(Debug, PartialEq, Eq)]
pub enum OperatorTree<Id: IsElement> {
    Branch(Id::OperatorId, Arc<[OperatorTree<Id>; 2]>),
    Leaf(Option<Id>),
}

/// Errors that can occur during bracketing.
#[derive(Debug, PartialEq, Eq)]
pub enum BracketingError<T> {
    /// Likely a malformed operator.
    InvalidOperator,
    /// The operator does not exist.
    FailedToResolve(T),
    /// Non-associative operators used consecutively.
    NonAssociative { previous: OperatorInfo<T>, operator: OperatorInfo<T> },
    /// Operators with the same precedence but different associativities.
    MixedAssociativity { previous: OperatorInfo<T>, operator: OperatorInfo<T> },
}

/// Utility structure for bundling operator information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperatorInfo<T> {
    pub id: T,
    pub associativity: Associativity,
    pub precedence: u8,
}

pub type BracketingResult<Id> =
    Result<OperatorTree<Id>, BracketingError<<Id as IsElement>::OperatorId>>;

pub type BracketingMap<Id> = FxHashMap<Id, BracketingResult<Id>>;

// section: top-level

/// The result of bracketing across all operator chains in a module.
#[derive(Debug, PartialEq, Eq)]
pub struct Bracketed {
    pub binders: BracketingMap<BinderId>,
    pub expressions: BracketingMap<ExpressionId>,
    pub types: BracketingMap<TypeId>,
}

/// Performs bracketing across all operator chains in a module.
pub fn bracketed(
    queries: &impl crate::ExternalQueries,
    lowered: &LoweredModule,
) -> QueryResult<Bracketed> {
    let mut context = BracketingContext::new(queries, lowered);

    let mut binders = FxHashMap::default();
    for (id, kind) in lowered.tree.iter_binder() {
        if let BinderKind::OperatorChain { head, tail } = kind {
            binders.insert(id, bracket(&mut context, *head, tail));
        }
    }

    let mut expressions = FxHashMap::default();
    for (id, kind) in lowered.tree.iter_expression() {
        if let ExpressionKind::OperatorChain { head, tail } = kind {
            expressions.insert(id, bracket(&mut context, *head, tail));
        }
    }

    let mut types = FxHashMap::default();
    for (id, kind) in lowered.tree.iter_type() {
        if let TypeKind::OperatorChain { head, tail } = kind {
            types.insert(id, bracket(&mut context, *head, tail));
        }
    }

    Ok(Bracketed { binders, expressions, types })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_ASSOCIATIVITIES: [Associativity; 3] =
        [Associativity::None, Associativity::Left, Associativity::Right];

    /// PureScript precedences range from 0 to 9.
    const PRECEDENCE_RANGE: std::ops::RangeInclusive<u8> = 0..=9;

    /// A lower-precedence operator must always yield to a recursive call
    /// from a higher-precedence operator. Without sufficient spacing in
    /// the binding power encoding, adjacent precedences can collide.
    ///
    /// Let `bp(a, p) = (l, r)` denote the binding power function. Then:
    ///
    /// ```text
    /// forall p1, p2 in 0..=9, a1, a2 in {None, Left, Right}.
    ///   p1 < p2 => fst(bp(a1, p1)) < snd(bp(a2, p2))
    /// ```
    #[test]
    fn lower_precedence_yields() {
        for p_high in PRECEDENCE_RANGE {
            for p_low in 0..p_high {
                for &assoc_high in &ALL_ASSOCIATIVITIES {
                    let (_, right_bp_high) = binding_power(assoc_high, p_high);
                    for &assoc_low in &ALL_ASSOCIATIVITIES {
                        let (left_bp_low, _) = binding_power(assoc_low, p_low);
                        assert!(
                            left_bp_low < right_bp_high,
                            "precedence {p_low} ({assoc_low:?}) should yield to \
                             precedence {p_high} ({assoc_high:?}): \
                             lbp {left_bp_low} must be < rbp {right_bp_high}"
                        );
                    }
                }
            }
        }
    }

    /// A higher-precedence operator must bind tighter than a recursive
    /// call from a lower-precedence operator.
    ///
    /// ```text
    /// forall p1, p2 in 0..=9, a1, a2 in {None, Left, Right}.
    ///   p1 < p2 => fst(bp(a2, p2)) >= snd(bp(a1, p1))
    /// ```
    #[test]
    fn higher_precedence_binds() {
        for p_low in PRECEDENCE_RANGE {
            for p_high in (p_low + 1)..=9 {
                for &assoc_low in &ALL_ASSOCIATIVITIES {
                    let (_, right_bp_low) = binding_power(assoc_low, p_low);
                    for &assoc_high in &ALL_ASSOCIATIVITIES {
                        let (left_bp_high, _) = binding_power(assoc_high, p_high);
                        assert!(
                            left_bp_high >= right_bp_low,
                            "precedence {p_high} ({assoc_high:?}) should bind inside \
                             precedence {p_low} ({assoc_low:?}): \
                             lbp {left_bp_high} must be >= rbp {right_bp_low}"
                        );
                    }
                }
            }
        }
    }

    /// Left-associative operators at the same precedence continue the
    /// current branch rather than recursing deeper.
    ///
    /// ```text
    /// forall p in 0..=9. fst(bp(Left, p)) < snd(bp(Left, p))
    /// ```
    #[test]
    fn left_associative_chains_left() {
        for p in PRECEDENCE_RANGE {
            let (left_bp, right_bp) = binding_power(Associativity::Left, p);
            assert!(
                left_bp < right_bp,
                "left-associative at precedence {p}: lbp {left_bp} must be < rbp {right_bp}"
            );
        }
    }

    /// Right-associative operators at the same precedence recurse deeper
    /// into the right subtree.
    ///
    /// ```text
    /// forall p in 0..=9. fst(bp(Right, p)) > snd(bp(Right, p))
    /// ```
    #[test]
    fn right_associative_chains_right() {
        for p in PRECEDENCE_RANGE {
            let (left_bp, right_bp) = binding_power(Associativity::Right, p);
            assert!(
                left_bp > right_bp,
                "right-associative at precedence {p}: lbp {left_bp} must be > rbp {right_bp}"
            );
        }
    }
}
