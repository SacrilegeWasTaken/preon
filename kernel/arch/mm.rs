/// Physical address. A thin wrapper around `usize` to keep physical and
/// virtual addresses from being mixed up at call sites.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PhysAddr(usize);

impl PhysAddr {
    pub const fn new(raw: usize) -> Self {
        Self(raw)
    }

    pub const fn as_usize(self) -> usize {
        self.0
    }

    pub const fn as_u64(self) -> u64 {
        self.0 as u64
    }
}

impl core::fmt::LowerHex for PhysAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::LowerHex::fmt(&self.0, f)
    }
}

impl core::fmt::UpperHex for PhysAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::UpperHex::fmt(&self.0, f)
    }
}

/// Virtual address. Paired with [`PhysAddr`]; the newtype lets functions
/// declare which kind of address they want and rejects the other at
/// compile time.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VirtAddr(usize);

impl VirtAddr {
    pub const fn new(raw: usize) -> Self {
        Self(raw)
    }

    pub const fn as_usize(self) -> usize {
        self.0
    }

    pub const fn as_u64(self) -> u64 {
        self.0 as u64
    }
}

impl core::fmt::LowerHex for VirtAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::LowerHex::fmt(&self.0, f)
    }
}

impl core::fmt::UpperHex for VirtAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::UpperHex::fmt(&self.0, f)
    }
}

/// Where a translation table sits in the 4-level hierarchy.
///
/// Each level consumes 9 bits of the virtual address (`index_shift`
/// returns the bit position) and either points at the next level via a
/// table entry or terminates the walk with a block (L1, L2) or page (L3).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Level {
    L0,
    L1,
    L2,
    L3,
}

impl Level {
    /// Bit position of this level's 9-bit index inside a virtual address.
    pub const fn index_shift(self) -> u64 {
        match self {
            Level::L0 => 39,
            Level::L1 => 30,
            Level::L2 => 21,
            Level::L3 => 12,
        }
    }

    /// Whether block entries are encodable at this level. L1 = 1 GiB
    /// blocks, L2 = 2 MiB blocks; L0 and L3 don't support blocks.
    pub const fn supports_block(self) -> bool {
        matches!(self, Level::L1 | Level::L2)
    }

    /// Extract this level's 9-bit index from a virtual address.
    ///
    /// Each level consumes a contiguous 9-bit slice of the VA — L0 takes
    /// the top, L3 the bottom. The result is in `0..512` and is meant
    /// for indexing a [`PageTable::entries`].
    pub const fn index_in(self, va: VirtAddr) -> usize {
        ((va.as_u64() >> self.index_shift()) & 0x1FF) as usize
    }

    /// Next deeper level. Panics on L3 — there is no level below.
    pub const fn next_level(self) -> Level {
        match self {
            Level::L0 => Level::L1,
            Level::L1 => Level::L2,
            Level::L2 => Level::L3,
            Level::L3 => panic!("Level::L3 has no next level"),
        }
    }

    /// Build a level from a 2-bit index (e.g. the level field of a fault
    /// status code). Masked to `0..=3`, so it is total — the `_` arm is
    /// unreachable and exists only to satisfy the `u8` match.
    pub const fn from_index(n: u8) -> Level {
        match n & 0b11 {
            0 => Level::L0,
            1 => Level::L1,
            2 => Level::L2,
            3 => Level::L3,
            _ => panic!("Bad level index"),
        }
    }
}

/*
 *
 *  Formal verification (Kani model-checking harnesses)
 *
 *  Compiled only under `cargo kani` (the `kani` cfg); a normal kernel build
 *  never sees this module. Every harness proves pure address/index
 *  arithmetic — no asm, no MMIO — so CBMC can reason about all paths.
 *
 */

#[cfg(kani)]
mod verification {
    use super::{Level, VirtAddr};

    /// The four levels' 9-bit indices tile VA bits [47:12] exactly — no gap,
    /// no overlap. Catches any off-by-one in `index_shift`.
    #[kani::proof]
    fn level_indices_tile_va() {
        let raw: u64 = kani::any();
        let va = VirtAddr::new(raw as usize);

        let l0 = Level::L0.index_in(va) as u64;
        let l1 = Level::L1.index_in(va) as u64;
        let l2 = Level::L2.index_in(va) as u64;
        let l3 = Level::L3.index_in(va) as u64;

        // Each slice is 9 bits wide.
        assert!(l0 < 512 && l1 < 512 && l2 < 512 && l3 < 512);

        // Reassembled, the four slices equal bits [47:12] of the VA.
        let reassembled = (l0 << 27) | (l1 << 18) | (l2 << 9) | l3;
        let expected = (raw >> 12) & 0xF_FFFF_FFFF; // low 36 bits
        assert!(reassembled == expected);
    }

    /// `from_index` is total over every `u8`: never panics, and decodes the
    /// low two bits to the matching level.
    #[kani::proof]
    fn from_index_is_total() {
        let n: u8 = kani::any();
        let level = Level::from_index(n);
        let expected = match n & 0b11 {
            0 => Level::L0,
            1 => Level::L1,
            2 => Level::L2,
            _ => Level::L3,
        };
        assert!(level == expected);
    }

    /// `next_level` walks L0 → L1 → L2 → L3 without panicking.
    #[kani::proof]
    fn next_level_progresses() {
        assert!(Level::L0.next_level() == Level::L1);
        assert!(Level::L1.next_level() == Level::L2);
        assert!(Level::L2.next_level() == Level::L3);
    }

    /// L3 has no level below it: `next_level` must panic rather than hand back
    /// a bogus level.
    #[kani::proof]
    #[kani::should_panic]
    fn next_level_l3_panics() {
        let _ = Level::L3.next_level();
    }
}
