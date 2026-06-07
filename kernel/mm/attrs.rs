use kernel_arch::{flush_cpu_pipeline, write_sysreg};

/// Memory attribute slot selector used inside page-table entries.
///
/// Each variant matches one slot in `MAIR_EL1` configured by `install`.
/// Page-table builders accept this type instead of a raw `u64` so the
/// chosen attribute always corresponds to a slot that's actually programmed.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryAttr {
    /// Index 0: Normal memory, Inner+Outer write-back, write-allocate.
    /// Use for the kernel image, stacks, page tables, generic RAM.
    Normal = 0,

    /// Index 1: Normal memory, non-cachable. Use for DMA buffers one
    /// devices that bypass coherency arrive.
    NormalNonCacheable = 1,

    /// Index 2: Device-nGnRnE - non-Gathering, non-Reordering, no Early
    /// write acknowledgement. The strictest device type, for registers
    /// where any reorder or gather would be wrong (e.g. GIC).
    DeviceNGNRNE = 2,

    /// Index 3: Device-nGnRE - like the above but with early ack. Safe
    /// for most MMIO that doesn't need strict completion (UART, timers).
    DeviceNGNRE = 3,
}

pub const MAIR_VALUE: u64 = mair_slot(MemoryAttr::Normal, 0xFF)
    | mair_slot(MemoryAttr::NormalNonCacheable, 0x44)
    | mair_slot(MemoryAttr::DeviceNGNRNE, 0x00)
    | mair_slot(MemoryAttr::DeviceNGNRE, 0x04);

const fn mair_slot(attr: MemoryAttr, encoding: u64) -> u64 {
    encoding << (attr.index() * 8)
}

impl MemoryAttr {
    /// Returns the raw MAIR slot index for use in PTE AttrIndx field.
    pub const fn index(self) -> u64 {
        self as u64
    }
}

/// Program `MAIR_EL1` with the kernel's chosen memory types.
///
/// Must run before any `TTBR*_EL1` write - page-table entries reference
/// MAIR slots by index, and stale or zero MAIR would give random
/// attributes to every translation.
#[inline]
pub fn install() {
    write_sysreg!(mair_el1, MAIR_VALUE);
    flush_cpu_pipeline!();
}
