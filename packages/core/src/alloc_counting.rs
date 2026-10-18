//! Test-only allocation counter: the `#[global_allocator]` behind the C7
//! allocation-guard tests. Production builds never compile this module.
//!
//! The counter is per thread (`thread_local!`), so tests running in
//! parallel cannot pollute each other's measurements: a test measures the
//! thread its audio work runs on, and only that thread. `try_with` is used
//! everywhere because the allocator also runs during thread-local storage
//! teardown, where `LocalKey::with` would panic.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    /// Const-initialised: creating the counter must not allocate.
    static ALLOCATIONS: Cell<u64> = const { Cell::new(0) };
}

/// Allocations performed by the calling thread since it started.
pub(crate) fn thread_allocations() -> u64 {
    ALLOCATIONS.try_with(|count| count.get()).unwrap_or(0)
}

fn count() {
    let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
}

/// [`System`] with a per-thread allocation counter in front of it.
pub(crate) struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count();
        System.alloc(layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count();
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count();
        System.realloc(ptr, layout, new_size)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }
}
