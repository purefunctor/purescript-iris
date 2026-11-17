use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use mimalloc::MiMalloc;
use serde::{Deserialize, Serialize};

pub struct CountingAllocator;

static COUNTING: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static REALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationCounts {
    pub allocations: u64,
    pub reallocations: u64,
    pub allocated_bytes: u64,
}

impl AllocationCounts {
    pub fn since(self, earlier: AllocationCounts) -> AllocationCounts {
        AllocationCounts {
            allocations: self.allocations - earlier.allocations,
            reallocations: self.reallocations - earlier.reallocations,
            allocated_bytes: self.allocated_bytes - earlier.allocated_bytes,
        }
    }
}

pub fn enable_counting() {
    COUNTING.store(true, Ordering::Relaxed);
}

pub fn counts() -> AllocationCounts {
    AllocationCounts {
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        reallocations: REALLOCATIONS.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

fn record(counter: &AtomicU64, bytes: usize) {
    if COUNTING.load(Ordering::Relaxed) {
        counter.fetch_add(1, Ordering::Relaxed);
        ALLOCATED_BYTES.fetch_add(bytes as u64, Ordering::Relaxed);
    }
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(&ALLOCATIONS, layout.size());
        unsafe { MiMalloc.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(&ALLOCATIONS, layout.size());
        unsafe { MiMalloc.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { MiMalloc.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(&REALLOCATIONS, new_size);
        unsafe { MiMalloc.realloc(pointer, layout, new_size) }
    }
}
