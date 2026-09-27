//! Concurrent interner backed by [`boxcar`] and sharded open-addressing tables.
//!
//! Values are stored in an append-only arena, so references handed out by
//! lookups stay valid while other threads intern. Deduplication tables are
//! split into shards selected by the value's hash. Each slot packs the low
//! half of the hash beside the arena index, so a probe compares values only
//! when the stored hash matches, and tables grow without rehashing values.
//!
//! Lookups never lock: a value that is already interned is found by reading
//! the shard's published table. Only insertions lock the shard, resuming the
//! probe from the vacant slot that the lock-free probe stopped at. Tables are
//! replaced rather than resized in place, and replaced tables stay allocated
//! until the interner is dropped, so concurrent readers never observe a
//! freed table. Since tables double in size, the retained tables occupy less
//! memory than the current one.

use std::hash::{BuildHasher, Hash};
use std::marker::PhantomData;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

use parking_lot::Mutex;
use rustc_hash::FxBuildHasher;

use crate::Id;

const SHARD_BITS: u32 = 5;
const SHARDS: usize = 1 << SHARD_BITS;

/// The smallest number of slots in a shard's table.
const MINIMUM_SLOTS: usize = 8;

/// A vacant slot; occupied slots are non-zero because arena indices are.
const VACANT: u64 = 0;

// Aligning writers to separate cache lines prevents threads that lock
// neighbouring shards from contending on the same line, and keeps lock
// traffic away from the table pointers that lock-free readers load.
#[repr(align(128))]
struct ShardWriter(Mutex<ShardTables>);

/// The tables allocated for a shard, owned by its writer lock.
///
/// Each table is a header slot holding the table's mask, followed by
/// `mask + 1` slots. The last table is the one published to readers.
struct ShardTables {
    length: usize,
    tables: Vec<Box<[AtomicU64]>>,
}

/// A borrowed view of a shard's published table.
#[derive(Clone, Copy)]
struct Table<'a> {
    mask: usize,
    slots: &'a [AtomicU64],
}

impl<'a> Table<'a> {
    #[inline]
    fn slot(self, position: usize) -> &'a AtomicU64 {
        // SAFETY: A table has `mask + 1` slots, so a masked position is in bounds.
        unsafe { self.slots.get_unchecked(position & self.mask) }
    }
}

enum Probe {
    Found(NonZeroU32),
    Vacant(usize),
}

pub struct Interner<T, M = ()>
where
    T: Send + Sync + 'static,
    M: Copy + Send + Sync + 'static,
{
    arena: boxcar::Vec<(T, M)>,
    published: [AtomicPtr<AtomicU64>; SHARDS],
    writers: [ShardWriter; SHARDS],
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
        let slots = slots_for_length(capacity.div_ceil(SHARDS));
        let writers: [ShardWriter; SHARDS] = std::array::from_fn(|_| {
            let tables = vec![allocate_table(slots)];
            ShardWriter(Mutex::new(ShardTables { length: 0, tables }))
        });
        let published = std::array::from_fn(|shard| {
            let ShardWriter(writer) = &writers[shard];
            AtomicPtr::new(writer.lock().current_header())
        });
        let arena = boxcar::Vec::with_capacity(capacity);
        Interner { arena, published, writers, phantom: PhantomData }
    }

    fn published_table(&self, shard: usize) -> Table<'_> {
        // `Acquire` synchronises with the `Release` store that published the
        // table, so its initialised slots are visible.
        let header = self.published[shard].load(Ordering::Acquire);
        // SAFETY: `published` only holds headers of this interner's tables.
        unsafe { self.table_at(header) }
    }

    /// # Safety
    ///
    /// `header` must be the start of a table owned by one of this interner's
    /// writers, as returned by [`ShardTables::current_header`].
    unsafe fn table_at(&self, header: *mut AtomicU64) -> Table<'_> {
        // SAFETY: Tables are never freed or moved before the interner is
        // dropped, and the returned view borrows `self`. `allocate_table`
        // stores the mask in the header, followed by exactly `mask + 1` slots.
        unsafe {
            let mask = (*header).load(Ordering::Relaxed) as usize;
            let slots = std::slice::from_raw_parts(header.add(1), mask + 1);
            Table { mask, slots }
        }
    }

    /// Returns both the value and metadata of `id` from a single arena lookup.
    pub fn value_with_metadata(&self, Id { id, .. }: Id<T>) -> (&T, M) {
        let index = id.get() - 1;
        let index = index as usize;
        if let Some((value, metadata)) = self.arena.get(index) {
            (value, *metadata)
        } else {
            unreachable!("invariant violated: {} is not a valid index", id)
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
        let shard = shard_index(hash);
        let tag = hash as u32;

        let observed = self.published_table(shard);
        let vacant = match self.probe(observed, tag as usize & observed.mask, tag, &value) {
            Probe::Found(id) => return Id::new(id),
            Probe::Vacant(position) => position,
        };

        let ShardWriter(writer) = &self.writers[shard];
        let mut writer = writer.lock();

        // Insertions only happen under this lock and slots are written once,
        // so if the table was not replaced, every slot before `vacant` on the
        // probe sequence is unchanged and the probe can resume from there.
        let current = self.published_table(shard);
        let start = if std::ptr::eq(current.slots, observed.slots) {
            vacant
        } else {
            tag as usize & current.mask
        };
        let (table, position) = match self.probe(current, start, tag, &value) {
            Probe::Found(id) => return Id::new(id),
            Probe::Vacant(position) => (current, position),
        };

        let (table, position) = if exceeds_load_factor(writer.length + 1, table.mask + 1) {
            let table = self.grow(shard, &mut writer, table);
            (table, vacant_position(table, tag))
        } else {
            (table, position)
        };

        let index = self.arena.push((value, metadata));
        let id = u32::try_from(index + 1).ok().and_then(NonZeroU32::new);
        let Some(id) = id else {
            panic!("interner exceeded {} values", u32::MAX);
        };

        // `Release` publishes the arena entry to readers that load this slot.
        table.slot(position).store(pack_slot(tag, id), Ordering::Release);
        writer.length += 1;

        Id::new(id)
    }

    pub fn get(&self, value: &T) -> Option<Id<T>> {
        let hash = FxBuildHasher.hash_one(value);
        let tag = hash as u32;
        let table = self.published_table(shard_index(hash));
        match self.probe(table, tag as usize & table.mask, tag, value) {
            Probe::Found(id) => Some(Id::new(id)),
            Probe::Vacant(_) => None,
        }
    }

    #[inline(always)]
    fn probe(&self, table: Table<'_>, start: usize, tag: u32, value: &T) -> Probe {
        let mut position = start;
        loop {
            let slot = table.slot(position).load(Ordering::Acquire);
            if slot == VACANT {
                return Probe::Vacant(position);
            }
            let (slot_tag, slot_id) = unpack_slot(slot);
            if slot_tag == tag
                && let Some(id) = slot_id
                && self.published_value(id) == value
            {
                return Probe::Found(id);
            }
            position = (position + 1) & table.mask;
        }
    }

    /// Reads the value of an id loaded from a slot of this interner's tables.
    #[inline]
    fn published_value(&self, id: NonZeroU32) -> &T {
        let index = id.get() - 1;
        // SAFETY: Slots only hold ids returned by `arena.push`, stored with
        // `Release` after the push completed. The slot was loaded with `Acquire`,
        // either from the table it was stored in, or from a replacement table
        // published with `Release` by `grow` under the writer lock that ordered
        // the original store before it. Either way, the entry is initialised.
        let (value, _) = unsafe { self.arena.get_unchecked(index as usize) };
        value
    }

    /// Publishes a table with twice the slots, containing every entry of `table`.
    fn grow<'a>(&'a self, shard: usize, writer: &mut ShardTables, table: Table<'a>) -> Table<'a> {
        writer.tables.push(allocate_table((table.mask + 1) * 2));
        let header = writer.current_header();

        // SAFETY: `header` was just returned by `current_header`.
        // Readers cannot see the replacement until it is published below.
        let replacement = unsafe { self.table_at(header) };
        for slot in table.slots {
            let slot = slot.load(Ordering::Relaxed);
            if slot != VACANT {
                let (tag, _) = unpack_slot(slot);
                let position = vacant_position(replacement, tag);
                replacement.slot(position).store(slot, Ordering::Relaxed);
            }
        }

        self.published[shard].store(header, Ordering::Release);
        replacement
    }
}

impl ShardTables {
    fn current_header(&self) -> *mut AtomicU64 {
        let Some(table) = self.tables.last() else {
            unreachable!("invariant violated: shard has no table");
        };
        // Readers only perform atomic operations through this pointer.
        table.as_ptr().cast_mut()
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

fn shard_index(hash: u64) -> usize {
    // FxHash moves its highest-entropy bits to the bottom of the hash, which
    // the table index uses; bits 20 and up stay clear of the index for tables
    // with fewer than a million slots per shard.
    (hash >> 20) as usize & (SHARDS - 1)
}

fn pack_slot(tag: u32, id: NonZeroU32) -> u64 {
    (u64::from(tag) << 32) | u64::from(id.get())
}

fn unpack_slot(slot: u64) -> (u32, Option<NonZeroU32>) {
    ((slot >> 32) as u32, NonZeroU32::new(slot as u32))
}

/// Keeping tables at most half full bounds the expected probe length.
fn exceeds_load_factor(length: usize, slots: usize) -> bool {
    length * 2 > slots
}

fn slots_for_length(length: usize) -> usize {
    (length * 2).next_power_of_two().max(MINIMUM_SLOTS)
}

fn allocate_table(slots: usize) -> Box<[AtomicU64]> {
    debug_assert!(slots.is_power_of_two());
    // Zeroed allocations can reuse memory the allocator knows is already
    // zeroed, which matters for the largest tables.
    let table = Box::<[AtomicU64]>::new_zeroed_slice(slots + 1);
    // SAFETY: `AtomicU64` has the same in-memory representation as `u64`, for
    // which all-zero bytes are valid; zero is also the `VACANT` slot.
    let table = unsafe { table.assume_init() };
    table[0].store(slots as u64 - 1, Ordering::Relaxed);
    table
}

fn vacant_position(table: Table<'_>, tag: u32) -> usize {
    let mut position = tag as usize & table.mask;
    while table.slot(position).load(Ordering::Relaxed) != VACANT {
        position = (position + 1) & table.mask;
    }
    position
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
    fn test_get() {
        let interner: Interner<u32> = Interner::default();

        let ids: Vec<_> = (0..1000).map(|value| interner.intern(value)).collect();

        for (value, id) in (0..1000).zip(ids) {
            assert_eq!(interner.get(&value), Some(id));
        }
        assert_eq!(interner.get(&1000), None);
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

    #[test]
    fn test_concurrent_growth() {
        let interner: Interner<u64> = Interner::default();
        let values = 0..50_000u64;

        // Threads intern the same values in different orders, so insertions
        // race with each other and with the table replacements they trigger.
        let results: Vec<Vec<_>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8u64)
                .map(|thread| {
                    let interner = &interner;
                    let values = values.clone();
                    scope.spawn(move || {
                        let offset = thread * 6151;
                        let interned = values.clone().map(|value| {
                            let value = (value + offset) % values.end;
                            (value, interner.intern(value))
                        });
                        let mut interned: Vec<_> = interned.collect();
                        interned.sort_unstable();
                        interned
                    })
                })
                .collect();
            handles.into_iter().map(|handle| handle.join().unwrap()).collect()
        });

        let [reference, remaining @ ..] = &results[..] else {
            unreachable!("invariant violated: empty results");
        };

        for interned in remaining {
            assert_eq!(reference, interned);
        }
        for &(value, id) in reference {
            assert_eq!(interner[id], value);
            assert_eq!(interner.get(&value), Some(id));
        }
    }
}
