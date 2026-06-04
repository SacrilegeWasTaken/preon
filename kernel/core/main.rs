#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

use kernel_builtin::{kernel_uart_direct_log, kernel_uart_log, wfe_loop};

// Boot. Allow FP & SIMD / Setting the stack pointer / clear .bss section
global_asm!(include_str!("asm/boot.s"));

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    kernel_exceptions::install();
    kernel_uart_log!("Hello from Exos: {}", 3);

    kernel_uart_direct_log!("Triggering exception...");
    unsafe { core::arch::asm!("udf #0") }
    kernel_uart_direct_log!("This should never print!");

    wfe_loop!();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    wfe_loop!();
}
