//! Concurrent interner backed by [`boxcar`] and sharded open-addressing tables.
//!
//! Values live in a [`boxcar::Vec`] arena, which appends without moving
//! earlier values, so an [`Id`] is an index into the arena that stays valid
//! for as long as the interner lives. The tables only deduplicate: they find
//! the [`Id`] of a value that is already in the arena.
//!
//! The [`FxBuildHasher`] hash of a value picks one of 1024 shards and a
//! 32-bit tag. Each shard's table is an array of [`AtomicU64`] slots, probed
//! linearly, and each slot packs a tag and an [`Id`] into one word. A probe
//! reads one word and reads the arena only when the tag matches, so most
//! mismatches never touch the arena. The interner used to be built on
//! `papaya`, where a lookup loaded a metadata byte, an entry pointer, and a
//! boxed entry before it could compare values, and every insert allocated an
//! entry; that was most of the cost of interning while checking.
//!
//! Lookups take no lock. A lookup that reaches a vacant slot takes the
//! shard's lock and probes again, because another thread may have inserted
//! the same value or grown the table since the first probe. An insert pushes
//! the value to the arena before it stores the slot with release ordering,
//! so any thread that sees the slot can read the value.
//!
//! A table grows when it is half full. The grown table goes into the next of
//! the shard's generation slots and is published by advancing the shard's
//! generation. Retired tables are not freed until the interner is dropped,
//! because a thread may still be probing one without the lock. Checking the
//! `acme` corpus keeps about 50 MB more at its peak because of this; in
//! return the tables own no allocation per entry, and dropping an engine
//! after checking `acme` at 8 threads takes about 49 ms instead of 370 ms.
//!
//! The shard lock is the only place where threads wait for each other, which
//! sets the shard count. With 64 shards, 1.9% of locked inserts found the lock
//! held at 8 threads, and checking `acme` was about 20% slower than with
//! `papaya`. With 1024 shards, about 0.1% do.
//!
//! The TLA+ model in `verification/` describes this design, and
//! `verification/check.sh` checks it along with mutants that break each
//! ordering and locking rule above.

use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use parking_lot::Mutex;
use rustc_hash::FxBuildHasher;

use crate::{Id, id_from_index};

const SHARDS: usize = 1024;

const MINIMUM_SLOTS: usize = 16;

const GENERATIONS: usize = 32;

const SPREAD: u64 = 0x9e37_79b9_7f4a_7c15;

const VACANT: u64 = 0;

pub struct Interner<T, M = ()>
where
    T: Send + Sync + 'static,
    M: Copy + Send + Sync + 'static,
{
    arena: boxcar::Vec<(T, M)>,
    shards: Box<[Shard]>,
    phantom: PhantomData<fn() -> T>,
}

#[repr(align(128))]
struct Shard {
    generation: AtomicUsize,
    tables: [OnceLock<Box<[AtomicU64]>>; GENERATIONS],
    occupied: Occupied,
}

#[repr(align(128))]
struct Occupied(Mutex<usize>);

struct Location {
    shard: usize,
    tag: u32,
}

enum Probe {
    Found(NonZeroU32),
    Vacant(usize),
}

impl Location {
    fn of<T: Hash + ?Sized>(value: &T) -> Location {
        let hash = FxBuildHasher.hash_one(value);
        let product = u128::from(hash) * u128::from(SPREAD);
        let folded = (product as u64) ^ ((product >> 64) as u64);
        let shard = folded as usize % SHARDS;
        let tag = (folded >> 32) as u32;
        Location { shard, tag }
    }
}

#[inline]
fn encode_slot(tag: u32, id: NonZeroU32) -> u64 {
    (u64::from(tag) << 32) | u64::from(id.get())
}

#[inline]
fn decode_slot(slot: u64) -> Option<(u32, NonZeroU32)> {
    let id = NonZeroU32::new(slot as u32)?;
    Some(((slot >> 32) as u32, id))
}

#[inline]
fn home(table: &[AtomicU64], tag: u32) -> usize {
    tag as usize & (table.len() - 1)
}

fn probe(
    table: &[AtomicU64],
    tag: u32,
    start: usize,
    mut matches: impl FnMut(NonZeroU32) -> bool,
) -> Probe {
    let mask = table.len() - 1;
    let mut index = start;
    loop {
        let Some((slot_tag, id)) = decode_slot(table[index].load(Ordering::Acquire)) else {
            return Probe::Vacant(index);
        };
        if slot_tag == tag && matches(id) {
            return Probe::Found(id);
        }
        index = (index + 1) & mask;
    }
}

fn allocate_table(slots: usize) -> Box<[AtomicU64]> {
    (0..slots).map(|_| AtomicU64::new(VACANT)).collect()
}

impl Shard {
    fn with_capacity(capacity: usize) -> Shard {
        let slots = capacity.saturating_mul(2).next_power_of_two().max(MINIMUM_SLOTS);
        let tables: [OnceLock<Box<[AtomicU64]>>; GENERATIONS] = Default::default();
        tables[0].get_or_init(|| allocate_table(slots));
        Shard { generation: AtomicUsize::new(0), tables, occupied: Occupied(Mutex::new(0)) }
    }

    #[inline]
    fn table(&self) -> (usize, &[AtomicU64]) {
        let generation = self.generation.load(Ordering::Acquire);
        if let Some(table) = self.tables[generation].get() {
            (generation, table)
        } else {
            unreachable!("invariant violated: {generation} is not a valid generation");
        }
    }

    fn grow(&self, generation: usize, table: &[AtomicU64]) {
        let grown = allocate_table(table.len() * 2);
        let mask = grown.len() - 1;
        for slot in table {
            let slot = slot.load(Ordering::Relaxed);
            let Some((tag, _)) = decode_slot(slot) else {
                continue;
            };
            let mut index = tag as usize & mask;
            while grown[index].load(Ordering::Relaxed) != VACANT {
                index = (index + 1) & mask;
            }
            grown[index].store(slot, Ordering::Relaxed);
        }

        let next = generation + 1;
        self.tables[next].get_or_init(|| grown);
        self.generation.store(next, Ordering::Release);
    }
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
        Interner {
            arena: boxcar::Vec::new(),
            shards: (0..SHARDS).map(|_| Shard::with_capacity(shard_capacity)).collect(),
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
        let Location { shard, tag } = Location::of(&value);
        let shard = &self.shards[shard];

        let matches = |id| self.arena_value(id) == &value;
        let (observed, table) = shard.table();
        let vacant = match probe(table, tag, home(table, tag), matches) {
            Probe::Found(id) => return Id::new(id),
            Probe::Vacant(index) => index,
        };

        let mut occupied = shard.occupied.0.lock();
        let (generation, table) = shard.table();
        let start = if generation == observed { vacant } else { home(table, tag) };
        let index = match probe(table, tag, start, matches) {
            Probe::Found(id) => return Id::new(id),
            Probe::Vacant(index) => index,
        };

        let id = id_from_index(self.arena.push((value, metadata)));
        table[index].store(encode_slot(tag, id), Ordering::Release);
        *occupied += 1;
        if *occupied * 2 > table.len() {
            shard.grow(generation, table);
        }

        Id::new(id)
    }

    pub fn get(&self, value: &T) -> Option<Id<T>> {
        let Location { shard, tag } = Location::of(value);
        let shard = &self.shards[shard];

        let matches = |id| self.arena_value(id) == value;
        let (observed, table) = shard.table();
        let vacant = match probe(table, tag, home(table, tag), matches) {
            Probe::Found(id) => return Some(Id::new(id)),
            Probe::Vacant(index) => index,
        };

        let _occupied = shard.occupied.0.lock();
        let (generation, table) = shard.table();
        let start = if generation == observed { vacant } else { home(table, tag) };
        match probe(table, tag, start, matches) {
            Probe::Found(id) => Some(Id::new(id)),
            Probe::Vacant(_) => None,
        }
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

    pub fn count(&self) -> usize {
        self.arena.count()
    }

    pub fn metadata_values(&self) -> impl Iterator<Item = M> + '_ {
        self.arena.iter().map(|(_, (_, metadata))| *metadata)
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

        let mut handles = Vec::new();
        for thread_id in 0..128 {
            let interner = Arc::clone(&interner);
            handles.push(thread::spawn(move || {
                let mut interned = Vec::new();
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
