#![no_std]
#![no_main]

pub mod uart;

use core::arch::{asm, global_asm};
use core::panic::PanicInfo;

// .section .text.boot
// .global _start
// _start:
//     # change the FP/SIMD flag at Coprocessor Access Control Register
//     mrs     x1, cpacr_el1
//     orr     x1, x1, #(0x3 << 20)
//     msr     cpacr_el1, x1
//     isb # Flush the CPU pipeline so FP/SIMD instructions coming after wont suddenly fail
//
//     # set the stack pointer
//     ldr     x1, =__stack_top
//     mov     sp, x1
//
//     # load .bss bounds and free it in a loop
//     ldr     x1, =__bss_start
//     ldr     x2, =__bss_end
// 1:
//     cmp     x1, x2
//     b.hs    2f
//     str     xzr, [x1], #8
//     b       1b
// 2:
//     bl      kmain
// 3:
//     wfe
//     b       3
global_asm!(
    r#"
.section .text.boot
.global _start
_start:
    mrs     x1, cpacr_el1
    orr     x1, x1, #(0x3 << 20)
    msr     cpacr_el1, x1
    isb

    ldr     x1, =__stack_top
    mov     sp, x1

    ldr     x1, =__bss_start
    ldr     x2, =__bss_end
1:
    cmp     x1, x2
    b.hs    2f
    str     xzr, [x1], #8
    b       1b
2:
    bl      kmain
3:
    wfe
    b       3b
"#
);

#[unsafe(no_mangle)]
pub extern "C" fn kmain() -> ! {
    uart::uart_write_str("Hello from exos!\n");
    loop {
        unsafe { asm!("wfe") }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {
        unsafe { asm!("wfe") }
    }
}
