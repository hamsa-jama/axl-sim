//! A counting global allocator for tests of the "no heap allocation in the step loop" rule.
//! It lives here because this is the only crate allowed `unsafe`.
//!
//! In a test binary: `#[global_allocator] static A: CountingAlloc = CountingAlloc;` and then
//! [`count_allocs`] around the code under test. The count covers every thread of the process, so
//! run such a test alone in its binary.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static ACTIVE: AtomicBool = AtomicBool::new(false);
static COUNT: AtomicUsize = AtomicUsize::new(0);

/// The allocator: the system allocator plus a counter that is on only inside [`count_allocs`].
pub struct CountingAlloc;

// SAFETY: every method forwards to `System` with the same arguments; the counter is an atomic.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` came from `alloc` above with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ACTIVE.load(Ordering::Relaxed) {
            COUNT.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: `ptr` came from `alloc` above with this layout.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Run `f` and return its result with the number of heap allocations it made (needs
/// [`CountingAlloc`] installed as the global allocator, otherwise the count is 0).
pub fn count_allocs<T>(f: impl FnOnce() -> T) -> (T, usize) {
    COUNT.store(0, Ordering::SeqCst);
    ACTIVE.store(true, Ordering::SeqCst);
    let out = f();
    ACTIVE.store(false, Ordering::SeqCst);
    (out, COUNT.load(Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[global_allocator]
    static A: CountingAlloc = CountingAlloc;

    #[test]
    fn counts_allocations_only_inside_the_closure() {
        let (_, none) = count_allocs(|| 1 + 1);
        assert_eq!(none, 0);
        let (v, some) = count_allocs(|| vec![1u8; 100]);
        assert_eq!(v.len(), 100);
        assert!(some >= 1);
        let (_, after) = count_allocs(|| ());
        assert_eq!(after, 0);
    }
}
