//! Build the kernel-side linear map covering all physical RAM.
//!
//! Each PA from DTB maps to VA = PA + KERNEL_VA_OFFSET via 1 GiB blocks
//! (or 2 MiB tail blocks if 1 GiB alignment doesn't fit). The resulting
//! root is installed in TTBR1 by mmu::switch_ttbr1.

use fdt::Fdt;

use crate::attrs::MemoryAttr;
use crate::frame::{alloc_page, PhysAddr, VirtAddr};
use crate::layout::pa_to_kernel_va;
use crate::page_table::{Access, Executable, LeafAttrs, Level, PageTable};
use crate::ram;
use crate::types::Shareability;

const BLOCK_1GB: usize = 1 << 30;
const BLOCK_2MB: usize = 2 << 20;

struct GlobalFrameAllocator;
impl crate::types::FrameAllocator for GlobalFrameAllocator {
    fn alloc_page(&mut self) -> PhysAddr {
        crate::frame::alloc_page()
    }
}

/// Normal-cacheable, kernel RW, EL1-executable, Inner Shareable.
/// Same profile we use in the bring-up asm block.
const KERNEL_RAM: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadWrite,
    share: Shareability::InnerShareable,
    execute: Executable::Kernel,
};

/// Allocate a fresh root table and build the kernel linear map.
/// Returns PA of root for TTBR1 installation.
pub fn build(fdt: &Fdt) -> PhysAddr {
    let root_pa = alloc_page();
    let root = unsafe { PageTable::from_phys(root_pa) };

    ram::for_each_region(fdt, |region| {
        map_region(root, region.base, region.size);
    });

    root_pa
}

/// Map a contiguous PA range PA..PA+size into the upper-half kernel
/// VA at `PA + KERNEL_VA_OFFSET`, preferring 1 GiB blocks then 2 MiB.
fn map_region(root: &mut PageTable, base: PhysAddr, size: usize) {
    let mut pa = base.as_usize();
    let end = pa + size;

    while pa < end {
        let remaining = end - pa;

        let (block_size, level) = if pa.is_multiple_of(BLOCK_1GB) && remaining >= BLOCK_1GB {
            (BLOCK_1GB, Level::L1)
        } else if pa.is_multiple_of(BLOCK_2MB) && remaining >= BLOCK_2MB {
            (BLOCK_2MB, Level::L2)
        } else {
            panic!(
                "kernel_map: unaligned region tail (pa=0x{:x}, remaining=0x{:x})",
                pa, remaining
            );
        };

        let va = pa_to_kernel_va(PhysAddr::new(pa));
        let mut alloc = GlobalFrameAllocator;
        root.map(va, PhysAddr::new(pa), level, KERNEL_RAM, &mut alloc);
        pa += block_size;
    }
}
