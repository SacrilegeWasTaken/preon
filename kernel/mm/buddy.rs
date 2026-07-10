//! Buddy physical page allocator — the kernel's page-granular allocator.
//!
//! Free blocks are kept in per-order free-lists (orders `0..MAX_ORDER`, block
//! `= 2^order` frames). `alloc` splits the smallest sufficient block down;
//! `free` coalesces with its buddy up the orders; `free_range` seeds the lists
//! from a contiguous run. A 16-byte [`PageInfo`] per frame (the mem-map) holds
//! each frame's state and free-list links. The split/coalesce/carve core is
//! Kani-verified on a bounded model; see `docs/VERIFICATION.md`.

use core::sync::atomic::AtomicU32;

use kernel_arch::mm::PhysAddr;
use kernel_builtin::sync::{Once, SpinLock};

use crate::frame::PAGE_SIZE;

use self::pfn::Pfn;

const _: () = assert!(size_of::<PageInfo>() == 16);
const _: () = assert!(align_of::<PageInfo>() == 4);

mod pfn {
    #[repr(transparent)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub struct Pfn(u32);

    impl Pfn {
        pub(super) fn new_unchecked(raw: u32) -> Self {
            Self(raw)
        }
        pub(super) fn index(self) -> usize {
            self.0 as usize
        }
        pub(super) fn raw(self) -> u32 {
            self.0
        }
    }
}

const RAW_NONE: u32 = u32::MAX;

#[repr(C)]
struct PageInfo {
    next: u32,
    prev: u32,
    count: AtomicU32,
    order: u8,
    state: PageState,
    flags: u16,
}

impl PageInfo {
    fn next(&self) -> Option<Pfn> {
        debug_assert!(self.is_free());
        if self.next == RAW_NONE {
            return None;
        }
        Some(Pfn::new_unchecked(self.next))
    }

    fn prev(&self) -> Option<Pfn> {
        debug_assert!(self.is_free());
        if self.prev == RAW_NONE {
            return None;
        }
        Some(Pfn::new_unchecked(self.prev))
    }

    fn set_next(&mut self, link: Option<Pfn>) {
        match link {
            Some(pfn) => {
                debug_assert_ne!(pfn.raw(), RAW_NONE);
                self.next = pfn.raw()
            }
            None => self.next = RAW_NONE,
        }
    }

    fn set_prev(&mut self, link: Option<Pfn>) {
        match link {
            Some(pfn) => {
                debug_assert_ne!(pfn.raw(), RAW_NONE);
                self.prev = pfn.raw()
            }
            None => self.prev = RAW_NONE,
        }
    }

    fn order(&self) -> u8 {
        debug_assert!(self.is_head());
        self.order
    }
    fn set_order(&mut self, order: u8) {
        debug_assert!((order as usize) < MAX_ORDER);
        self.order = order
    }
    fn state(&self) -> PageState {
        self.state
    }
    fn set_state(&mut self, state: PageState) {
        self.state = state
    }

    fn count(&self) -> &AtomicU32 {
        &self.count
    }

    fn is_head(&self) -> bool {
        matches!(self.state, PageState::Free | PageState::Allocated)
    }
    fn is_free(&self) -> bool {
        self.state == PageState::Free
    }
    fn mark_free(&mut self, order: u8) {
        self.state = PageState::Free;
        self.order = order
    }
    fn mark_allocated(&mut self, order: u8) {
        self.state = PageState::Allocated;
        self.order = order;
        self.set_next(None);
        self.set_prev(None);
    }
    fn mark_tail(&mut self) {
        self.state = PageState::Tail;
        self.set_next(None);
        self.set_prev(None);
    }
    fn mark_reserved(&mut self) {
        self.state = PageState::Reserved;
        self.set_next(None);
        self.set_prev(None);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum PageState {
    Reserved,
    Free,
    Allocated,
    Tail,
}

const MAX_ORDER: usize = 11;

/// The global buddy allocator, installed once by `kmain` after hand-off.
/// `get()` is `None` until then — callers (e.g. `frame::alloc_page`) fall back
/// to the boot bump pool.
pub static BUDDY: Once<SpinLock<BuddyAllocator>> = Once::new();

/// A buddy allocator over a contiguous frame range `[dram_base, +nr_frames)`,
/// backed by a `pages` mem-map (one [`PageInfo`] per frame).
pub struct BuddyAllocator {
    pages: &'static mut [PageInfo],
    dram_base: PhysAddr,
    nr_frames: usize,
    free_area: [Option<Pfn>; MAX_ORDER],
    free_frames: usize,
}

impl BuddyAllocator {
    fn new(pages: &'static mut [PageInfo], dram_base: PhysAddr, nr_frames: usize) -> Self {
        debug_assert!(nr_frames <= pages.len());
        for p in pages.iter_mut() {
            p.mark_reserved();
        }
        Self {
            pages,
            dram_base,
            nr_frames,
            free_area: [None; MAX_ORDER],
            free_frames: 0,
        }
    }

    pub fn free_frames(&self) -> usize {
        self.free_frames
    }
    pub fn alloc_pages(&mut self, order: u8) -> Option<PhysAddr> {
        self.alloc(order).map(|pfn| self.pa_of(pfn))
    }

    pub fn alloc_page(&mut self) -> Option<PhysAddr> {
        self.alloc_pages(0)
    }

    pub fn free_pages(&mut self, pa: PhysAddr, order: u8) {
        let pfn = self.pfn_of(pa);
        self.free(pfn, order);
    }

    pub fn pfn_of(&self, pa: PhysAddr) -> Pfn {
        debug_assert!(
            self.dram_base.as_usize() <= pa.as_usize()
                && pa.as_usize() < (self.dram_base.as_usize() + self.nr_frames * PAGE_SIZE)
        );
        debug_assert!(pa.as_usize().is_multiple_of(PAGE_SIZE));
        Pfn::new_unchecked(((pa.as_usize() - self.dram_base.as_usize()) / PAGE_SIZE) as u32)
    }

    fn pa_of(&self, pfn: Pfn) -> PhysAddr {
        debug_assert!(pfn.index() < self.nr_frames);
        PhysAddr::new(self.dram_base.as_usize() + pfn.raw() as usize * PAGE_SIZE)
    }

    fn buddy_pfn(&self, pfn: Pfn, order: u8) -> Option<Pfn> {
        debug_assert!((order as usize) < MAX_ORDER);
        let n = pfn.raw() ^ (1 << order);
        if n as usize >= self.nr_frames {
            return None;
        }
        Some(Pfn::new_unchecked(n))
    }

    fn page(&self, pfn: Pfn) -> &PageInfo {
        &self.pages[pfn.index()]
    }

    fn page_mut(&mut self, pfn: Pfn) -> &mut PageInfo {
        &mut self.pages[pfn.index()]
    }

    fn push_front(&mut self, order: u8, pfn: Pfn) {
        debug_assert!(self.page(pfn).is_free());
        let old_head = self.free_area[order as usize];

        self.page_mut(pfn).set_prev(None);
        self.page_mut(pfn).set_next(old_head);

        if let Some(h) = old_head {
            self.page_mut(h).set_prev(Some(pfn));
        }

        self.free_area[order as usize] = Some(pfn);
    }

    fn unlink(&mut self, order: u8, pfn: Pfn) {
        let prev = self.page(pfn).prev();
        let next = self.page(pfn).next();

        // fix forward-link for prev
        match prev {
            Some(p) => self.page_mut(p).set_next(next),
            None => self.free_area[order as usize] = next,
        }

        if let Some(n) = next {
            self.page_mut(n).set_prev(prev);
        }
    }

    fn pop_front(&mut self, order: u8) -> Option<Pfn> {
        let head = self.free_area[order as usize]?;
        self.unlink(order, head);
        Some(head)
    }

    fn alloc(&mut self, order: u8) -> Option<Pfn> {
        let mut k = (order as usize..MAX_ORDER).find(|&k| self.free_area[k].is_some())?;
        let block = self.pop_front(k as u8).unwrap();
        while k > order as usize {
            k -= 1;
            let buddy = Pfn::new_unchecked(block.raw() + (1 << k) as u32);
            self.page_mut(buddy).mark_free(k as u8);
            self.push_front(k as u8, buddy);
        }
        self.free_frames -= 1 << order;
        self.page_mut(block).mark_allocated(order);
        Some(block)
    }

    fn free(&mut self, pfn: Pfn, order: u8) {
        self.free_frames += 1 << order;

        let mut cur = pfn;
        let mut cur_order = order;

        while (cur_order as usize) < MAX_ORDER - 1 {
            let buddy = match self.buddy_pfn(cur, cur_order) {
                Some(b) => b,
                None => break,
            };

            if !(self.page(buddy).is_free() && self.page(buddy).order() == cur_order) {
                break;
            }
            self.unlink(cur_order, buddy);

            let (head, tail) = if cur.raw() < buddy.raw() {
                (cur, buddy)
            } else {
                (buddy, cur)
            };
            self.page_mut(tail).mark_tail();
            cur = head;
            cur_order += 1;
        }
        self.page_mut(cur).mark_free(cur_order);
        self.push_front(cur_order, cur);
    }

    fn max_order_at(&self, pfn: Pfn, remaining: usize) -> u8 {
        let cap = (MAX_ORDER - 1) as u32;
        let align = pfn.raw().trailing_zeros().min(cap);

        let size = (usize::BITS - 1 - remaining.leading_zeros()).min(cap);
        align.min(size) as u8
    }

    pub fn free_range(&mut self, start: Pfn, count: usize) {
        debug_assert!((start.raw() as usize) + count <= self.nr_frames);
        let mut pfn = start.raw();
        let end = pfn + count as u32;

        while pfn < end {
            let order = self.max_order_at(Pfn::new_unchecked(pfn), (end - pfn) as usize);
            let mut i = 1u32;
            while i < (1 << order) {
                self.page_mut(Pfn::new_unchecked(pfn + i)).mark_tail();
                i += 1;
            }
            let head = Pfn::new_unchecked(pfn);
            self.page_mut(head).mark_free(order);
            self.push_front(order, head);
            self.free_frames += 1 << order;

            pfn += 1 << order;
        }
    }
    /// Frames needed for a mem-map of one PageInfo per frame.
    pub fn memmap_frames(nr_frames: usize) -> usize {
        (nr_frames * size_of::<PageInfo>()).div_ceil(PAGE_SIZE)
    }

    /// Build an allocator whose mem-map lives at `mm_pa` (physical, reachable via
    /// the linear map). Marks every frame Reserved — populate with `free_range`.
    ///
    /// # Safety
    /// `mm_pa` must point at `memmap_frames(nr_frames) * PAGE_SIZE` bytes of
    /// mapped, page-aligned RAM reserved for the mem-map (e.g. from
    /// `bootmem.alloc`). Nothing else may alias it.
    pub unsafe fn new_at(mm_pa: PhysAddr, dram_base: PhysAddr, nr_frames: usize) -> Self {
        let va = crate::layout::pa_to_linear_va(mm_pa);
        let pages =
            unsafe { core::slice::from_raw_parts_mut(va.as_usize() as *mut PageInfo, nr_frames) };
        Self::new(pages, dram_base, nr_frames)
    }
}

/*
 *
 *  Formal verification (Kani model-checking harnesses)
 *
 *  Compiled only under `cargo kani`. The address math (`buddy_pfn`, `pfn_of`,
 *  `pa_of`) never indexes `pages`, so those harnesses run over an empty slice;
 *  `push_front` does, so it gets a small fixed backing store.
 *
 */

#[cfg(kani)]
mod verification {
    use super::*;

    /// A buddy allocator with live address parameters but an empty `pages`
    /// slice. Sound for the math-only methods, which never touch `pages`.
    fn math_only(dram_base: usize, nr_frames: usize) -> BuddyAllocator {
        BuddyAllocator {
            pages: <&mut [PageInfo]>::default(),
            dram_base: PhysAddr::new(dram_base),
            nr_frames,
            free_area: [None; MAX_ORDER],
            free_frames: 0,
        }
    }

    /// `pfn_of` and `pa_of` are mutual inverses over every in-range frame.
    #[kani::proof]
    fn pfn_pa_round_trip() {
        let dram_base: usize = kani::any();
        let nr_frames: usize = kani::any();
        // Page-aligned base, and keep base + span clear of usize overflow
        // while staying fully symbolic within a realistic 2^47 PA space.
        kani::assume(dram_base % PAGE_SIZE == 0);
        kani::assume(dram_base <= (1usize << 47));
        kani::assume(nr_frames > 0 && nr_frames <= (1usize << 20));

        let a = math_only(dram_base, nr_frames);

        let idx: u32 = kani::any();
        kani::assume((idx as usize) < nr_frames);
        let pfn = Pfn::new_unchecked(idx);

        assert!(a.pfn_of(a.pa_of(pfn)) == pfn);
    }

    /// XOR by the order bit is an involution: the buddy of the buddy is the
    /// original frame, whenever both stay in range.
    #[kani::proof]
    fn buddy_is_involution() {
        let nr_frames: usize = kani::any();
        kani::assume(nr_frames > 0 && nr_frames <= (1usize << 20));
        let a = math_only(0, nr_frames);

        let idx: u32 = kani::any();
        kani::assume((idx as usize) < nr_frames);
        let order: u8 = kani::any();
        kani::assume((order as usize) < MAX_ORDER);

        let pfn = Pfn::new_unchecked(idx);
        if let Some(buddy) = a.buddy_pfn(pfn, order) {
            assert!(a.buddy_pfn(buddy, order) == Some(pfn));
        }
    }

    /// After pushing two distinct free frames onto the same order, the list is
    /// LIFO-linked: newest is head with `prev = None`, and the two nodes point
    /// at each other consistently.
    #[kani::proof]
    fn push_front_links_head() {
        static mut BACKING: [PageInfo; 4] = [const {
            PageInfo {
                next: RAW_NONE,
                prev: RAW_NONE,
                count: AtomicU32::new(0),
                order: 0,
                state: PageState::Free,
                flags: 0,
            }
        }; 4];
        let pages: &'static mut [PageInfo] = unsafe { &mut *(&raw mut BACKING) };

        let mut a = BuddyAllocator {
            pages,
            dram_base: PhysAddr::new(0),
            nr_frames: 4,
            free_area: [None; MAX_ORDER],
            free_frames: 0,
        };

        let order: u8 = kani::any();
        kani::assume((order as usize) < MAX_ORDER);

        let i1: u32 = kani::any();
        let i2: u32 = kani::any();
        kani::assume(i1 < 4 && i2 < 4 && i1 != i2);
        let p1 = Pfn::new_unchecked(i1);
        let p2 = Pfn::new_unchecked(i2);

        a.push_front(order, p1);
        a.push_front(order, p2);

        assert!(a.free_area[order as usize] == Some(p2));
        assert!(a.page(p2).prev() == None);
        assert!(a.page(p2).next() == Some(p1));
        assert!(a.page(p1).prev() == Some(p2));
    }

    // Verification-driven spec for the next two ops. Implement on
    // `impl BuddyAllocator` to satisfy the harnesses below:
    //   fn unlink(&mut self, order: u8, pfn: Pfn);
    //   fn pop_front(&mut self, order: u8) -> Option<Pfn>;
    // Until both exist, `cargo kani` won't compile this module. The kernel
    // build is unaffected — everything here is `#[cfg(kani)]`.

    /// Fresh allocator over a private 4-frame backing store, every frame
    /// pre-marked Free so the free-list ops can link it.
    macro_rules! fresh4 {
        () => {{
            static mut B: [PageInfo; 4] = [const {
                PageInfo {
                    next: RAW_NONE,
                    prev: RAW_NONE,
                    count: AtomicU32::new(0),
                    order: 0,
                    state: PageState::Free,
                    flags: 0,
                }
            }; 4];
            let pages: &'static mut [PageInfo] = unsafe { &mut *(&raw mut B) };
            let n = pages.len();
            BuddyAllocator {
                pages,
                dram_base: PhysAddr::new(0),
                nr_frames: n,
                free_area: [None; MAX_ORDER],
                free_frames: 0,
            }
        }};
    }

    /// One frame: `push_front` then `pop_front` returns it and empties the
    /// list; a second pop yields `None`.
    #[kani::proof]
    fn push_pop_round_trip() {
        let mut a = fresh4!();

        let o: u8 = kani::any();
        kani::assume((o as usize) < MAX_ORDER);
        let i: u32 = kani::any();
        kani::assume(i < 4);
        let p = Pfn::new_unchecked(i);

        a.push_front(o, p);
        assert!(a.pop_front(o) == Some(p));
        assert!(a.free_area[o as usize] == None);
        assert!(a.pop_front(o) == None);
    }

    /// Removing the head repoints `free_area` at the successor, whose `prev`
    /// becomes `None`.
    #[kani::proof]
    fn unlink_head() {
        let mut a = fresh4!();

        let o: u8 = kani::any();
        kani::assume((o as usize) < MAX_ORDER);
        let (i0, i1): (u32, u32) = (kani::any(), kani::any());
        kani::assume(i0 < 4 && i1 < 4 && i0 != i1);
        let (p0, p1) = (Pfn::new_unchecked(i0), Pfn::new_unchecked(i1));

        a.push_front(o, p0);
        a.push_front(o, p1); // head: p1 -> p0
        a.unlink(o, p1);

        assert!(a.free_area[o as usize] == Some(p0));
        assert!(a.page(p0).prev() == None);
        assert!(a.page(p0).next() == None);
    }

    /// Removing the tail leaves the head with `next == None`.
    #[kani::proof]
    fn unlink_tail() {
        let mut a = fresh4!();

        let o: u8 = kani::any();
        kani::assume((o as usize) < MAX_ORDER);
        let (i0, i1): (u32, u32) = (kani::any(), kani::any());
        kani::assume(i0 < 4 && i1 < 4 && i0 != i1);
        let (p0, p1) = (Pfn::new_unchecked(i0), Pfn::new_unchecked(i1));

        a.push_front(o, p0);
        a.push_front(o, p1); // head: p1 -> p0 (p0 is tail)
        a.unlink(o, p0);

        assert!(a.free_area[o as usize] == Some(p1));
        assert!(a.page(p1).next() == None);
        assert!(a.page(p1).prev() == None);
    }

    /// Removing an interior node splices its neighbours together and leaves
    /// the rest of the list intact.
    #[kani::proof]
    fn unlink_middle() {
        let mut a = fresh4!();

        let o: u8 = kani::any();
        kani::assume((o as usize) < MAX_ORDER);
        let (i0, i1, i2): (u32, u32, u32) = (kani::any(), kani::any(), kani::any());
        kani::assume(i0 < 4 && i1 < 4 && i2 < 4);
        kani::assume(i0 != i1 && i1 != i2 && i0 != i2);
        let (p0, p1, p2) = (
            Pfn::new_unchecked(i0),
            Pfn::new_unchecked(i1),
            Pfn::new_unchecked(i2),
        );

        a.push_front(o, p0);
        a.push_front(o, p1);
        a.push_front(o, p2); // head: p2 -> p1 -> p0
        a.unlink(o, p1);

        // list is now p2 -> p0
        assert!(a.free_area[o as usize] == Some(p2));
        assert!(a.page(p2).next() == Some(p0));
        assert!(a.page(p0).prev() == Some(p2));
        assert!(a.page(p2).prev() == None);
        assert!(a.page(p0).next() == None);
    }

    /*
     *
     *  alloc / free
     *
     */

    /// With every free-list empty, `alloc` reports OOM for any order and never
    /// hands back a frame.
    #[kani::proof]
    #[kani::unwind(12)]
    fn alloc_oom() {
        let mut a = fresh4!();

        let o: u8 = kani::any();
        kani::assume((o as usize) < MAX_ORDER);

        assert!(a.alloc(o) == None);
    }

    /// A single free order-2 block at frame 0 satisfies any request `o <= 2`:
    /// the base comes back Allocated at `o`, each split level deposits the
    /// upper buddy half onto `free_area[level]`, and `free_frames` drops by
    /// exactly `2^o` (mass conservation).
    #[kani::proof]
    #[kani::unwind(12)]
    fn alloc_splits_correctly() {
        let mut a = fresh4!();

        let f0 = Pfn::new_unchecked(0);
        let f1 = Pfn::new_unchecked(1);
        let f2 = Pfn::new_unchecked(2);

        a.page_mut(f1).mark_tail();
        a.page_mut(f2).mark_tail();
        a.page_mut(Pfn::new_unchecked(3)).mark_tail();
        a.page_mut(f0).mark_free(2);
        a.push_front(2, f0);
        a.free_frames = 4;

        let o: u8 = kani::any();
        kani::assume((o as usize) <= 2);

        assert!(a.alloc(o) == Some(f0));
        assert!(a.page(f0).state() == PageState::Allocated);
        assert!(a.page(f0).order() == o);

        match o {
            2 => {
                assert!(a.free_area[2] == None);
                assert!(a.free_area[1] == None);
                assert!(a.free_area[0] == None);
            }
            1 => {
                assert!(a.free_area[1] == Some(f2));
                assert!(a.free_area[0] == None);
                assert!(a.page(f2).state() == PageState::Free);
                assert!(a.page(f2).order() == 1);
            }
            _ => {
                assert!(a.free_area[1] == Some(f2));
                assert!(a.free_area[0] == Some(f1));
                assert!(a.page(f2).order() == 1);
                assert!(a.page(f1).order() == 0);
            }
        }

        assert!(a.free_frames == 4 - (1usize << o));
    }

    /// Freeing a block whose buddy is not free just drops it onto
    /// `free_area[order]` and bumps the free count — no coalescing.
    #[kani::proof]
    #[kani::unwind(12)]
    fn free_no_coalesce() {
        let mut a = fresh4!();
        let f0 = Pfn::new_unchecked(0);
        let f1 = Pfn::new_unchecked(1);

        a.page_mut(f1).mark_reserved(); // buddy of f0 at order 0 — not mergeable
        a.page_mut(f0).mark_allocated(0);
        a.free_frames = 0;

        a.free(f0, 0);

        assert!(a.free_area[0] == Some(f0));
        assert!(a.page(f0).state() == PageState::Free);
        assert!(a.page(f0).order() == 0);
        assert!(a.free_frames == 1);
    }

    /// When the freed block is the *upper* buddy, the merged block's head must
    /// be the *lower* PFN. Frees frame 1 next to a free frame 0.
    #[kani::proof]
    #[kani::unwind(12)]
    fn free_upper_buddy_coalesces() {
        let mut a = fresh4!();
        let f0 = Pfn::new_unchecked(0);
        let f1 = Pfn::new_unchecked(1);

        // Stop any further merge at order 1: frame 2 (buddy of the order-1
        // block at 0) must not be a free order-1 block.
        a.page_mut(Pfn::new_unchecked(2)).mark_reserved();
        a.page_mut(Pfn::new_unchecked(3)).mark_reserved();

        a.page_mut(f0).mark_free(0);
        a.push_front(0, f0); // free_area[0] = 0
        a.page_mut(f1).mark_allocated(0);
        a.free_frames = 1;

        a.free(f1, 0);

        assert!(a.free_area[1] == Some(f0)); // head is the lower PFN, not f1
        assert!(a.free_area[0] == None);
        assert!(a.page(f0).state() == PageState::Free);
        assert!(a.page(f0).order() == 1);
        assert!(a.page(f1).state() == PageState::Tail);
        assert!(a.free_frames == 2);
    }

    /// `free` is the exact inverse of `alloc`: allocating any `o <= 2` out of a
    /// single order-2 block and then freeing the result restores the original
    /// block, coalescing all the way back up. Covers 0, 1 and 2 merge levels.
    #[kani::proof]
    #[kani::unwind(12)]
    fn alloc_free_round_trip() {
        let mut a = fresh4!();
        let f0 = Pfn::new_unchecked(0);

        a.page_mut(Pfn::new_unchecked(1)).mark_tail();
        a.page_mut(Pfn::new_unchecked(2)).mark_tail();
        a.page_mut(Pfn::new_unchecked(3)).mark_tail();
        a.page_mut(f0).mark_free(2);
        a.push_front(2, f0);
        a.free_frames = 4;

        let o: u8 = kani::any();
        kani::assume((o as usize) <= 2);

        let block = a.alloc(o).unwrap();
        a.free(block, o);

        // back to the original lone order-2 block
        assert!(a.free_area[2] == Some(f0));
        assert!(a.free_area[1] == None);
        assert!(a.free_area[0] == None);
        assert!(a.page(f0).state() == PageState::Free);
        assert!(a.page(f0).order() == 2);
        assert!(a.free_frames == 4);
    }

    /*
     *
     *  free_range (init)
     *
     */

    /// Mark every frame of the 4-frame backing store as Reserved, the state
    /// `free_range` is expected to start from at init.
    fn reserve_all(a: &mut BuddyAllocator) {
        let mut i = 0u32;
        while i < 4 {
            a.page_mut(Pfn::new_unchecked(i)).mark_reserved();
            i += 1;
        }
    }

    /// A fully-aligned range that is itself a power of two collapses to a
    /// single maximal block: [0,4) becomes one order-2 block.
    #[kani::proof]
    #[kani::unwind(12)]
    fn free_range_full_is_one_block() {
        let mut a = fresh4!();
        reserve_all(&mut a);

        a.free_range(Pfn::new_unchecked(0), 4);

        assert!(a.free_area[2] == Some(Pfn::new_unchecked(0)));
        assert!(a.free_area[1] == None);
        assert!(a.free_area[0] == None);
        assert!(a.page(Pfn::new_unchecked(0)).state() == PageState::Free);
        assert!(a.page(Pfn::new_unchecked(0)).order() == 2);
        assert!(a.page(Pfn::new_unchecked(1)).state() == PageState::Tail);
        assert!(a.page(Pfn::new_unchecked(2)).state() == PageState::Tail);
        assert!(a.page(Pfn::new_unchecked(3)).state() == PageState::Tail);
        assert!(a.free_frames == 4);
    }

    /// An unaligned range is carved greedily by alignment then size: [1,4)
    /// splits into an order-0 block at 1 and an order-1 block at [2,4).
    #[kani::proof]
    #[kani::unwind(12)]
    fn free_range_unaligned_tail() {
        let mut a = fresh4!();
        reserve_all(&mut a);

        a.free_range(Pfn::new_unchecked(1), 3);

        assert!(a.free_area[0] == Some(Pfn::new_unchecked(1)));
        assert!(a.free_area[1] == Some(Pfn::new_unchecked(2)));
        assert!(a.free_area[2] == None);
        assert!(a.page(Pfn::new_unchecked(1)).order() == 0);
        assert!(a.page(Pfn::new_unchecked(2)).order() == 1);
        assert!(a.page(Pfn::new_unchecked(3)).state() == PageState::Tail);
        assert!(a.free_frames == 3);
    }

    /// Over every sub-range of the 4-frame space, `free_range` adds exactly
    /// `count` frames — no double-count, no dropped tail (mass conservation).
    #[kani::proof]
    #[kani::unwind(12)]
    fn free_range_conserves_mass() {
        let mut a = fresh4!();
        reserve_all(&mut a);

        let start: u32 = kani::any();
        let count: usize = kani::any();
        kani::assume(count >= 1 && count <= 4);
        kani::assume((start as usize) + count <= 4);

        a.free_range(Pfn::new_unchecked(start), count);

        assert!(a.free_frames == count);
    }

    /*
     *
     *  public API (PhysAddr) + constructor
     *
     */

    /// A freshly-built allocator owns every frame as Reserved, holds no free
    /// blocks, and reports OOM until `free_range` populates it.
    #[kani::proof]
    #[kani::unwind(12)]
    fn new_starts_reserved() {
        static mut B: [PageInfo; 4] = [const {
            PageInfo {
                next: RAW_NONE,
                prev: RAW_NONE,
                count: AtomicU32::new(0),
                order: 0,
                state: PageState::Free,
                flags: 0,
            }
        }; 4];
        let pages: &'static mut [PageInfo] = unsafe { &mut *(&raw mut B) };

        let mut a = BuddyAllocator::new(pages, PhysAddr::new(0x4000_0000), 4);

        let mut i = 0u32;
        while i < 4 {
            assert!(a.page(Pfn::new_unchecked(i)).state() == PageState::Reserved);
            i += 1;
        }
        assert!(a.free_frames == 0);
        assert!(a.alloc(0) == None);
    }

    /// The PhysAddr wrappers preserve the verified core: `alloc_pages` returns
    /// the block's page-aligned physical base, and `free_pages` of that address
    /// restores the original block through the `dram_base` offset.
    #[kani::proof]
    #[kani::unwind(12)]
    fn alloc_free_pages_round_trip() {
        let mut a = fresh4!();
        a.dram_base = PhysAddr::new(0x4000_0000);

        let f0 = Pfn::new_unchecked(0);
        a.page_mut(Pfn::new_unchecked(1)).mark_tail();
        a.page_mut(Pfn::new_unchecked(2)).mark_tail();
        a.page_mut(Pfn::new_unchecked(3)).mark_tail();
        a.page_mut(f0).mark_free(2);
        a.push_front(2, f0);
        a.free_frames = 4;

        let o: u8 = kani::any();
        kani::assume((o as usize) <= 2);

        let pa = a.alloc_pages(o).unwrap();
        assert!(pa == PhysAddr::new(0x4000_0000)); // block base via dram_base
        assert!(pa.as_usize() % 4096 == 0);

        a.free_pages(pa, o);

        assert!(a.free_area[2] == Some(f0));
        assert!(a.free_frames == 4);
    }

    /// The mem-map sizing holds exactly one `PageInfo` per frame: enough bytes
    /// for `n` descriptors, and no more than one page of slack.
    #[kani::proof]
    fn memmap_frames_is_enough() {
        let n: usize = kani::any();
        kani::assume(n <= (1 << 30)); // keep n * 16 well within usize

        let frames = BuddyAllocator::memmap_frames(n);

        assert!(frames * PAGE_SIZE >= n * size_of::<PageInfo>());
        assert!(n == 0 || (frames - 1) * PAGE_SIZE < n * size_of::<PageInfo>());
    }
}
