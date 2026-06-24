use kernel_arch::mm::VirtAddr;
use kernel_arch::read_sysreg;
use kernel_arch::reg::{Esr, Spsr};
use kernel_builtin::kernel_uart_direct_log;

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
