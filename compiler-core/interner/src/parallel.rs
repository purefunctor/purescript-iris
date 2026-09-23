//! Concurrent interner backed by [`boxcar`] and sharded [`HashTable`]s.
//!
//! Values are stored in an append-only arena, so references handed out by
//! lookups stay valid while other threads intern. Deduplication tables are
//! split into shards selected by the value's hash; each shard stores the
//! hash beside the arena index, so tables grow without rehashing values and
//! collisions resolve by comparing against the arena.
//!
//! Lookups that hit an existing value take the shard lock briefly. Sharding
//! keeps this uncontended in practice, and avoids the per-entry allocation
//! that a lock-free map requires on every insertion.

use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;
use std::num::NonZeroU32;

use hashbrown::HashTable;
use parking_lot::Mutex;
use rustc_hash::FxBuildHasher;

use crate::Id;

const SHARD_BITS: u32 = 5;
const SHARDS: usize = 1 << SHARD_BITS;

// Aligning shards to separate cache lines prevents threads that lock
// neighbouring shards from contending on the same line.
#[repr(align(128))]
struct Shard(Mutex<HashTable<(u64, NonZeroU32)>>);

pub struct Interner<T, M = ()>
where
    T: Send + Sync + 'static,
    M: Copy + Send + Sync + 'static,
{
    arena: boxcar::Vec<(T, M)>,
    shards: Box<[Shard]>,
    phantom: PhantomData<fn() -> T>,
}

impl<T, M> Default for Interner<T, M>
where
    T: Send + Sync + 'static,
    M: Copy + Send + Sync + 'static,
{
    fn default() -> Interner<T, M> {
        Interner::with_capacity(0)
    }
}

impl<T, M> Interner<T, M>
where
    T: Send + Sync + 'static,
    M: Copy + Send + Sync + 'static,
{
    pub fn with_capacity(capacity: usize) -> Interner<T, M> {
        let shard_capacity = capacity.div_ceil(SHARDS);
        let shards = (0..SHARDS)
            .map(|_| Shard(Mutex::new(HashTable::with_capacity(shard_capacity))))
            .collect();
        Interner { arena: boxcar::Vec::with_capacity(capacity), shards, phantom: PhantomData }
    }

    fn shard(&self, hash: u64) -> &Mutex<HashTable<(u64, NonZeroU32)>> {
        // hashbrown consumes the top 7 bits for control bytes and the low bits for
        // bucket selection, so select shards from bits that neither uses heavily.
        let index = (hash >> (64 - 7 - SHARD_BITS)) as usize & (SHARDS - 1);
        &self.shards[index].0
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
        let mut table = self.shard(hash).lock();

        let equivalent = |&(entry_hash, id): &(u64, NonZeroU32)| {
            entry_hash == hash && self.arena_value(id) == &value
        };
        if let Some(&(_, id)) = table.find(hash, equivalent) {
            return Id::new(id);
        }

        let index = self.arena.push((value, metadata));
        let id = unsafe { NonZeroU32::new_unchecked(index as u32 + 1) };
        table.insert_unique(hash, (hash, id), |&(entry_hash, _)| entry_hash);
        Id::new(id)
    }

    pub fn get(&self, value: &T) -> Option<Id<T>> {
        let hash = FxBuildHasher.hash_one(value);
        let table = self.shard(hash).lock();
        let equivalent = |&(entry_hash, id): &(u64, NonZeroU32)| {
            entry_hash == hash && self.arena_value(id) == value
        };
        table.find(hash, equivalent).map(|&(_, id)| Id::new(id))
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
    fn test_hash_collisions() {
        #[derive(Debug, PartialEq, Eq)]
        struct Colliding(u32);

        impl std::hash::Hash for Colliding {
            fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
                state.write_u32(0);
            }
        }

        let interner: Interner<Colliding> = Interner::default();

        let ids: Vec<_> = (0..64).map(|value| interner.intern(Colliding(value))).collect();

        for (value, &id) in ids.iter().enumerate() {
            assert_eq!(interner[id], Colliding(value as u32));
            assert_eq!(interner.intern(Colliding(value as u32)), id);
            assert_eq!(interner.get(&Colliding(value as u32)), Some(id));
        }
        assert_eq!(interner.get(&Colliding(64)), None);
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
