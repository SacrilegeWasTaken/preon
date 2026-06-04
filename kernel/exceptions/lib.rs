#![no_std]

use core::arch::global_asm;
use kernel_arch::{read_sysreg, ExceptionClass};
use kernel_builtin::kernel_uart_direct_log;
use kernel_builtin::wfe_loop;
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
// Construct Exception Vector Table
// Construct [TrapFrame] onto the stack
// Calling exact Rust's handler
global_asm!(include_str!("asm/ventry.s"));

#[unsafe(no_mangle)]
extern "C" fn bad_mode_handler(frame: &mut TrapFrame) {}

#[unsafe(no_mangle)]
extern "C" fn el1_sync_handler(frame: &mut TrapFrame) {
    let esr = read_sysreg!(esr_el1);
    let far = read_sysreg!(far_el1);
    let ec = ExceptionClass::from_esr(esr);
    let ec_raw = ((esr >> 26) & 0x3f) as u8;

    kernel_uart_direct_log!("");
    kernel_uart_direct_log!("=== EL1 SYNC EXCEPTION ===");
    kernel_uart_direct_log!("Class    : {:?} ({:#04x})", ec, ec_raw);
    kernel_uart_direct_log!("Reason   : {}", ec.description());
    kernel_uart_direct_log!("ESR_EL1  : {:#018x}", esr);
    kernel_uart_direct_log!("ELR_EL1  : {:#018x}", frame.elr_el1);
    kernel_uart_direct_log!("FAR_EL1  : {:#018x}", far);
    kernel_uart_direct_log!("SPSR_EL1 : {:#018x}", frame.spsr_el1);

    wfe_loop!()
}

#[unsafe(no_mangle)]
extern "C" fn el1_irq_handler(frame: &mut TrapFrame) {}

#[unsafe(no_mangle)]
extern "C" fn el1_fiq_handler(frame: &mut TrapFrame) {}

#[unsafe(no_mangle)]
extern "C" fn el1_serror_handler(frame: &mut TrapFrame) {}

#[unsafe(no_mangle)]
extern "C" fn el0_sync_handler(frame: &mut TrapFrame) {}

#[unsafe(no_mangle)]
extern "C" fn el0_irq_handler(frame: &mut TrapFrame) {}

#[unsafe(no_mangle)]
extern "C" fn el0_fiq_handler(frame: &mut TrapFrame) {}

#[unsafe(no_mangle)]
extern "C" fn el0_serror_handler(frame: &mut TrapFrame) {}

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
