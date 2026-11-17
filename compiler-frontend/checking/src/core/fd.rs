use std::sync::Arc;

use building_types::QueryResult;
use files::FileId;
use indexing::TypeItemId;
use smallvec::SmallVec;

use crate::context::CheckContext;
use crate::state::CheckState;
use crate::{ExternalQueries, safe_loop};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Positions {
    words: SmallVec<[u64; 1]>,
}

impl Positions {
    const WORD_BITS: usize = u64::BITS as usize;

    pub fn all(count: usize) -> Positions {
        Positions::from_iter(0..count)
    }

    pub fn insert(&mut self, position: usize) -> bool {
        let (word, bit) = (position / Positions::WORD_BITS, position % Positions::WORD_BITS);
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        let inserted = self.words[word] & (1 << bit) == 0;
        self.words[word] |= 1 << bit;
        inserted
    }

    pub fn remove(&mut self, position: usize) {
        let (word, bit) = (position / Positions::WORD_BITS, position % Positions::WORD_BITS);
        if let Some(bits) = self.words.get_mut(word) {
            *bits &= !(1 << bit);
        }
        while self.words.last() == Some(&0) {
            self.words.pop();
        }
    }

    pub fn contains(&self, position: usize) -> bool {
        let (word, bit) = (position / Positions::WORD_BITS, position % Positions::WORD_BITS);
        self.words.get(word).is_some_and(|bits| bits & (1 << bit) != 0)
    }

    pub fn len(&self) -> usize {
        self.words.iter().map(|bits| bits.count_ones() as usize).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn is_subset(&self, other: &Positions) -> bool {
        self.words.iter().enumerate().all(|(index, &bits)| {
            let other = other.words.get(index).copied().unwrap_or(0);
            bits & !other == 0
        })
    }

    pub fn union_with(&mut self, other: &Positions) -> bool {
        if other.words.len() > self.words.len() {
            self.words.resize(other.words.len(), 0);
        }
        let mut changed = false;
        for (bits, &other) in self.words.iter_mut().zip(other.words.iter()) {
            changed |= other & !*bits != 0;
            *bits |= other;
        }
        changed
    }
}

impl FromIterator<usize> for Positions {
    fn from_iter<I: IntoIterator<Item = usize>>(positions: I) -> Positions {
        let mut result = Positions::default();
        for position in positions {
            result.insert(position);
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fd {
    pub determiners: Positions,
    pub determined: Positions,
}

impl Fd {
    pub fn new(
        determiners: impl IntoIterator<Item = usize>,
        determined: impl IntoIterator<Item = usize>,
    ) -> Fd {
        Fd {
            determiners: determiners.into_iter().collect(),
            determined: determined.into_iter().collect(),
        }
    }

    pub fn from_lowering(functional_dependency: &lowering::FunctionalDependency) -> Fd {
        Fd::new(
            functional_dependency.determiners.iter().map(|&position| position as usize),
            functional_dependency.determined.iter().map(|&position| position as usize),
        )
    }
}

pub fn get_functional_dependencies<Q>(
    state: &CheckState,
    context: &CheckContext<Q>,
    file_id: FileId,
    type_id: TypeItemId,
) -> QueryResult<Arc<[Fd]>>
where
    Q: ExternalQueries,
{
    let functional_dependencies = if file_id == context.id {
        state.checked.classes.get(&type_id).map(|class| Arc::clone(&class.functional_dependencies))
    } else {
        let checked = context.checked_dependency(file_id)?;
        checked.classes.get(&type_id).map(|class| Arc::clone(&class.functional_dependencies))
    };

    Ok(functional_dependencies.unwrap_or_default())
}

pub fn get_all_determined(functional_dependencies: &[Fd]) -> Positions {
    let mut determined = Positions::default();
    for functional_dependency in functional_dependencies {
        determined.union_with(&functional_dependency.determined);
    }
    determined
}

pub fn compute_closure(functional_dependencies: &[Fd], initial_positions: &Positions) -> Positions {
    let mut determined = initial_positions.clone();
    safe_loop! {
        let mut changed = false;

        for functional_dependency in functional_dependencies {
            if functional_dependency.determiners.is_subset(&determined) {
                changed |= determined.union_with(&functional_dependency.determined);
            }
        }

        if !changed {
            return determined;
        }
    }
}

/// Closure monotonicity makes this equivalent to `positions` containing a covering set.
/// Exact equality preserves behavior when a malformed dependency introduces out-of-range positions.
pub fn positions_cover_all(
    functional_dependencies: &[Fd],
    positions: &Positions,
    all_positions: &Positions,
) -> bool {
    compute_closure(functional_dependencies, positions) == *all_positions
}

#[cfg(test)]
mod tests;
