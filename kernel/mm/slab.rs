//! Slab allocator — the kernel's `#[global_allocator]`.
//!
//! Small requests are served from per-size-class free-lists carved out of buddy
//! pages; anything larger than the top class goes straight to the buddy in
//! whole pages. Freed objects are cached in their class's free-list and reused —
//! there is no slab-page reclaim yet (deferred to the SMP/per-CPU redesign in
//! Phase 4). Size-class selection, page carving, and the free-list are
//! Kani-verified; the `GlobalAlloc` glue and locking are reviewed by hand.

use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::NonNull,
};

use kernel_arch::mm::VirtAddr;
use kernel_builtin::sync::SpinLock;

use crate::{buddy::BUDDY, frame::PAGE_SIZE};

/// Size classes served from free-lists (bytes). A request rounds up to the
/// smallest class that fits its size and alignment; larger goes to the buddy.
pub const CLASSES: [usize; 9] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

/// Smallest size-class index fitting `size` and `align`, or None if it
/// exceeds the largest class (→ large/buddy path).
pub fn size_class_index(size: usize, align: usize) -> Option<usize> {
    let need = size.max(align);
    let mut i = 0;
    while i < CLASSES.len() {
        if CLASSES[i] >= need {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// How many objects of `CLASSES[idx]` fit in one page.
pub fn objects_per_page(idx: usize) -> usize {
    PAGE_SIZE / CLASSES[idx]
}

/// The global allocator: a [`Slab`] behind a spin-lock.
pub struct LockedSlab(SpinLock<Slab>);
impl LockedSlab {
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self(SpinLock::new(Slab::new()))
    }
}
unsafe impl Send for LockedSlab {}
unsafe impl Sync for LockedSlab {}

unsafe impl GlobalAlloc for LockedSlab {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match size_class_index(layout.size(), layout.align()) {
            Some(idx) => {
                let mut slab = self.0.lock();
                if slab.free_lists[idx].is_none() && unsafe { slab.refill(idx) }.is_none() {
                    return core::ptr::null_mut(); // OOM / buddy down
                }
                slab.pop(idx).map_or(core::ptr::null_mut(), |p| p.as_ptr())
            }
            None => unsafe { large_alloc(layout) }, // > 2048 → whole pages
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        match size_class_index(layout.size(), layout.align()) {
            Some(idx) => {
                let p = NonNull::new(ptr).unwrap();
                unsafe { self.0.lock().push(idx, p) };
            }
            None => unsafe { large_dealloc(ptr, layout) },
        }
    }
}

/// The registered global allocator instance.
#[global_allocator]
static ALLOCATOR: LockedSlab = LockedSlab::new();

/// Serve a request larger than the top size class as a run of whole buddy
/// pages. Returns null if the buddy is down or out of memory.
unsafe fn large_alloc(layout: Layout) -> *mut u8 {
    let order = pages_order(layout.size());
    match BUDDY.get().and_then(|b| b.lock().alloc_pages(order)) {
        Some(pa) => crate::layout::pa_to_linear_va(pa).as_usize() as *mut u8,
        None => core::ptr::null_mut(),
    }
}
/// Return a large (page-run) allocation to the buddy.
unsafe fn large_dealloc(ptr: *mut u8, layout: Layout) {
    let pa = crate::layout::linear_va_to_pa(VirtAddr::new(ptr as usize));
    if let Some(b) = BUDDY.get() {
        b.lock().free_pages(pa, pages_order(layout.size()));
    }
}

/// Buddy order covering `size` bytes: ceil_log2(ceil(size / PAGE_SIZE)).
fn pages_order(size: usize) -> u8 {
    size.div_ceil(PAGE_SIZE)
        .next_power_of_two()
        .trailing_zeros() as u8
}

/// Intrusive free-list node — lives in a freed object's own first bytes.
#[repr(C)]
struct FreeObject {
    next: Option<NonNull<FreeObject>>,
}
const _: () = assert!(CLASSES[0] >= size_of::<FreeObject>()); // min class holds a link

/// Per-size-class object free-lists. A class stays empty until an allocation
/// refills it from a fresh buddy page.
pub struct Slab {
    free_lists: [Option<NonNull<FreeObject>>; CLASSES.len()],
}

impl Slab {
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            free_lists: [None; CLASSES.len()],
        }
    }
    /// # Safety: `ptr` = ≥ CLASSES[idx] bytes of free, writable, aligned memory.
    unsafe fn push(&mut self, idx: usize, ptr: NonNull<u8>) {
        let node = ptr.cast::<FreeObject>();
        unsafe {
            node.as_ptr().write(FreeObject {
                next: self.free_lists[idx],
            })
        };
        self.free_lists[idx] = Some(node);
    }

    fn pop(&mut self, idx: usize) -> Option<NonNull<u8>> {
        let node = self.free_lists[idx]?;
        let next = unsafe { node.as_ptr().read().next };
        self.free_lists[idx] = next;
        Some(node.cast::<u8>())
    }

    /// Carve one fresh buddy page into class-`idx` objects and push them all.
    unsafe fn refill(&mut self, idx: usize) -> Option<()> {
        let pa = BUDDY.get()?.lock().alloc_page()?; // raw page, NOT zeroed
        let base = crate::layout::pa_to_linear_va(pa).as_usize();
        let size = CLASSES[idx];
        for k in 0..objects_per_page(idx) {
            // SAFETY: base is a mapped, page-aligned linear VA; base + k*size is
            // non-null and CLASSES[idx]-aligned
            let p = unsafe { NonNull::new_unchecked((base + k * size) as *mut u8) };
            unsafe { self.push(idx, p) };
        }
        Some(())
    }
}

/*
 *
 *  Formal verification (Kani model-checking harnesses)
 *
 *  Compiled only under `cargo kani`. Pure size-class arithmetic.
 *
 */

#[cfg(kani)]
mod verification {
    use super::*;

    /// The chosen class fits both size and alignment, and is the smallest that
    /// does — guards against handing out a class too small for the request.
    #[kani::proof]
    #[kani::unwind(11)]
    fn size_class_is_smallest_fit() {
        let size: usize = kani::any();
        let align_shift: u32 = kani::any();
        kani::assume(size >= 1 && size <= 4096);
        kani::assume(align_shift <= 12); // align = 2^shift, 1..=4096
        let align = 1usize << align_shift;

        let need = size.max(align);
        match size_class_index(size, align) {
            Some(i) => {
                assert!(CLASSES[i] >= size && CLASSES[i] >= align); // fits both
                if i > 0 {
                    assert!(CLASSES[i - 1] < need); // smallest that fits
                }
            }
            None => assert!(need > 2048), // only when larger than the top class
        }
    }

    /// Carving is sound: at least one object per page, and they never run past
    /// the page boundary.
    #[kani::proof]
    fn carving_stays_in_page() {
        let idx: usize = kani::any();
        kani::assume(idx < CLASSES.len());

        let n = objects_per_page(idx);
        assert!(n >= 1); // 2048 <= 4096 -> at least one
        assert!(n * CLASSES[idx] <= PAGE_SIZE); // stays within the page
    }

    /// The free-list is LIFO: the last object pushed is the first popped, and
    /// draining returns every object exactly once, then None.
    #[kani::proof]
    fn push_pop_is_lifo() {
        static mut BUF: [u8; 32] = [0; 32]; // four 8-byte slots
        let mut slab = Slab::new();

        let p0 = NonNull::new(unsafe { &raw mut BUF[0] }).unwrap();
        let p1 = NonNull::new(unsafe { &raw mut BUF[8] }).unwrap();

        unsafe {
            slab.push(0, p0);
            slab.push(0, p1);
        }

        assert!(slab.pop(0) == Some(p1)); // last in, first out
        assert!(slab.pop(0) == Some(p0));
        assert!(slab.pop(0) == None); // drained
    }
}
