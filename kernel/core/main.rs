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
use kernel_builtin::{kernel_uart_direct_log, kernel_uart_log, sync::SpinLock};
use kernel_exceptions::ExceptionVectors;
use kernel_mm::{
    bootmem::BootMem,
    buddy::{BUDDY, BuddyAllocator},
    frame::PAGE_SIZE,
    layout::{PHYS_LOAD_BASE, linear_va_to_pa},
};

// Boot stub: drop EL2 -> EL1, enable FP/SIMD, set the kernel stack,
// zero .bss, and jump into `kmain` with the DTB pointer in x0.
global_asm!(include_str!("asm/boot.s"));

unsafe extern "C" {
    static __stack_top_pa: u8;
    static __boot_bss_start: u8;
    static __boot_bss_end: u8;
}

#[unsafe(no_mangle)]
pub extern "C" fn kmain(dtb: usize) -> ! {
    ExceptionVectors::install();
    kernel_uart_log!("Hello from ExOS!");

    kernel_arch::percpu::init();

    let fdt = parse_fdt(dtb);
    let root = kernel_mm::kernel_map::build(&fdt);
    kernel_uart_log!("kernel_map built, root at 0x{:x}", root.as_usize());

    unsafe { kernel_mm::mmu::switch_ttbr1(root) };
    kernel_uart_log!("TTBR1 switched");

    unsafe { kernel_mm::mmu::disable_ttbr0() };
    kernel_uart_log!("TTBR0 disabled");

    let boot = build_bootmem(&fdt, dtb);
    let buddy = init_buddy(boot);
    kernel_uart_log!("buddy up: {} free frames", buddy.free_frames());
    BUDDY.call_once(|| SpinLock::new(buddy));

    let psci = kernel_cpu::psci::Psci::from_fdt(&fdt).expect("no /psci node");
    kernel_cpu::Smp::bring_up_all(root, &fdt, &psci).expect("SMP bring-up failed");

    unsafe { reclaim_trampoline() };

    let p1 = kernel_mm::frame::alloc_page();
    let p2 = kernel_mm::frame::alloc_page();
    kernel_uart_direct_log!("buddy alloc: {:#x}, {:#x}", p1.as_usize(), p2.as_usize());

    let v = alloc::vec![1u32, 2, 3, 4];
    kernel_uart_direct_log!("vec len={} @ {:p}", v.len(), v.as_ptr());
    let b = alloc::boxed::Box::new(0xDEADBEEFu64);
    kernel_uart_direct_log!("box {:#x} @ {:p}", *b, core::ptr::addr_of!(*b));

    kernel_uart_direct_log!("Triggering exception...");
    unsafe { core::arch::asm!("udf #0") }

    wfe_loop!();
}

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

fn init_buddy(mut boot: BootMem) -> BuddyAllocator {
    let (base_pfn, nr_frames) = boot.ram_span();
    let dram_base = PhysAddr::new(base_pfn as usize * PAGE_SIZE);

    let mm_frames = BuddyAllocator::memmap_frames(nr_frames);
    let mm_pfn = boot
        .alloc(mm_frames as u32, 1)
        .expect("no room for mem-map");
    let mm_pa = PhysAddr::new(mm_pfn as usize * PAGE_SIZE);

    let mut buddy = unsafe { BuddyAllocator::new_at(mm_pa, dram_base, nr_frames) };

    boot.for_each_free(|h| {
        let pfn = buddy.pfn_of(PhysAddr::new(h.base as usize * PAGE_SIZE)); // абс → relative
        buddy.free_range(pfn, h.frames as usize);
    });

    buddy
}

fn parse_fdt(dtb: usize) -> fdt::Fdt<'static> {
    unsafe { fdt::Fdt::from_ptr(dtb as *const u8) }.expect("DTB error. Check QEMU conf.")
}

/// # Safety
/// Caller guarantees to call it obly once and only right after buddy allocator was created
unsafe fn reclaim_trampoline() {
    let mut buddy = unsafe { BUDDY.get().unwrap_unchecked().lock() };
    let bb_start = &raw const __boot_bss_start as usize;
    let bb_end = &raw const __boot_bss_end as usize;
    let bb_pages = (bb_end - bb_start) / PAGE_SIZE;

    let a = buddy.pfn_of(PhysAddr::new(bb_start));
    buddy.free_range(a, bb_pages);
    kernel_uart_direct_log!("reclaimed {} trampoline pages", bb_pages);
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel_exceptions::panic::panic_dump(info);
    wfe_loop!();
}
