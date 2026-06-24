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
