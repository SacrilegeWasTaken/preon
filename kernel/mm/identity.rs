//! Identity mapping (VA == PA) for early MMU bring-up.
//!
//! The flip of `SCTLR_EL1.M` only stops being a foot-gun if the
//! instruction *after* `msr` translates to the same physical address it
//! occupied before. The simplest way to guarantee that is an identity
//! map — every region we still need (kernel image, stacks, page tables,
//! UART) gets `VA = PA` until we later relocate the kernel to the upper
//! half through TTBR1.
//!
//! What we map:
//!   - RAM `0x4000_0000` .. `0x47FF_FFFF` (128 MiB), 2 MiB blocks at L2.
//!     This covers the kernel image, BSS, stacks, the bootstrap frame
//!     pool, and the page tables themselves. Attributes treat the whole
//!     range as RW + kernel-executable; refining `.text` to RO+X and
//!     data to RW+NX is left for a follow-up once the linker exposes
//!     section boundaries.
//!   - PL011 UART, a single 4 KiB page at `0x0900_0000` mapped as
//!     Device-nGnRE so register accesses keep their ordering after MMU
//!     enable.

use crate::attrs::MemoryAttr;
use crate::frame::{alloc_page, PhysAddr, VirtAddr};
use crate::page_table::{Access, Executable, LeafAttrs, Level, PageTable, Shareability};

//
// Region constants
//

/// Physical base of the QEMU `virt` RAM region.
const RAM_BASE: usize = 0x4000_0000;

/// Size of the RAM region we identity-map. 128 MiB matches the `-m 128M`
/// QEMU command-line.
const RAM_SIZE: usize = 128 * 1024 * 1024;

/// PL011 UART base address.
const UART_BASE: usize = 0x0900_0000;

/// 2 MiB — block size we use at level 2 when covering RAM.
const BLOCK_2MB: usize = 2 * 1024 * 1024;

//
// Attribute presets
//

/// Normal kernel RAM. Read/write at EL1, executable at EL1 (refined
/// later once `.text` / `.data` boundaries are wired through), inner
/// shareable so all CPUs see the same view.
const KERNEL_RAM: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadWrite,
    share: Shareability::InnerShareable,
    execute: Executable::Kernel,
};

/// PL011 register window. Device-nGnRE preserves ordering and disables
/// gathering / caching, and `Executable::None` blocks instruction fetch
/// from MMIO.
const DEVICE_UART: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::DeviceNGNRE,
    access: Access::KernelReadWrite,
    share: Shareability::NonShareable,
    execute: Executable::None,
};

//
// Builder
//

/// Allocate a fresh root table, install every identity mapping the
/// kernel needs to survive `SCTLR.M = 1`, and return the physical
/// address of the root table for installation into `TTBR0_EL1`.
pub fn build() -> PhysAddr {
    let root_pa = alloc_page();
    // Safety: `alloc_page` returns a 4 KiB-aligned, zero-initialised
    // frame that no other reference observes during bring-up.
    let root = unsafe { PageTable::from_phys(root_pa) };

    map_ram(root);
    map_uart(root);

    root_pa
}

/// Identity-map the QEMU virt RAM region in 2 MiB blocks at level 2.
fn map_ram(root: &mut PageTable) {
    let blocks = RAM_SIZE / BLOCK_2MB;
    for i in 0..blocks {
        let pa = RAM_BASE + i * BLOCK_2MB;
        root.map(VirtAddr::new(pa), PhysAddr::new(pa), Level::L2, KERNEL_RAM);
    }
}

/// Identity-map the PL011 UART as a single 4 KiB device page at level 3.
fn map_uart(root: &mut PageTable) {
    root.map(
        VirtAddr::new(UART_BASE),
        PhysAddr::new(UART_BASE),
        Level::L3,
        DEVICE_UART,
    );
}
