//! Kernel entry point and early-boot orchestration.
//!
//! [`kmain`] runs after the assembly boot stub (`asm/boot.s`) has dropped to
//! EL1 and set up a stack. It parses the device tree, builds the page tables
//! and switches to the upper half, brings the physical allocators online,
//! starts the secondary CPUs, and reclaims the boot trampoline — then parks.
#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

extern crate alloc;
use fdt::Fdt;
use kernel_arch::{
    mm::{PhysAddr, VirtAddr},
    wfe_loop,
};
use kernel_builtin::{kernel_log, sync::SpinLock};
use kernel_exceptions::ExceptionVectors;
use kernel_mm::{
    bootmem::BootMem,
    buddy::{BUDDY, BuddyAllocator},
    frame::PAGE_SIZE,
    layout::{PHYS_LOAD_BASE, linear_va_to_pa},
    types::FrameCount,
};

// Boot stub: drop EL2 -> EL1, enable FP/SIMD, set the kernel stack,
// zero .bss, and jump into `kmain` with the DTB pointer in x0.
global_asm!(include_str!("asm/boot.s"));

unsafe extern "C" {
    static __stack_top_pa: u8;
    static __boot_bss_start: u8;
    static __boot_bss_end: u8;
}

/*
 *
 *  Entry
 *
 */

#[unsafe(no_mangle)]
pub extern "C" fn kmain(dtb: usize) -> ! {
    ExceptionVectors::install();
    kernel_log!("Hello from Preon kernel!");

    kernel_arch::percpu::init();

    let fdt = parse_fdt(dtb);
    let root = kernel_mm::kernel_map::build(&fdt);
    kernel_log!("kernel_map built, root at 0x{:x}", root.as_usize());

    unsafe { kernel_mm::mmu::switch_ttbr1(root) };
    kernel_log!("TTBR1 switched");

    unsafe { kernel_mm::mmu::disable_ttbr0() };
    kernel_log!("TTBR0 disabled");

    let boot = build_bootmem(&fdt, dtb);
    let buddy = init_buddy(boot);
    kernel_log!("buddy up: {} free frames", buddy.free_frames().raw());
    BUDDY.call_once(|| SpinLock::new(buddy));

    let psci = kernel_cpu::psci::Psci::from_fdt(&fdt).expect("no /psci node");
    kernel_cpu::Smp::bring_up_all(root, &fdt, &psci).expect("SMP bring-up failed");

    unsafe { reclaim_trampoline() };

    let p1 = kernel_mm::frame::alloc_page();
    let p2 = kernel_mm::frame::alloc_page();
    kernel_log!("buddy alloc: {:#x}, {:#x}", p1.as_usize(), p2.as_usize());

    let v = alloc::vec![1u32, 2, 3, 4];
    kernel_log!("vec len={} @ {:p}", v.len(), v.as_ptr());
    let b = alloc::boxed::Box::new(0xDEADBEEFu64);
    kernel_log!("box {:#x} @ {:p}", *b, core::ptr::addr_of!(*b));

    kernel_log!("Triggering exception...");
    unsafe { core::arch::asm!("udf #0") }

    wfe_loop!();
}

/*
 *
 *  Boot-time setup
 *
 */

/// Build the boot-time memblock allocator from the FDT: register every RAM
/// region, then reserve the kernel image and the DTB so they survive hand-off.
fn build_bootmem(fdt: &Fdt, dtb: usize) -> BootMem {
    let mut boot = BootMem::new();

    kernel_mm::ram::for_each_region(fdt, |r| {
        let base = r.base.as_usize();
        boot.add_memory_bytes(base, base + r.size);
    });

    let img_end = &raw const __stack_top_pa as usize;
    boot.reserve_bytes(PHYS_LOAD_BASE, img_end);

    let fdt_pa = linear_va_to_pa(VirtAddr::new(dtb)).as_usize();
    boot.reserve_bytes(fdt_pa, fdt_pa + fdt.total_size());

    boot
}

/// Hand memory from bootmem to a fresh buddy allocator: size and place the
/// mem-map over the RAM span, then free every non-reserved frame into it.
fn init_buddy(mut boot: BootMem) -> BuddyAllocator {
    let (base_pfn, nr_frames) = boot.ram_span();
    let dram_base = PhysAddr::new(base_pfn.index() * PAGE_SIZE);

    let mm_frames = BuddyAllocator::memmap_frames(nr_frames);
    let mm_pfn = boot.alloc(mm_frames, 1).expect("no room for mem-map");
    let mm_pa = PhysAddr::new(mm_pfn.index() * PAGE_SIZE);

    let mut buddy = unsafe { BuddyAllocator::new_at(mm_pa, dram_base, nr_frames) };

    boot.for_each_free(|h| {
        let pfn = buddy.pfn_of(PhysAddr::new(h.base.index() * PAGE_SIZE)); // absolute → relative
        buddy.free_range(pfn, h.frames);
    });

    buddy
}

/// Parse the flattened device tree at `dtb`; panics if the blob is malformed.
fn parse_fdt(dtb: usize) -> fdt::Fdt<'static> {
    unsafe { fdt::Fdt::from_ptr(dtb as *const u8) }.expect("DTB error. Check QEMU conf.")
}

/// # Safety
/// Caller guarantees to call it only once, right after the buddy allocator is created.
unsafe fn reclaim_trampoline() {
    let mut buddy = unsafe { BUDDY.get().unwrap_unchecked().lock() };
    let bb_start = &raw const __boot_bss_start as usize;
    let bb_end = &raw const __boot_bss_end as usize;
    let bb_pages = (bb_end - bb_start) / PAGE_SIZE;

    let a = buddy.pfn_of(PhysAddr::new(bb_start));
    buddy.free_range(a, FrameCount::new(bb_pages));
    kernel_log!("reclaimed {} trampoline pages", bb_pages);
}

/*
 *
 *  Panic
 *
 */

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel_exceptions::panic::panic_dump(info);
    wfe_loop!();
}
