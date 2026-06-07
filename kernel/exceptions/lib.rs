#![no_std]

use core::arch::global_asm;

use kernel_arch::{read_sysreg, Esr, Spsr, VirtAddr};
use kernel_builtin::{kernel_uart_direct_log, wfe_loop};

/// CPU snapshot saved at exception entry and restored on `eret`.
///
/// The assembler trampoline (`ventry.s`) writes this frame on the kernel
/// stack; handlers receive `&mut TrapFrame` and may modify any field —
/// changes take effect when control returns to the interrupted code.
///
/// Layout is hardware-defined and mirrors what `save_context` /
/// `restore_context` macros store, so adding fields requires updating the
/// `_OFFSET` constants and `FRAME_SIZE` together.
#[derive(Debug)]
#[repr(C)]
pub struct TrapFrame {
    pub x: [u64; 31],
    pub sp_el0: u64,
    pub elr_el1: u64,
    pub spsr_el1: u64,
}

impl TrapFrame {
    pub const X_OFFSET: usize = 0;
    pub const SP_EL0_OFFSET: usize = 31 * 8;
    pub const ELR_OFFSET: usize = 31 * 8 + 8;
    pub const SPSR_OFFSET: usize = 31 * 8 + 16;
    pub const FRAME_SIZE: usize = 272;

    /// PC the interrupted code will resume at, decoded as a virtual address.
    pub fn elr(&self) -> VirtAddr {
        VirtAddr::new(self.elr_el1 as usize)
    }

    /// EL0 stack pointer at the time of exception. Meaningful only when
    /// the exception came from EL0.
    pub fn user_sp(&self) -> VirtAddr {
        VirtAddr::new(self.sp_el0 as usize)
    }

    /// Saved `PSTATE` at the moment of exception.
    pub fn spsr(&self) -> Spsr {
        Spsr::from_raw(self.spsr_el1)
    }

    /// Pretty-print the frame to the emergency UART. Used by handlers
    /// that have no good way to recover.
    pub fn dump(&self, label: &str) {
        let esr = Esr::current();
        let far = VirtAddr::new(read_sysreg!(far_el1) as usize);
        let spsr = self.spsr();

        kernel_uart_direct_log!("");
        kernel_uart_direct_log!("=== {} ===", label);
        kernel_uart_direct_log!("Class    : {:?} ({:#04x})", esr.class(), esr.ec_raw());
        kernel_uart_direct_log!("Reason   : {}", esr.class().description());
        kernel_uart_direct_log!("ESR_EL1  : {:#018x}", esr.raw());
        kernel_uart_direct_log!("ELR_EL1  : {:#018x}", self.elr());
        kernel_uart_direct_log!("FAR_EL1  : {:#018x}", far);
        kernel_uart_direct_log!("SPSR_EL1 : {:#018x} ({:?})", spsr, spsr.mode());
        kernel_uart_direct_log!("SP_EL0   : {:#018x}", self.user_sp());
        for (i, x) in self.x.iter().enumerate() {
            kernel_uart_direct_log!("x{:<2}      : {:#018x}", i, x);
        }
    }
}

// Vector table and dispatch trampolines for EL1.
global_asm!(include_str!("asm/ventry.s"));

unsafe extern "C" {
    static vector_table: u8;
}

/// Print panic context to the emergency UART.
///
/// There is no real TrapFrame here — read the live system registers,
/// the current SP, and the panic info. `ESR_EL1` / `ELR_EL1` / `FAR_EL1`
/// may carry stale values from an earlier exception; surface them anyway
/// because they're often the most useful clue.
pub fn panic_dump(info: &core::panic::PanicInfo) {
    let esr = Esr::current();
    let elr = VirtAddr::new(read_sysreg!(elr_el1) as usize);
    let far = VirtAddr::new(read_sysreg!(far_el1) as usize);
    let spsr = Spsr::from_raw(read_sysreg!(spsr_el1));
    let mpidr = read_sysreg!(mpidr_el1);
    let sp: usize;
    unsafe {
        core::arch::asm!(
            "mov {}, sp",
            out(reg) sp,
            options(nomem, nostack, preserves_flags),
        );
    }
    let sp = VirtAddr::new(sp);

    kernel_uart_direct_log!("");
    kernel_uart_direct_log!("=== KERNEL PANIC ===");
    if let Some(loc) = info.location() {
        kernel_uart_direct_log!("Location : {}:{}:{}", loc.file(), loc.line(), loc.column());
    }
    kernel_uart_direct_log!("Message  : {}", info.message());
    kernel_uart_direct_log!("MPIDR    : {:#018x}", mpidr);
    kernel_uart_direct_log!("SP       : {:#018x}", sp);
    kernel_uart_direct_log!(
        "ESR_EL1  : {:#018x} ({:?} / {:#04x})",
        esr.raw(),
        esr.class(),
        esr.ec_raw()
    );
    kernel_uart_direct_log!("Reason   : {}", esr.class().description());
    kernel_uart_direct_log!("ELR_EL1  : {:#018x}", elr);
    kernel_uart_direct_log!("FAR_EL1  : {:#018x}", far);
    kernel_uart_direct_log!("SPSR_EL1 : {:#018x} ({:?})", spsr, spsr.mode());
}

/// Public interface to the kernel's exception vector table.
pub struct ExceptionVectors;

impl ExceptionVectors {
    /// Publish the address of `vector_table` to `VBAR_EL1`. Idempotent;
    /// each CPU calls this once during its own bring-up.
    pub fn install() {
        unsafe {
            core::arch::asm!(
                "msr vbar_el1, {0}",
                "isb",
                in(reg) &raw const vector_table,
                options(nostack, preserves_flags),
            );
        }
    }
}

// Handlers reached from `ventry.s`.
//
// Every slot in the vector table funnels into one of these via the
// `impl_handler` macro. Names are mangled-free because the assembler
// references them by symbol.

#[unsafe(no_mangle)]
extern "C" fn bad_mode_handler(frame: &mut TrapFrame) {
    frame.dump("BAD MODE");
    wfe_loop!()
}

#[unsafe(no_mangle)]
extern "C" fn el1_sync_handler(frame: &mut TrapFrame) {
    frame.dump("EL1 SYNC EXCEPTION");
    wfe_loop!()
}

#[unsafe(no_mangle)]
extern "C" fn el1_irq_handler(frame: &mut TrapFrame) {
    frame.dump("EL1 IRQ (unhandled)");
    wfe_loop!()
}

#[unsafe(no_mangle)]
extern "C" fn el1_fiq_handler(frame: &mut TrapFrame) {
    frame.dump("EL1 FIQ (unhandled)");
    wfe_loop!()
}

#[unsafe(no_mangle)]
extern "C" fn el1_serror_handler(frame: &mut TrapFrame) {
    frame.dump("EL1 SError");
    wfe_loop!()
}

#[unsafe(no_mangle)]
extern "C" fn el0_sync_handler(frame: &mut TrapFrame) {
    frame.dump("EL0 SYNC (unhandled)");
    wfe_loop!()
}

#[unsafe(no_mangle)]
extern "C" fn el0_irq_handler(frame: &mut TrapFrame) {
    frame.dump("EL0 IRQ (unhandled)");
    wfe_loop!()
}

#[unsafe(no_mangle)]
extern "C" fn el0_fiq_handler(frame: &mut TrapFrame) {
    frame.dump("EL0 FIQ (unhandled)");
    wfe_loop!()
}

#[unsafe(no_mangle)]
extern "C" fn el0_serror_handler(frame: &mut TrapFrame) {
    frame.dump("EL0 SError");
    wfe_loop!()
}
