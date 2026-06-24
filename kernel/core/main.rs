#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

use kernel_arch::wfe_loop;
use kernel_builtin::{kernel_uart_direct_log, kernel_uart_log};
use kernel_exceptions::ExceptionVectors;

// Boot stub: drop EL2 -> EL1, enable FP/SIMD, set the kernel stack,
// zero .bss, and jump into `kmain` with the DTB pointer in x0.
global_asm!(include_str!("asm/boot.s"));

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

    kernel_uart_direct_log!("Triggering exception...");
    unsafe { core::arch::asm!("udf #0") }

    wfe_loop!();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    kernel_exceptions::panic::panic_dump(info);
    wfe_loop!();
}
