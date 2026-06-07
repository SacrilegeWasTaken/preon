//! 4-level page-table types for 4 KiB granule, 48-bit virtual addresses.
//!
//! Each table holds 512 entries; an entry is a 64-bit word with a
//! hardware-defined bit layout. Bits 1:0 pick the entry kind:
//!
//! ```text
//!   00  invalid
//!   01  block  (1 GiB at L1, 2 MiB at L2; not valid at L0 or L3)
//!   11  table  (at L0..L2)  /  page  (at L3 only)
//! ```
//!
//! Public API uses typed parameters ([`Level`], [`Access`],
//! [`Shareability`], [`Executable`], [`LeafAttrs`]) so call sites can
//! never accidentally cross-wire bit fields. The constants below name
//! the raw bits and are kept `pub` for inspection / debug printing —
//! constructors compose them through the typed enums.
//!
//! Reference: ARM Architecture Reference Manual, D5.3 (translation
//! table descriptor formats).

use crate::attrs::MemoryAttr;
use crate::frame::{PhysAddr, VirtAddr};

// Type bits (bits 0:1)

/// Bit 0 — the entry is in use. A zero here means MMU treats the entry
/// as invalid regardless of the other bits.
pub const VALID_BIT: u64 = 1 << 0;

/// Bit 1 — distinguishes a table/page descriptor from a block.
pub const TABLE_BIT: u64 = 1 << 1;

/// Zero word: every bit clear, entry is invalid.
pub const TYPE_INVALID: u64 = 0;

/// `0b01` — valid block entry. Only meaningful at L1 (1 GiB) and L2
/// (2 MiB); at L0 and L3 the hardware treats this encoding as reserved.
pub const TYPE_BLOCK: u64 = VALID_BIT;

/// `0b11` — valid table descriptor (at L0..L2) or 4 KiB page (at L3).
pub const TYPE_TABLE_OR_PAGE: u64 = VALID_BIT | TABLE_BIT;

// AttrIndx (bits 2:4)

/// Position of the 3-bit `AttrIndx` field. The value at the field is a
/// slot index into `MAIR_EL1`; see [`crate::attrs::MemoryAttr`].
pub const ATTR_INDX_SHIFT: u64 = 2;

// Access permissions (bits 6:7)

pub const AP_SHIFT: u64 = 6;

/// Read/write at EL1, no EL0 access. Default for kernel data.
pub const AP_RW_EL1: u64 = 0b00 << AP_SHIFT;

/// Read/write at both EL0 and EL1. For shared user/kernel mappings.
pub const AP_RW_EL0_EL1: u64 = 0b01 << AP_SHIFT;

/// Read-only at EL1, no EL0. For kernel `.text` and `.rodata`.
pub const AP_RO_EL1: u64 = 0b10 << AP_SHIFT;

/// Read-only at both EL0 and EL1.
pub const AP_RO_EL0_EL1: u64 = 0b11 << AP_SHIFT;

// Shareability (bits 8:9)

pub const SH_SHIFT: u64 = 8;

pub const SH_NON_SHAREABLE: u64 = 0b00 << SH_SHIFT;
pub const SH_OUTER_SHAREABLE: u64 = 0b10 << SH_SHIFT;

/// Default for kernel mappings on SMP — all CPUs in the cluster snoop
/// each other for this region.
pub const SH_INNER_SHAREABLE: u64 = 0b11 << SH_SHIFT;

// Access flag (bit 10)

/// Access flag. Must be 1, otherwise the first access takes an
/// `Access Fault`. We don't track page usage, so we set it unconditionally.
pub const AF: u64 = 1 << 10;

// Output address (bits 12:47)

/// Mask isolating the physical-address bits of an entry. Bits 11:0 are
/// the page offset (always zero, page-aligned), and bits 63:48 hold
/// upper attributes — neither belongs in the address.
pub const OUTPUT_ADDR_MASK: u64 = 0x0000_FFFF_FFFF_F000;

// Execute-never bits (53, 54)

/// Privileged eXecute Never — EL1 cannot fetch instructions from this
/// region. Set on data mappings to harden against control-flow hijack.
pub const PXN: u64 = 1 << 53;

/// Unprivileged eXecute Never — EL0 cannot fetch from here. Set on
/// every kernel mapping so userspace can never run kernel code paths.
pub const UXN: u64 = 1 << 54;

// Typed parameters

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
}

/// Access permissions for a leaf entry (block or page). Names describe
/// the policy ("who can do what") rather than encoding raw AP bits.
#[derive(Debug, Copy, Clone)]
pub enum Access {
    /// EL1 read/write, EL0 has no access at all. Default for kernel data.
    KernelReadWrite,
    /// EL1 read-only, EL0 has no access. For `.text` and `.rodata`.
    KernelReadOnly,
    /// Both EL0 and EL1 read/write. For shared user / kernel regions.
    SharedReadWrite,
    /// Both EL0 and EL1 read-only.
    SharedReadOnly,
}

impl Access {
    /// Pre-shifted AP field ready to OR into an entry.
    pub const fn bits(self) -> u64 {
        match self {
            Access::KernelReadWrite => AP_RW_EL1,
            Access::KernelReadOnly => AP_RO_EL1,
            Access::SharedReadWrite => AP_RW_EL0_EL1,
            Access::SharedReadOnly => AP_RO_EL0_EL1,
        }
    }
}

/// Cache-coherence domain of the mapped region.
#[derive(Debug, Copy, Clone)]
pub enum Shareability {
    NonShareable,
    OuterShareable,
    /// All CPUs in the inner-shareable domain snoop each other. The
    /// right choice for kernel mappings on SMP.
    InnerShareable,
}

impl Shareability {
    pub const fn bits(self) -> u64 {
        match self {
            Shareability::NonShareable => SH_NON_SHAREABLE,
            Shareability::OuterShareable => SH_OUTER_SHAREABLE,
            Shareability::InnerShareable => SH_INNER_SHAREABLE,
        }
    }
}

/// Who, if anyone, may fetch instructions from the mapped region.
///
/// Encoded with the PXN / UXN bits; each variant sets the bits that
/// *block* execution at the wrong privilege level.
#[derive(Debug, Copy, Clone)]
pub enum Executable {
    /// EL1 may execute, EL0 may not. `.text` of the kernel image.
    Kernel,
    /// EL0 may execute, EL1 may not. Userspace `.text` once we have it.
    User,
    /// Neither EL may execute. All data, MMIO, stacks.
    None,
}

impl Executable {
    pub const fn bits(self) -> u64 {
        match self {
            Executable::Kernel => UXN,
            Executable::User => PXN,
            Executable::None => PXN | UXN,
        }
    }
}

/// Bundle of attributes applied to a block or page entry. Built once
/// at the call site and passed to [`Entry::block`] / [`Entry::page`].
#[derive(Debug, Copy, Clone)]
pub struct LeafAttrs {
    pub memory: MemoryAttr,
    pub access: Access,
    pub share: Shareability,
    pub execute: Executable,
}

impl LeafAttrs {
    /// Compose every typed field into one pre-shifted bit pattern. AF
    /// is always set — we don't track page usage.
    const fn bits(self) -> u64 {
        (self.memory.index() << ATTR_INDX_SHIFT)
            | self.access.bits()
            | self.share.bits()
            | self.execute.bits()
            | AF
    }
}

// Entry type

/// One page-table entry.
///
/// Layout is hardware-defined; the constants above name every bit field
/// the kernel touches during bring-up. `#[repr(transparent)]` keeps this
/// bit-for-bit identical to a raw `u64`, which both the MMU walking the
/// table and our `[Entry; 512]` array layout depend on.
#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct Entry(u64);

impl Entry {
    /// Build an invalid (zeroed) entry. MMU follows nothing here.
    pub const fn invalid() -> Self {
        Self(TYPE_INVALID)
    }

    /// Build a table descriptor pointing at the next-level table.
    ///
    /// Use at L0..L2 — at L3 the same bit pattern means a 4 KiB page.
    /// The `next` address is masked to its valid output bits before
    /// being installed.
    pub const fn table(next: PhysAddr) -> Self {
        Self((next.as_u64() & OUTPUT_ADDR_MASK) | TYPE_TABLE_OR_PAGE)
    }

    /// Build a block entry mapping `pa` at `level` with `attrs`.
    ///
    /// Asserts that the level actually supports blocks (L1 = 1 GiB,
    /// L2 = 2 MiB).
    pub const fn block(pa: PhysAddr, level: Level, attrs: LeafAttrs) -> Self {
        assert!(level.supports_block(), "block entry only valid at L1 or L2");
        Self((pa.as_u64() & OUTPUT_ADDR_MASK) | attrs.bits() | TYPE_BLOCK)
    }

    /// Build a 4 KiB page entry at L3 mapping `pa` with `attrs`.
    pub const fn page(pa: PhysAddr, attrs: LeafAttrs) -> Self {
        Self((pa.as_u64() & OUTPUT_ADDR_MASK) | attrs.bits() | TYPE_TABLE_OR_PAGE)
    }

    /// Raw 64-bit value, useful for logging and bit inspection.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Bit 0 — entry is valid, MMU will follow it.
    pub const fn is_valid(self) -> bool {
        self.0 & VALID_BIT != 0
    }

    /// `0b11` — table descriptor at L0..L2, or 4 KiB page at L3. The
    /// distinction depends on the level being walked.
    pub const fn is_table_or_page(self) -> bool {
        self.0 & TYPE_TABLE_OR_PAGE == TYPE_TABLE_OR_PAGE
    }

    /// `0b01` — valid block entry. Only meaningful at L1 and L2.
    pub const fn is_block(self) -> bool {
        self.is_valid() && (self.0 & TABLE_BIT == 0)
    }

    /// Physical address contained in the output-address field. Valid
    /// for any non-invalid entry. The interpretation (next-level table
    /// vs block / page base) depends on the level and entry kind.
    pub const fn output_addr(self) -> PhysAddr {
        PhysAddr::new((self.0 & OUTPUT_ADDR_MASK) as usize)
    }
}

// Table type

/// 4 KiB page-table holding 512 entries.
///
/// Aligned to 4 KiB so it can serve as a translation table directly —
/// the MMU expects the table base to look like `..PA..0000_0000_0000`,
/// and the bootstrap allocator hands out frames at exactly that alignment.
#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [Entry; 512],
}

impl PageTable {
    /// Reborrow a freshly-allocated physical frame as an empty page table.
    ///
    /// The returned reference is `'static` because frames handed out by
    /// the bootstrap pool live for the entire kernel lifetime.
    ///
    /// # Safety
    /// `pa` must point at a 4 KiB-aligned, zero-initialised frame
    /// reserved for use as a page table. The caller commits to not
    /// aliasing the same frame through any other mutable reference.
    pub unsafe fn from_phys(pa: PhysAddr) -> &'static mut PageTable {
        unsafe { &mut *(pa.as_usize() as *mut PageTable) }
    }

    /// Read the entry indexed by `va` at `level`.
    pub fn entry_at(&self, va: VirtAddr, level: Level) -> Entry {
        self.entries[level.index_in(va)]
    }

    /// Mutable handle to the entry indexed by `va` at `level`. Used by
    /// the walker to install or refine descriptors.
    pub fn entry_at_mut(&mut self, va: VirtAddr, level: Level) -> &mut Entry {
        &mut self.entries[level.index_in(va)]
    }

    /// Install a leaf mapping for `va -> pa` at `target`, allocating
    /// intermediate tables as needed.
    ///
    /// `target` must be L1, L2 (block entry, 1 GiB / 2 MiB) or L3 (page
    /// entry, 4 KiB). The walker assumes any existing intermediate
    /// entries are themselves tables — splitting an existing block to
    /// refine a sub-range is not supported during bring-up.
    pub fn map(&mut self, va: VirtAddr, pa: PhysAddr, target: Level, attrs: LeafAttrs) {
        assert!(target != Level::L0, "leaf entries cannot live at L0");

        let leaf = if matches!(target, Level::L3) {
            Entry::page(pa, attrs)
        } else {
            Entry::block(pa, target, attrs)
        };

        let mut current: &mut PageTable = self;
        let mut level = Level::L0;

        while level != target {
            let entry = current.entry_at_mut(va, level);
            if !entry.is_valid() {
                // Fresh frame to host the next-level table.
                *entry = Entry::table(crate::frame::alloc_page());
            } else {
                assert!(
                    entry.is_table_or_page(),
                    "cannot descend through an existing block entry"
                );
            }
            let next_pa = entry.output_addr();
            // Safety: the address was either just allocated by us (zeroed
            // 4 KiB-aligned frame) or originally installed as a table by
            // an earlier `map` call — both satisfy `from_phys`.
            current = unsafe { PageTable::from_phys(next_pa) };
            level = level.next_level();
        }

        *current.entry_at_mut(va, target) = leaf;
    }
}
