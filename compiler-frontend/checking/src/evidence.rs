//! Type class evidence produced by constraint solving.
//!
//! Evidence variables identify wanted occurrences rather than canonical
//! constraints. Multiple occurrences of the same constraint therefore remain
//! distinguishable even when the solver deduplicates their work.

use std::ops::{Index, IndexMut};

use files::FileId;
use indexing::TypeItemId;

use crate::TypeId;
pub use crate::core::constraint::instances::InstanceCandidateOrigin;

/// A solver-written evidence variable for one wanted occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvidenceVarId(pub u32);

/// An identifier for a proof in a module's evidence graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvidenceId(pub u32);

/// A dictionary parameter introduced by a given or generalised constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvidenceBinderId(pub u32);

/// Stable identity of a superclass declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SuperclassId {
    pub file_id: FileId,
    pub type_id: TypeItemId,
    pub source_id: lowering::TypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceState {
    Unsolved,
    Solved(EvidenceId),
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceEntry {
    pub state: EvidenceState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceBinder {
    /// The canonical constraint represented by this dictionary parameter.
    pub constraint: TypeId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynthesizedEvidence {
    IsSymbol(lowering::StringLiteral),
    Reflectable(ReflectableEvidence),
    AbortIdentity(lowering::StringLiteral),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReflectableEvidence {
    Integer(i32),
    String(lowering::StringLiteral),
    Boolean(bool),
    Ordering(ReflectableOrdering),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReflectableOrdering {
    Less,
    Equal,
    Greater,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    /// Indirection from a deduplicated wanted occurrence to its representative.
    Variable(EvidenceVarId),
    /// A dictionary parameter in scope.
    Given(EvidenceBinderId),
    /// A declared or derived instance applied to its prerequisite dictionaries.
    Instance { origin: InstanceCandidateOrigin, subgoals: Vec<EvidenceVarId> },
    /// A superclass dictionary projected from another dictionary.
    Superclass { parent: EvidenceId, superclass: SuperclassId },
    /// Compiler-solved evidence with no materialised dictionary contents.
    ///
    /// This proof does not remove its evidence application. Evidence
    /// abstractions introduce binders independently of the supplied proof.
    Trivial,
    /// Compiler-known evidence which must be materialised at runtime.
    Synthesized(SynthesizedEvidence),
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Evidences {
    variables: Vec<EvidenceEntry>,
    binders: Vec<EvidenceBinder>,
    proofs: Vec<Evidence>,
}

impl Evidences {
    pub fn fresh_variable(&mut self) -> EvidenceVarId {
        let id = EvidenceVarId(self.variables.len() as u32);
        let entry = EvidenceEntry { state: EvidenceState::Unsolved };
        self.variables.push(entry);
        id
    }

    pub fn fresh_binder(&mut self, constraint: TypeId) -> EvidenceBinderId {
        let id = EvidenceBinderId(self.binders.len() as u32);
        self.binders.push(EvidenceBinder { constraint });
        id
    }

    pub fn bind_binder(&mut self, id: EvidenceBinderId, constraint: TypeId) {
        self[id].constraint = constraint;
    }

    pub fn allocate(&mut self, evidence: Evidence) -> EvidenceId {
        let id = EvidenceId(self.proofs.len() as u32);
        self.proofs.push(evidence);
        id
    }

    pub fn solve(&mut self, id: EvidenceVarId, evidence: EvidenceId) {
        let entry = &mut self[id];
        debug_assert!(
            matches!(entry.state, EvidenceState::Unsolved),
            "invariant violated: evidence variable solved more than once",
        );
        if matches!(entry.state, EvidenceState::Unsolved) {
            entry.state = EvidenceState::Solved(evidence);
        }
    }

    pub fn merge_duplicate(&mut self, duplicate: EvidenceVarId, representative: EvidenceVarId) {
        if duplicate == representative {
            return;
        }

        match self[duplicate].state {
            EvidenceState::Unsolved => {}
            EvidenceState::Solved(proof) if self[proof] == Evidence::Variable(representative) => {
                return;
            }
            EvidenceState::Solved(_) | EvidenceState::Error => {
                unreachable!("invariant violated: resolved evidence occurrence deduplicated");
            }
        }

        let evidence = self.allocate(Evidence::Variable(representative));
        self.solve(duplicate, evidence);
    }

    pub fn mark_error(&mut self, id: EvidenceVarId) {
        let entry = &mut self[id];
        match entry.state {
            EvidenceState::Unsolved => {
                entry.state = EvidenceState::Error;
            }
            EvidenceState::Error => {
                // EvidenceState::Error is idempotent
            }
            EvidenceState::Solved(_) => {
                unreachable!("invariant violated: solved evidence variable marked as error");
            }
        }
    }

    pub fn variables(&self) -> impl Iterator<Item = (EvidenceVarId, &EvidenceEntry)> {
        let variables = self.variables.iter().enumerate();
        variables.map(|(index, entry)| (EvidenceVarId(index as u32), entry))
    }

    pub fn binders(&self) -> impl Iterator<Item = (EvidenceBinderId, &EvidenceBinder)> {
        let binders = self.binders.iter().enumerate();
        binders.map(|(index, binder)| (EvidenceBinderId(index as u32), binder))
    }

    pub fn assert_finished(&self) {
        assert!(
            self.variables().all(|(_, entry)| {
                matches!(entry.state, EvidenceState::Solved(_) | EvidenceState::Error)
            }),
            "invariant violated: unsolved evidence variables remain",
        );
    }
}

impl Index<EvidenceVarId> for Evidences {
    type Output = EvidenceEntry;

    fn index(&self, EvidenceVarId(index): EvidenceVarId) -> &EvidenceEntry {
        &self.variables[index as usize]
    }
}

impl IndexMut<EvidenceVarId> for Evidences {
    fn index_mut(&mut self, EvidenceVarId(index): EvidenceVarId) -> &mut EvidenceEntry {
        &mut self.variables[index as usize]
    }
}

impl Index<EvidenceBinderId> for Evidences {
    type Output = EvidenceBinder;

    fn index(&self, EvidenceBinderId(index): EvidenceBinderId) -> &EvidenceBinder {
        &self.binders[index as usize]
    }
}

impl IndexMut<EvidenceBinderId> for Evidences {
    fn index_mut(&mut self, EvidenceBinderId(index): EvidenceBinderId) -> &mut EvidenceBinder {
        &mut self.binders[index as usize]
    }
}

impl Index<EvidenceId> for Evidences {
    type Output = Evidence;

    fn index(&self, EvidenceId(index): EvidenceId) -> &Evidence {
        &self.proofs[index as usize]
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::{Evidence, EvidenceState, Evidences};
    use crate::TypeId;

    #[test]
    fn trivial_proofs_solve_wanted_occurrences() {
        let mut evidences = Evidences::default();
        let wanted = evidences.fresh_variable();
        let trivial = evidences.allocate(Evidence::Trivial);

        evidences.solve(wanted, trivial);

        assert_eq!(evidences[wanted].state, EvidenceState::Solved(trivial));
        assert_eq!(evidences[trivial], Evidence::Trivial);
    }

    #[test]
    fn marking_an_error_is_idempotent() {
        let mut evidences = Evidences::default();
        let wanted = evidences.fresh_variable();

        evidences.mark_error(wanted);
        evidences.mark_error(wanted);

        assert_eq!(evidences[wanted].state, EvidenceState::Error);
    }

    #[test]
    fn duplicate_occurrences_retain_alias_indirection() {
        let mut evidences = Evidences::default();
        let representative = evidences.fresh_variable();
        let duplicate = evidences.fresh_variable();

        evidences.merge_duplicate(duplicate, representative);

        let EvidenceState::Solved(proof) = evidences[duplicate].state else {
            panic!("duplicate evidence was not solved");
        };
        assert_eq!(evidences[proof], Evidence::Variable(representative));
        assert!(matches!(evidences[representative].state, EvidenceState::Unsolved));

        evidences.merge_duplicate(duplicate, representative);
    }

    #[test]
    fn every_occurrence_has_distinct_identity() {
        let constraint = TypeId::new(NonZeroU32::new(1).unwrap());
        let mut evidences = Evidences::default();

        assert_ne!(evidences.fresh_variable(), evidences.fresh_variable());
        assert_ne!(evidences.fresh_binder(constraint), evidences.fresh_binder(constraint));
    }
}
