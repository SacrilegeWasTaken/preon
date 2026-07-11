//! Boot-time physical memory allocator (memblock-style).
//!
//! Two fixed-capacity interval lists — `memory` (RAM from the FDT) and
//! `reserved` (kernel image, DTB, early allocations). [`BootMem::alloc`] carves
//! a top-down aligned block from the free gaps; [`BootMem::for_each_free`]
//! yields every RAM frame not reserved, for hand-off to the buddy allocator.
//! Retired once the buddy is up. The interval algebra — overlap, hole carve,
//! alloc safety, byte rounding, and the RAM span — is Kani-verified.

use crate::frame::PAGE_SIZE;
use crate::types::{FrameCount, Pfn};

/// A frame interval `[base, base + frames)` in absolute PFNs (`pa / PAGE_SIZE`).
#[derive(Clone, Copy)]
pub struct Region {
    pub base: Pfn,
    pub frames: FrameCount,
}

impl Region {
    const ZERO: Self = Self {
        base: Pfn::new(0),
        frames: FrameCount::new(0),
    };
    fn end(&self) -> Pfn {
        Pfn::new((self.base.index() + self.frames.raw()) as u32)
    }

    #[allow(dead_code)] // exercised only by Kani harnesses
    fn overlaps(&self, o: Region) -> bool {
        self.base < o.end() && o.base < self.end()
    }
}

const NR_REGIONS: usize = 128;

/*
 *
 *  Interval allocator
 *
 */

/// Fixed-capacity `memory` / `reserved` interval lists. `reserved` is kept
/// sorted by base so [`BootMem::for_each_free`]'s carve is a single pass.
pub struct BootMem {
    memory: [Region; NR_REGIONS],
    n_memory: usize,
    reserved: [Region; NR_REGIONS],
    n_reserved: usize,
}

impl BootMem {
    #[allow(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            memory: [Region::ZERO; NR_REGIONS],
            n_memory: 0,
            reserved: [Region::ZERO; NR_REGIONS],
            n_reserved: 0,
        }
    }

    /// Span from the lowest RAM frame to the highest: (base pfn, frame count).
    /// The buddy mem-map covers this whole range; inter-region gaps stay Reserved.
    pub fn ram_span(&self) -> (Pfn, FrameCount) {
        assert!(self.n_memory > 0, "bootmem: no memory regions");
        let mut lo = u32::MAX;
        let mut hi = 0u32;
        for m in &self.memory[..self.n_memory] {
            lo = lo.min(m.base.raw());
            hi = hi.max(m.end().raw());
        }
        (Pfn::new(lo), FrameCount::new((hi - lo) as usize))
    }

    fn add_memory(&mut self, base: Pfn, frames: FrameCount) {
        debug_assert!(frames.raw() >= 1);
        assert!(
            self.n_memory < NR_REGIONS,
            "bootmem: memory regions exhausted"
        );
        self.memory[self.n_memory] = Region { base, frames };
        self.n_memory += 1;
    }

    fn reserve(&mut self, base: Pfn, frames: FrameCount) {
        debug_assert!(frames.raw() >= 1);
        assert!(
            self.n_reserved < NR_REGIONS,
            "bootmem: reserved regions exhausted"
        );
        let mut i = self.n_reserved;
        while i > 0 && self.reserved[i - 1].base > base {
            self.reserved[i] = self.reserved[i - 1];
            i -= 1;
        }
        self.reserved[i] = Region { base, frames };
        self.n_reserved += 1;
    }

    pub fn reserve_bytes(&mut self, base_pa: usize, end_pa: usize) {
        let base = base_pa / PAGE_SIZE;
        let end = end_pa.div_ceil(PAGE_SIZE);
        self.reserve(Pfn::new(base as u32), FrameCount::new(end - base));
    }

    pub fn add_memory_bytes(&mut self, base_pa: usize, end_pa: usize) {
        let base = base_pa.div_ceil(PAGE_SIZE);
        let end = end_pa / PAGE_SIZE;
        if end > base {
            self.add_memory(Pfn::new(base as u32), FrameCount::new(end - base));
        }
    }

    pub fn for_each_free(&self, mut f: impl FnMut(Region)) {
        for m in &self.memory[..self.n_memory] {
            let mut cursor = m.base;
            for r in &self.reserved[..self.n_reserved] {
                if r.end() <= cursor || r.base >= m.end() {
                    continue;
                }
                if r.base > cursor {
                    f(Region {
                        base: cursor,
                        frames: FrameCount::new(r.base.index() - cursor.index()),
                    });
                }
                cursor = cursor.max(r.end());
            }
            if cursor < m.end() {
                f(Region {
                    base: cursor,
                    frames: FrameCount::new(m.end().index() - cursor.index()),
                });
            }
        }
    }

    pub fn alloc(&mut self, frames: FrameCount, align: u32) -> Option<Pfn> {
        debug_assert!(frames.raw() >= 1 && align >= 1);

        let mut best: Option<Pfn> = None;
        self.for_each_free(|hole| {
            if hole.frames >= frames {
                // highest possible base in this hole
                let top = Pfn::new((hole.end().index() - frames.raw()) as u32);
                // round DOWN to alignment
                let cand = Pfn::new(top.raw() - (top.raw() % align));
                if cand >= hole.base && best.map_or(true, |b| cand > b) {
                    best = Some(cand); // keep the highest candidate
                }
            }
        });

        let base = best?;
        self.reserve(base, frames);
        Some(base)
    }
}

/*
 *
 *  Formal verification (Kani model-checking harnesses)
 *
 *  Compiled only under `cargo kani`. Pure interval algebra — CBMC covers it
 *  exhaustively over small region sets.
 *
 *  Note: `for_each_free` must iterate `memory[..n_memory]` /
 *  `reserved[..n_reserved]`, never the full 128-slot arrays, or these unwind
 *  bounds blow up. It also assumes `reserved` is sorted by `base` — the
 *  invariant `reserve` maintains.
 *
 */

#[cfg(kani)]
mod verification {
    use super::*;

    /// A non-empty region whose `end()` does not wrap the u32 PFN space — the
    /// real invariants: regions carry ≥ 1 frame and none straddles the top of
    /// physical memory. (Empty regions make `overlaps` and the intersection
    /// definition disagree, so `reserve`/`add_memory` must never insert them.)
    fn any_region() -> Region {
        let base: u32 = kani::any();
        let frames: u32 = kani::any();
        kani::assume(frames >= 1);
        kani::assume((base as u64) + (frames as u64) <= u32::MAX as u64);
        Region {
            base: Pfn::new(base),
            frames: FrameCount::new(frames as usize),
        }
    }

    fn empty_bootmem() -> BootMem {
        BootMem {
            memory: [Region {
            base: Pfn::new(0),
            frames: FrameCount::new(0),
        }; NR_REGIONS],
            n_memory: 0,
            reserved: [Region {
            base: Pfn::new(0),
            frames: FrameCount::new(0),
        }; NR_REGIONS],
            n_reserved: 0,
        }
    }

    /// Overlap is a symmetric relation.
    #[kani::proof]
    fn overlaps_is_symmetric() {
        let a = any_region();
        let b = any_region();
        assert!(a.overlaps(b) == b.overlaps(a));
    }

    /// Two regions overlap iff their intersection is non-empty, i.e.
    /// `max(bases) < min(ends)`.
    #[kani::proof]
    fn overlaps_matches_definition() {
        let a = any_region();
        let b = any_region();
        let lo = a.base.max(b.base);
        let hi = a.end().min(b.end());
        assert!(a.overlaps(b) == (lo < hi));
    }

    /// One reserved hole in the middle of RAM leaves exactly the two flanking
    /// gaps: `[0,8) − [2,4)` = `[0,2)` and `[4,8)`.
    #[kani::proof]
    #[kani::unwind(12)]
    fn for_each_free_two_holes() {
        let mut bm = empty_bootmem();
        bm.memory[0] = Region {
            base: Pfn::new(0),
            frames: FrameCount::new(8),
        };
        bm.n_memory = 1;
        bm.reserved[0] = Region {
            base: Pfn::new(2),
            frames: FrameCount::new(2),
        }; // [2,4)
        bm.n_reserved = 1;

        let mut holes = [Region {
            base: Pfn::new(0),
            frames: FrameCount::new(0),
        }; 4];
        let mut n = 0usize;
        bm.for_each_free(|r| {
            holes[n] = r;
            n += 1;
        });

        assert!(n == 2);
        assert!(holes[0].base == Pfn::new(0) && holes[0].frames == FrameCount::new(2));
        assert!(holes[1].base == Pfn::new(4) && holes[1].frames == FrameCount::new(4));
    }

    /// Two reserved ranges split RAM into three gaps and exercise the
    /// cursor-advance between successive reservations.
    #[kani::proof]
    #[kani::unwind(12)]
    fn for_each_free_two_reserved() {
        let mut bm = empty_bootmem();
        bm.memory[0] = Region {
            base: Pfn::new(0),
            frames: FrameCount::new(8),
        };
        bm.n_memory = 1;
        bm.reserved[0] = Region {
            base: Pfn::new(1),
            frames: FrameCount::new(1),
        }; // [1,2)
        bm.reserved[1] = Region {
            base: Pfn::new(4),
            frames: FrameCount::new(1),
        }; // [4,5)
        bm.n_reserved = 2;

        let mut holes = [Region {
            base: Pfn::new(0),
            frames: FrameCount::new(0),
        }; 4];
        let mut n = 0usize;
        bm.for_each_free(|r| {
            holes[n] = r;
            n += 1;
        });

        assert!(n == 3);
        assert!(holes[0].base == Pfn::new(0) && holes[0].frames == FrameCount::new(1)); // [0,1)
        assert!(holes[1].base == Pfn::new(2) && holes[1].frames == FrameCount::new(2)); // [2,4)
        assert!(holes[2].base == Pfn::new(5) && holes[2].frames == FrameCount::new(3)); // [5,8)
    }

    /// Mass conservation over one RAM region and one symbolic reserved range:
    /// the freed frames equal RAM minus the reserved∩RAM overlap.
    #[kani::proof]
    #[kani::unwind(12)]
    fn for_each_free_conserves_mass() {
        let m_frames: u32 = kani::any();
        kani::assume(m_frames >= 1 && m_frames <= 8);
        let r_base: u32 = kani::any();
        let r_frames: u32 = kani::any();
        kani::assume(r_base <= 8 && r_frames <= 8);

        let mut bm = empty_bootmem();
        bm.memory[0] = Region {
            base: Pfn::new(0),
            frames: FrameCount::new(m_frames as usize),
        }; // [0, m_frames)
        bm.n_memory = 1;
        bm.reserved[0] = Region {
            base: Pfn::new(r_base),
            frames: FrameCount::new(r_frames as usize),
        };
        bm.n_reserved = 1;

        let mut freed = 0u32;
        bm.for_each_free(|r| {
            freed += r.frames.raw() as u32;
        });

        // reserved ∩ [0, m_frames)
        let hi = (r_base + r_frames).min(m_frames);
        let overlap = if hi > r_base { hi - r_base } else { 0 };
        assert!(freed == m_frames - overlap);
    }

    /// Inserting a region into a sorted `reserved` list keeps it sorted by
    /// `base` and grows the count by one.
    #[kani::proof]
    #[kani::unwind(6)]
    fn reserve_keeps_sorted() {
        let mut bm = empty_bootmem();
        bm.reserved[0] = Region {
            base: Pfn::new(2),
            frames: FrameCount::new(1),
        };
        bm.reserved[1] = Region {
            base: Pfn::new(5),
            frames: FrameCount::new(1),
        };
        bm.n_reserved = 2;

        let base: u32 = kani::any();
        kani::assume(base <= 8);
        bm.reserve(Pfn::new(base), FrameCount::new(1));

        assert!(bm.n_reserved == 3);
        let mut i = 1usize;
        while i < bm.n_reserved {
            assert!(bm.reserved[i - 1].base <= bm.reserved[i].base);
            i += 1;
        }
    }

    /// `add_memory` appends regions in call order.
    #[kani::proof]
    fn add_memory_appends() {
        let mut bm = empty_bootmem();
        bm.add_memory(Pfn::new(0), FrameCount::new(4));
        bm.add_memory(Pfn::new(10), FrameCount::new(4));

        assert!(bm.n_memory == 2);
        assert!(bm.memory[0].base == Pfn::new(0) && bm.memory[0].frames == FrameCount::new(4));
        assert!(bm.memory[1].base == Pfn::new(10) && bm.memory[1].frames == FrameCount::new(4));
    }

    /// `reserve_bytes` rounds OUTWARD: the frame range fully covers the byte
    /// range `[base_pa, end_pa)`.
    #[kani::proof]
    #[kani::unwind(4)]
    fn reserve_bytes_rounds_outward() {
        let base_pa: usize = kani::any();
        let end_pa: usize = kani::any();
        kani::assume(base_pa < end_pa);
        kani::assume(end_pa <= 64 * crate::frame::PAGE_SIZE);

        let mut bm = empty_bootmem();
        bm.reserve_bytes(base_pa, end_pa);

        assert!(bm.n_reserved == 1);
        let r = bm.reserved[0];
        assert!(r.base.index() * crate::frame::PAGE_SIZE <= base_pa);
        assert!(r.end().index() * crate::frame::PAGE_SIZE >= end_pa);
    }

    /// `add_memory_bytes` rounds INWARD: any region it adds lies fully inside
    /// the byte range `[base_pa, end_pa)` (and it may add nothing).
    #[kani::proof]
    fn add_memory_bytes_rounds_inward() {
        let base_pa: usize = kani::any();
        let end_pa: usize = kani::any();
        kani::assume(base_pa < end_pa);
        kani::assume(end_pa <= 64 * crate::frame::PAGE_SIZE);

        let mut bm = empty_bootmem();
        bm.add_memory_bytes(base_pa, end_pa);

        if bm.n_memory == 1 {
            let r = bm.memory[0];
            assert!(r.base.index() * crate::frame::PAGE_SIZE >= base_pa);
            assert!(r.end().index() * crate::frame::PAGE_SIZE <= end_pa);
        }
    }

    /// Top-down placement takes the highest fitting aligned slot: over RAM
    /// `[0,8)` with `[2,4)` reserved, `alloc(2, 2)` lands at `[6,8)` and
    /// reserves it.
    #[kani::proof]
    #[kani::unwind(12)]
    fn alloc_top_down_picks_highest() {
        let mut bm = empty_bootmem();
        bm.memory[0] = Region {
            base: Pfn::new(0),
            frames: FrameCount::new(8),
        };
        bm.n_memory = 1;
        bm.reserved[0] = Region {
            base: Pfn::new(2),
            frames: FrameCount::new(2),
        }; // [2,4)
        bm.n_reserved = 1;

        let base = bm.alloc(FrameCount::new(2), 2).unwrap();
        assert!(base == Pfn::new(6));
        assert!(bm.n_reserved == 2); // the block is now reserved
    }

    /// Whatever `alloc` returns is aligned, inside RAM, and never overlaps a
    /// reserved range — the core safety property.
    #[kani::proof]
    #[kani::unwind(12)]
    fn alloc_never_hits_reserved() {
        let mut bm = empty_bootmem();
        bm.memory[0] = Region {
            base: Pfn::new(0),
            frames: FrameCount::new(8),
        };
        bm.n_memory = 1;
        bm.reserved[0] = Region {
            base: Pfn::new(2),
            frames: FrameCount::new(2),
        }; // [2,4)
        bm.n_reserved = 1;

        let frames: u32 = kani::any();
        kani::assume(frames >= 1 && frames <= 8);
        let align: u32 = kani::any();
        kani::assume(align == 1 || align == 2 || align == 4);

        if let Some(base) = bm.alloc(FrameCount::new(frames as usize), align) {
            assert!(base.raw() % align == 0); // aligned
            assert!(base.raw() + frames <= 8); // inside RAM [0,8)
            assert!(base.raw() + frames <= 2 || base.raw() >= 4); // disjoint from [2,4)
        }
    }

    /// A request larger than the biggest free gap fails and reserves nothing.
    #[kani::proof]
    #[kani::unwind(12)]
    fn alloc_returns_none_when_too_big() {
        let mut bm = empty_bootmem();
        bm.memory[0] = Region {
            base: Pfn::new(0),
            frames: FrameCount::new(8),
        };
        bm.n_memory = 1;
        bm.reserved[0] = Region {
            base: Pfn::new(2),
            frames: FrameCount::new(2),
        }; // [2,4)
        bm.n_reserved = 1;

        // free gaps are [0,2) and [4,8); the largest is 4 frames.
        assert!(bm.alloc(FrameCount::new(5), 1) == None);
        assert!(bm.n_reserved == 1);
    }

    /// `ram_span` covers every RAM region: its base is at/below every region
    /// base, and its end at/above every region end.
    #[kani::proof]
    #[kani::unwind(4)]
    fn ram_span_covers_all_regions() {
        let mut bm = empty_bootmem();

        let b0: u32 = kani::any();
        let f0: u32 = kani::any();
        let b1: u32 = kani::any();
        let f1: u32 = kani::any();
        kani::assume(f0 >= 1 && f1 >= 1);
        kani::assume(b0 <= (1 << 20) && f0 <= (1 << 20));
        kani::assume(b1 <= (1 << 20) && f1 <= (1 << 20));

        bm.memory[0] = Region {
            base: Pfn::new(b0),
            frames: FrameCount::new(f0 as usize),
        };
        bm.memory[1] = Region {
            base: Pfn::new(b1),
            frames: FrameCount::new(f1 as usize),
        };
        bm.n_memory = 2;

        let (base, nr) = bm.ram_span();

        assert!(base.raw() <= b0 && base.raw() <= b1); // base at/below every region base
        assert!(base.index() + nr.raw() >= (b0 + f0) as usize); // end at/above ends
        assert!(base.index() + nr.raw() >= (b1 + f1) as usize);
    }
}
