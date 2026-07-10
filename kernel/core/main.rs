#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

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
}

#[unsafe(no_mangle)]
pub extern "C" fn kmain(dtb: usize) -> ! {
    ExceptionVectors::install();

    kernel_uart_log!("Hello from ExOS!");

    let fdt = unsafe { fdt::Fdt::from_ptr(dtb as *const u8) }.expect("DTB error. Check QEMU conf.");
    kernel_mm::ram::for_each_region(&fdt, |region| {
        kernel_uart_log!(
            "RAM region: base=0x{:x} size=0x{:x}",
            region.base.as_usize(),
            region.size
        );
    });

    let new_root = kernel_mm::kernel_map::build(&fdt);
    kernel_uart_log!("kernel_map built, root at 0x{:x}", new_root.as_usize());

    unsafe { kernel_mm::mmu::switch_ttbr1(new_root) };
    kernel_uart_log!("TTBR1 switched");

    unsafe {
        kernel_mm::mmu::disable_ttbr0();
    }

    kernel_uart_log!("TTBR0 disabled");
    let mut boot = BootMem::new();

    kernel_mm::ram::for_each_region(&fdt, |r| {
        let base = r.base.as_usize();
        boot.add_memory_bytes(base, base + r.size);
    });

    let img_end = &raw const __stack_top_pa as usize;
    boot.reserve_bytes(PHYS_LOAD_BASE, img_end);

    let fdt_pa = linear_va_to_pa(VirtAddr::new(dtb)).as_usize();
    boot.reserve_bytes(fdt_pa, fdt_pa + fdt.total_size());

    boot.for_each_free(|h| {
        kernel_uart_log!("free: pfn 0x{:x}..0x{:x}", h.base, h.base + h.frames);
    });

    let dram_base = PhysAddr::new(0x4000_0000);
    let nr_frames = 0x80000;

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

    kernel_uart_log!("buddy up: {} free frames", buddy.free_frames());

    BUDDY.call_once(|| SpinLock::new(buddy));

    let p1 = kernel_mm::frame::alloc_page();
    let p2 = kernel_mm::frame::alloc_page();

    kernel_uart_direct_log!("buddy alloc: {:#x}, {:#x}", p1.as_usize(), p2.as_usize());

    kernel_uart_direct_log!("Triggering exception...");
    unsafe { core::arch::asm!("udf #0") }

    wfe_loop!();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel_exceptions::panic::panic_dump(info);
    wfe_loop!();
}
