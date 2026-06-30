use core::sync::atomic::AtomicU32;

use kernel_arch::mm::PhysAddr;

use crate::frame::PAGE_SIZE;

use self::pfn::Pfn;

const _: () = assert!(size_of::<PageInfo>() == 16);
const _: () = assert!(align_of::<PageInfo>() == 4);

mod pfn {
    #[repr(transparent)]
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub(super) struct Pfn(u32);

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

struct BuddyAllocator {
    pages: &'static mut [PageInfo],
    dram_base: PhysAddr,
    nr_frames: usize,
    free_area: [Option<Pfn>; MAX_ORDER],
    free_frames: usize,
}

impl BuddyAllocator {
    fn pfn_of(&self, pa: PhysAddr) -> Pfn {
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
}
