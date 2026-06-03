use core::arch::global_asm;

use crate::kernel_uart_direct_log;
/// CPU snapshot taken at the moment of an exception.
///
/// Saved by the assembler trampoline on entry, restored before `eret`.
/// The handler receives `&mut TrapFrame` and may modify any field —
/// changes take effect when control returns to the interrupted code.
///
/// # Fields
///
/// - `x` — general-purpose registers `x0..x30` (full integer state of
///   the interrupted code).
/// - `sp_el0` — stack pointer of EL0; meaningful only when the exception
///   came from EL0, undefined otherwise.
/// - `elr_el1` — Exception Link Register: address to return to via `eret`.
/// - `spsr_el1` — saved `PSTATE` at the moment of exception:
///   - `NZCV` — condition flags from the last comparison
///   - `DAIF` — interrupt masks (`Debug`, `SError`, `IRQ`, `FIQ`)
///   - `M[3:0]` — source EL and SP selection (`EL1h`, `EL1t`, `EL0t`)
///   - `IL` — Illegal Execution State flag
///   - `SS` — single-step debug flag
#[derive(Debug)]
#[repr(C)]
pub struct TrapFrame {
    pub x: [u64; 31],
    pub sp_el0: u64,
    pub elr_el1: u64,
    pub spsr_el1: u64,
}

impl TrapFrame {
    pub const X_OFFSET: usize = 0; // x[0] starts from 0
    pub const SP_EL0_OFFSET: usize = 31 * 8; // 248
    pub const ELR_OFFSET: usize = 31 * 8 + 8; // 256
    pub const SPSR_OFFSET: usize = 31 * 8 + 16; // 264
    pub const FRAME_SIZE: usize = 272;
}

global_asm!(
    r#"
.section .text.vectors
.balign 2048
.global vector_table
vector_table:
.balign 0x80
sync_curr_sp0:
    mov x0, #0
    b common_handler

.balign 0x80
irq_curr_sp0:
    mov x0, #1
    b common_handler

.balign 0x80
fiq_curr_sp0:
    mov x0, #2
    b common_handler

.balign 0x80
serror_curr_sp0:
    mov x0, #3
    b common_handler

.balign 0x80
sync_curr_spx:
    mov x0, #4
    b common_handler

.balign 0x80
irq_curr_spx:
    mov x0, #5
    b common_handler

.balign 0x80
fiq_curr_spx:
    mov x0, #6
    b common_handler

.balign 0x80
serror_curr_spx:
    mov x0, #7
    b common_handler

.balign 0x80
sync_lower_64:
    mov x0, #8
    b common_handler

.balign 0x80
irq_lower_64:
    mov x0, #9
    b common_handler

.balign 0x80
fiq_lower_64:
    mov x0, #10
    b common_handler

.balign 0x80
serror_lower_64:
    mov x0, #11
    b common_handler

.balign 0x80
sync_lower_32:
    mov x0, #12
    b common_handler

.balign 0x80
irq_lower_32:
    mov x0, #13
    b common_handler

.balign 0x80
fiq_lower_32:
    mov x0, #14
    b common_handler

.balign 0x80
serror_lower_32:
    mov x0, #15
    b common_handler

.balign 0x80
common_handler:
    bl rust_exception_entry
1:
    wfe
    b 1b
"#
);

#[unsafe(no_mangle)]
fn rust_exception_entry(vector: u64) -> ! {
    let esr_el1: u64;
    let elr_el1: u64;
    let far_el1: u64;
    unsafe {
        core::arch::asm!("mrs {}, esr_el1", out(reg) esr_el1);
        core::arch::asm!("mrs {}, elr_el1", out(reg) elr_el1);
        core::arch::asm!("mrs {}, far_el1", out(reg) far_el1);
    };
    kernel_uart_direct_log!("EXCEPTION CAUGHT");
    kernel_uart_direct_log!("vector = {}", vector);
    kernel_uart_direct_log!("ESR_EL1 = {:#018x}", esr_el1);
    kernel_uart_direct_log!("ELR_EL1 = {:#018x}", elr_el1);
    kernel_uart_direct_log!("FAR_EL1 = {:#018x}", far_el1);

    loop {
        unsafe { core::arch::asm!("wfe") }
    }
}

unsafe extern "C" {
    static vector_table: u8;
}

pub fn install() {
    unsafe {
        core::arch::asm!(
            "msr vbar_el1, {0}",
            "isb",
            in(reg) &raw const vector_table
        )
    }
}
