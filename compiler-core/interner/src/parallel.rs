//! Lock-free interner backed by [`boxcar`] and [`papaya`].
//!
//! Since `papaya` currently does not expose its HashTable API, we use the
//! [`FxBuildHasher::hash_one`] of the value being interned. This seems
//! wasteful at first, but in practice [`u64`] hashing is cheap enough.
//!
//! For hash collisions, we use an overflow chain to emulate buckets in
//! a hashmap; on every lookup and insertion, we check that the value
//! stored in the arena is equal to the value stored in the deduplication
//! table, searching the overflow chain when the values are not equal.

use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;
use std::num::NonZeroU32;

use parking_lot::Mutex;
use rustc_hash::FxBuildHasher;

use crate::Id;

pub struct Interner<T, M = ()>
where
    T: Send + Sync + 'static,
    M: Copy + Send + Sync + 'static,
{
    arena: boxcar::Vec<(T, M)>,
    table: papaya::HashMap<u64, NonZeroU32, FxBuildHasher>,
    overflow: Mutex<Vec<(u64, NonZeroU32)>>,
    phantom: PhantomData<fn() -> T>,
}

impl<T, M> Default for Interner<T, M>
where
    T: Send + Sync + 'static,
    M: Copy + Send + Sync + 'static,
{
    fn default() -> Interner<T, M> {
        Interner {
            arena: boxcar::Vec::new(),
            table: papaya::HashMap::builder().hasher(FxBuildHasher).build(),
            overflow: Mutex::new(Vec::new()),
            phantom: PhantomData,
        }
    }
}

impl<T, M> Interner<T, M>
where
    T: Send + Sync + 'static,
    M: Copy + Send + Sync + 'static,
{
    pub fn with_capacity(capacity: usize) -> Interner<T, M> {
        Interner {
            arena: boxcar::Vec::with_capacity(capacity),
            table: papaya::HashMap::builder().capacity(capacity).hasher(FxBuildHasher).build(),
            overflow: Mutex::new(Vec::new()),
            phantom: PhantomData,
        }
    }
}

impl<T, M> Interner<T, M>
where
    T: Send + Sync + Eq + Hash + 'static,
    M: Copy + Send + Sync + Default + 'static,
{
    pub fn intern(&self, value: T) -> Id<T> {
        self.intern_with_metadata(value, M::default())
    }
}

impl<T, M> Interner<T, M>
where
    T: Send + Sync + Eq + Hash + 'static,
    M: Copy + Send + Sync + 'static,
{
    pub fn intern_with_metadata(&self, value: T, metadata: M) -> Id<T> {
        let hash = FxBuildHasher.hash_one(&value);
        let table = self.table.pin();

        if let Some(&id) = table.get(&hash) {
            if self.arena_value(id) == &value {
                return Id::new(id);
            }
            return self.intern_at_collision(hash, value, metadata);
        }

        let index = self.arena.push((value, metadata));
        let candidate = unsafe { NonZeroU32::new_unchecked(index as u32 + 1) };

        match table.try_insert(hash, candidate) {
            Ok(_) => Id::new(candidate),
            // Another thread has already published a value at this hash.
            // If the values are the same, abandon the current candidate
            // and return the existing `Id`. This sacrifices a handful
            // of bytes in arena slots for the sake of concurrency.
            Err(papaya::OccupiedError { current, .. }) => {
                if self.arena_value(*current) == self.arena_value(candidate) {
                    return Id::new(*current);
                }
                self.publish_collision(hash, candidate)
            }
        }
    }

    fn intern_at_collision(&self, hash: u64, value: T, metadata: M) -> Id<T> {
        let mut overflow = self.overflow.lock();
        if let Some(id) = self.scan_overflow(&overflow, hash, &value) {
            return Id::new(id);
        }

        let index = self.arena.push((value, metadata));
        let candidate = unsafe { NonZeroU32::new_unchecked(index as u32 + 1) };

        overflow.push((hash, candidate));
        Id::new(candidate)
    }

    fn publish_collision(&self, hash: u64, candidate: NonZeroU32) -> Id<T> {
        let mut overflow = self.overflow.lock();
        if let Some(id) = self.scan_overflow(&overflow, hash, self.arena_value(candidate)) {
            return Id::new(id);
        }

        overflow.push((hash, candidate));
        Id::new(candidate)
    }

    pub fn get(&self, value: &T) -> Option<Id<T>> {
        let hash = FxBuildHasher.hash_one(value);
        let table = self.table.pin();

        let id = table.get(&hash).copied()?;
        if self.arena_value(id) == value {
            return Some(Id::new(id));
        }

        let overflow = self.overflow.lock();
        self.scan_overflow(&overflow, hash, value).map(Id::new)
    }

    fn arena_value(&self, id: NonZeroU32) -> &T {
        let index = id.get() - 1;
        let index = index as usize;
        if let Some((value, _)) = self.arena.get(index) {
            value
        } else {
            unreachable!("invariant violated: {id} is not a valid index");
        }
    }

    fn scan_overflow(
        &self,
        overflow: &[(u64, NonZeroU32)],
        hash: u64,
        value: &T,
    ) -> Option<NonZeroU32> {
        for &(overflow_hash, overflow_id) in overflow.iter().rev() {
            if overflow_hash != hash {
                continue;
            }
            if self.arena_value(overflow_id) == value {
                return Some(overflow_id);
            }
        }
        None
    }

    pub fn metadata(&self, Id { id, .. }: Id<T>) -> M {
        let index = id.get() - 1;
        let index = index as usize;
        if let Some((_, metadata)) = self.arena.get(index) {
            *metadata
        } else {
            unreachable!("invariant violated: {} is not a valid index", id)
        }
    }
}

impl<T, M> std::ops::Index<Id<T>> for Interner<T, M>
where
    T: Send + Sync + 'static,
    M: Copy + Send + Sync + 'static,
{
    type Output = T;

    fn index(&self, Id { id, .. }: Id<T>) -> &T {
        let index = id.get() - 1;
        let index = index as usize;
        if let Some((value, _)) = self.arena.get(index) {
            value
        } else {
            unreachable!("invariant violated: {} is not a valid index", id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Interner;

    #[test]
    fn test_basic() {
        let interner: Interner<&'static str> = Interner::default();

        let a = interner.intern("hello");
        let b = interner.intern("hello");
        let c = interner.intern("world");

        assert_eq!(a, b);
        assert_ne!(a, c);

        assert_eq!(interner[a], "hello");
        assert_eq!(interner[c], "world");
    }

    #[test]
    fn test_with_metadata() {
        let interner: Interner<&'static str, u8> = Interner::default();

        let a = interner.intern_with_metadata("hello", 7);
        let b = interner.intern_with_metadata("hello", 99);

        assert_eq!(a, b);
        assert_eq!(interner.metadata(a), 7);
    }

    #[test]
    fn test_concurrent() {
        use std::sync::Arc;
        use std::thread;

        let interner: Arc<Interner<String>> = Arc::new(Interner::default());

        let mut handles = vec![];
        for thread_id in 0..128 {
            let interner = Arc::clone(&interner);
            handles.push(thread::spawn(move || {
                let mut interned = vec![];
                for i in 0..1000 {
                    interned.push(interner.intern(format!("k{}", i % 100)));
                }
                (thread_id, interned)
            }));
        }

        let results: Vec<_> = handles.into_iter().map(|handle| handle.join().unwrap()).collect();

        let [(_, reference), remaining @ ..] = &results[..] else {
            unreachable!("invariant violated: empty results");
        };

        for (_, interned) in remaining {
            assert_eq!(reference, interned);
        }
    }
}
