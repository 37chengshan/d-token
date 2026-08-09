//! Allocation instrumentation for the benchmark binary (PR-C7A/C).
//!
//! A global counting allocator wraps `System` and tallies allocation count
//! and bytes.  It is only ever installed in the benchmark *binary* — never in
//! the library and never in production crates — so it measures the resolved
//! path without changing any production code.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct CountingAllocator {
    alloc_count: AtomicU64,
    alloc_bytes: AtomicU64,
    live_bytes: AtomicU64,
}

impl CountingAllocator {
    pub const fn new() -> Self {
        Self {
            alloc_count: AtomicU64::new(0),
            alloc_bytes: AtomicU64::new(0),
            live_bytes: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> AllocationSnapshot {
        AllocationSnapshot {
            count: self.alloc_count.load(Ordering::Relaxed),
            bytes: self.alloc_bytes.load(Ordering::Relaxed),
            live_bytes: self.live_bytes.load(Ordering::Relaxed),
        }
    }

    pub fn reset(&self) {
        self.alloc_count.store(0, Ordering::Relaxed);
        self.alloc_bytes.store(0, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllocationSnapshot {
    pub count: u64,
    pub bytes: u64,
    pub live_bytes: u64,
}

// SAFETY: the counters are atomic and the wrapper only delegates to System,
// which is a valid global allocator.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            self.alloc_count.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
            self.live_bytes
                .fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        self.live_bytes
            .fetch_sub(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            self.alloc_count.fetch_add(1, Ordering::Relaxed);
            self.alloc_bytes
                .fetch_add(new_size as u64, Ordering::Relaxed);
            self.live_bytes
                .fetch_sub(layout.size() as u64, Ordering::Relaxed);
            self.live_bytes.fetch_add(new_size as u64, Ordering::Relaxed);
        }
        new_pointer
    }
}
