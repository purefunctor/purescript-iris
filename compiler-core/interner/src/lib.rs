use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::{any, fmt, ops};

use hashbrown::{Equivalent, HashTable};
use rustc_hash::FxBuildHasher;

pub mod parallel;

pub struct Id<T> {
    pub(crate) id: NonZeroU32,
    phantom: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    pub const fn new(id: NonZeroU32) -> Self {
        Id { id, phantom: PhantomData }
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("Id<{}>({})", any::type_name::<T>(), self.id))
    }
}

impl<T> Copy for Id<T> {}

impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<T> Eq for Id<T> {}

impl<T> PartialOrd for Id<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Ord for Id<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl<T> Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Debug)]
pub struct Interner<T> {
    inner: Vec<T>,
    table: HashTable<NonZeroU32>,
}

impl<T> Default for Interner<T> {
    fn default() -> Interner<T> {
        let inner = Vec::new();
        let table = HashTable::default();
        Interner { inner, table }
    }
}

impl<T: PartialEq> PartialEq for Interner<T> {
    fn eq(&self, other: &Interner<T>) -> bool {
        self.inner == other.inner
    }
}

impl<T: Eq> Eq for Interner<T> {}

impl<T: Eq + Hash> Interner<T> {
    pub fn intern(&mut self, value: T) -> Id<T> {
        let hash = FxBuildHasher.hash_one(&value);

        let existing =
            self.table.find(hash, |&id| arena_equivalent(&self.inner, id, &value)).copied();

        let id = existing.unwrap_or_else(|| {
            self.inner.push(value);
            let index = self.inner.len();
            // SAFETY: Vec::push ensures that the subsequent Vec::len
            // returns a non-zero value to be used as a 1-based index.
            let id = unsafe { NonZeroU32::new_unchecked(index as u32) };
            self.table.insert_unique(hash, id, |&id| arena_hasher(&self.inner, id));
            id
        });

        Id::new(id)
    }

    pub fn get<Q>(&self, value: &Q) -> Option<Id<T>>
    where
        Q: ?Sized + Hash + Equivalent<T>,
    {
        let hash = FxBuildHasher.hash_one(value);
        let id = self.table.find(hash, |&id| arena_equivalent(&self.inner, id, value))?;
        Some(Id::new(*id))
    }

    pub fn shrink_to_fit(&mut self) {
        self.inner.shrink_to_fit();
        self.table.shrink_to_fit(|&id| arena_hasher(&self.inner, id));
    }
}

impl<T> ops::Index<Id<T>> for Interner<T> {
    type Output = T;

    fn index(&self, Id { id, .. }: Id<T>) -> &Self::Output {
        arena_index(&self.inner, id).unwrap_or_else(|| {
            unreachable!("invariant violated: {id} is not a valid index");
        })
    }
}

#[inline]
fn arena_index<T>(arena: &[T], id: NonZeroU32) -> Option<&T> {
    let index = id.get() as usize;
    arena.get(index - 1)
}

#[inline]
fn arena_hasher<T: Hash>(arena: &[T], id: NonZeroU32) -> u64 {
    let inner = arena_index(arena, id).unwrap_or_else(|| {
        unreachable!("invariant violated: {id} is not a valid index");
    });
    FxBuildHasher.hash_one(inner)
}

#[inline]
fn arena_equivalent<T, Q>(arena: &[T], id: NonZeroU32, value: &Q) -> bool
where
    T: Hash,
    Q: ?Sized + Hash + Equivalent<T>,
{
    let inner = arena_index(arena, id).unwrap_or_else(|| {
        unreachable!("invariant violated: {id} is not a valid index");
    });
    value.equivalent(inner)
}

#[cfg(test)]
mod tests {
    use super::Interner;

    #[test]
    fn test_basic() {
        let mut interner = Interner::default();

        let hello_a = interner.intern("hello");
        let hello_b = interner.intern("hello");

        assert_eq!(hello_a, hello_b);
    }

    #[test]
    fn test_eq() {
        let mut interner_a = Interner::default();
        interner_a.intern("hello");
        interner_a.intern("world");

        let mut interner_b = Interner::default();
        interner_b.intern("hello");
        interner_b.intern("world");

        let mut interner_c = Interner::default();
        interner_c.intern("world");
        interner_c.intern("hello");

        assert_eq!(interner_a, interner_b);
        assert_ne!(interner_a, interner_c);
        assert_ne!(interner_b, interner_c);
    }
}
