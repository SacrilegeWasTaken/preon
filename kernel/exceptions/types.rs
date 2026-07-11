//! The [`TrapFrame`] — the CPU register snapshot saved at exception entry and
//! restored on `eret`.

use kernel_arch::mm::VirtAddr;
use kernel_arch::read_sysreg;
use kernel_arch::reg::{Esr, Spsr};
use kernel_builtin::kernel_log_raw;

/// CPU snapshot saved at exception entry and restored on `eret`.
///
/// The assembler trampoline (`ventry.s`) writes this frame on the kernel
/// stack; handlers receive `&mut TrapFrame` and may modify any field —
/// changes take effect when control returns to the interrupted code.
///
/// Layout is hardware-defined and mirrors what `save_context` /
/// `restore_context` macros store, so adding fields requires updating the
/// `_OFFSET` constants and `SIZE` together.
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
    pub const SIZE: usize = 272;

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

        kernel_log_raw!("");
        kernel_log_raw!("=== {} ===", label);
        kernel_log_raw!("Class    : {:?} ({:#04x})", esr.class(), esr.ec_raw());
        kernel_log_raw!("Reason   : {}", esr.class().description());
        kernel_log_raw!("ESR_EL1  : {:#018x}", esr.raw());
        kernel_log_raw!("ELR_EL1  : {:#018x}", self.elr());
        kernel_log_raw!("FAR_EL1  : {:#018x}", far);
        kernel_log_raw!("SPSR_EL1 : {:#018x} ({:?})", spsr, spsr.mode());
        kernel_log_raw!("SP_EL0   : {:#018x}", self.user_sp());
        for (i, x) in self.x.iter().enumerate() {
            kernel_log_raw!("x{:<2}      : {:#018x}", i, x);
        }
    }
}
