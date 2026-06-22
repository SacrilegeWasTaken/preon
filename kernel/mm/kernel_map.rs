//! Build the kernel-side linear map covering all physical RAM.
//!
//! Each PA from DTB maps to VA = PA + KERNEL_VA_OFFSET via 1 GiB blocks
//! (or 2 MiB tail blocks if 1 GiB alignment doesn't fit). The resulting
//! root is installed in TTBR1 by mmu::switch_ttbr1.

use fdt::Fdt;
use kernel_arch::VirtAddr;

use crate::attrs::MemoryAttr;
use crate::frame::{alloc_page, PhysAddr};

use crate::layout::KERNEL_IMAGE_BASE;
use crate::page_table::{Access, Executable, LeafAttrs, Level, PageTable};
use crate::ram;
use crate::types::Shareability;

const BLOCK_1GB: usize = 1 << 30;
const BLOCK_2MB: usize = 2 << 20;

const IMAGE_BLOCK_VA: usize = KERNEL_IMAGE_BASE; // 0xFFFF_FFFF_8000_0000
const IMAGE_BLOCK_PA: usize = 0x4000_0000; // 1 GiB-aligned base
const LINEAR_RAM: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadWrite,
    share: Shareability::InnerShareable,
    execute: Executable::None,
}; // NX
   //
const IMAGE_RAM: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadWrite,
    share: Shareability::InnerShareable,
    execute: Executable::Kernel,
};

struct GlobalFrameAllocator;
impl crate::types::FrameAllocator for GlobalFrameAllocator {
    fn alloc_page(&mut self) -> PhysAddr {
        crate::frame::alloc_page()
    }
}

/// Allocate a fresh root table and build the kernel linear map.
/// Returns PA of root for TTBR1 installation.
pub fn build(fdt: &Fdt) -> PhysAddr {
    let root_pa = alloc_page();
    let root = unsafe { PageTable::from_phys(root_pa) };

    build_linear(root, fdt);
    build_image(root);

    root_pa
}

pub fn build_image(root: &mut PageTable) {
    let mut alloc = GlobalFrameAllocator;
    root.map(
        VirtAddr::new(IMAGE_BLOCK_VA),
        PhysAddr::new(IMAGE_BLOCK_PA),
        Level::L1,
        IMAGE_RAM,
        &mut alloc,
    );
}

pub fn build_linear(root: &mut PageTable, fdt: &Fdt) {
    ram::for_each_region(fdt, |r| {
        map_region(root, r.base, r.size, LINEAR_RAM);
    })
}

/// Map a contiguous PA range PA..PA+size into the upper-half kernel
/// VA at `PA + KERNEL_VA_OFFSET`, preferring 1 GiB blocks then 2 MiB.
fn map_region(root: &mut PageTable, base: PhysAddr, size: usize, attrs: LeafAttrs) {
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

        let va = crate::layout::pa_to_linear_va(PhysAddr::new(pa));
        let mut alloc = GlobalFrameAllocator;
        root.map(va, PhysAddr::new(pa), level, attrs, &mut alloc);
        pa += block_size;
    }
}
