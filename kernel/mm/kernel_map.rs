//! Build the kernel-side linear map covering all physical RAM.
//!
//! Each PA from DTB maps to VA = PA + KERNEL_VA_OFFSET via 1 GiB blocks
//! (or 2 MiB tail blocks if 1 GiB alignment doesn't fit). The resulting
//! root is installed in TTBR1 by mmu::switch_ttbr1.

use fdt::Fdt;
use kernel_arch::VirtAddr;

use crate::attrs::MemoryAttr;
use crate::frame::{alloc_page, PhysAddr, PAGE_SIZE};

use crate::layout::image_va_to_pa;
use crate::page_table::{Access, Executable, LeafAttrs, Level, PageTable};
use crate::ram;
use crate::types::{FrameAllocator, Shareability};

unsafe extern "C" {
    static __text_start: u8;
    static __text_end: u8;
    static __data_start: u8;
    static __data_end: u8;
    static __rodata_start: u8;
    static __rodata_end: u8;
    static __bss_start: u8;
    static __bss_end: u8;
    static __stack_bottom: u8;
    static __stack_top: u8;
}

const TEXT_ATTRS: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadOnly,
    share: Shareability::InnerShareable,
    execute: Executable::Kernel,
};
const RODATA_ATTRS: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadOnly,
    share: Shareability::InnerShareable,
    execute: Executable::None,
};
const DATA_ATTRS: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadWrite,
    share: Shareability::InnerShareable,
    execute: Executable::None,
};
const BSS_ATTRS: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadWrite,
    share: Shareability::InnerShareable,
    execute: Executable::None,
};
const STACK_ATTRS: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadWrite,
    share: Shareability::InnerShareable,
    execute: Executable::None,
};

const BLOCK_1GB: usize = 1 << 30;
const BLOCK_2MB: usize = 2 << 20;

const LINEAR_RAM: LeafAttrs = LeafAttrs {
    memory: MemoryAttr::Normal,
    access: Access::KernelReadWrite,
    share: Shareability::InnerShareable,
    execute: Executable::None,
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

    let mut alloc = GlobalFrameAllocator;

    build_linear(root, fdt, &mut alloc);
    build_image(root, &mut alloc);

    root_pa
}

pub fn build_image(root: &mut PageTable, alloc: &mut impl FrameAllocator) {
    unsafe {
        map_section(root, &__text_start, &__text_end, TEXT_ATTRS, alloc);
        map_section(root, &__rodata_start, &__rodata_end, RODATA_ATTRS, alloc);
        map_section(root, &__data_start, &__data_end, DATA_ATTRS, alloc);
        map_section(root, &__bss_start, &__bss_end, BSS_ATTRS, alloc);
        map_section(root, &__stack_bottom, &__stack_top, STACK_ATTRS, alloc);
    }
}

pub fn build_linear(root: &mut PageTable, fdt: &Fdt, alloc: &mut impl FrameAllocator) {
    ram::for_each_region(fdt, |r| {
        map_region(root, r.base, r.size, LINEAR_RAM, alloc);
    })
}

fn map_section(
    root: &mut PageTable,
    start_sym: *const u8,
    end_sym: *const u8,
    attrs: LeafAttrs,
    alloc: &mut impl FrameAllocator,
) {
    let va_start = start_sym as usize;
    let va_end = end_sym as usize;

    let mut va = va_start;

    while va < va_end {
        let pa = image_va_to_pa(VirtAddr::new(va));
        root.map(VirtAddr::new(va), pa, Level::L3, attrs, alloc);
        va += PAGE_SIZE;
    }
}

/// Map a contiguous PA range PA..PA+size into the upper-half kernel
/// VA at `PA + KERNEL_VA_OFFSET`, preferring 1 GiB blocks then 2 MiB.
fn map_region(
    root: &mut PageTable,
    base: PhysAddr,
    size: usize,
    attrs: LeafAttrs,
    alloc: &mut impl FrameAllocator,
) {
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

        root.map(va, PhysAddr::new(pa), level, attrs, alloc);
        pa += block_size;
    }
}
