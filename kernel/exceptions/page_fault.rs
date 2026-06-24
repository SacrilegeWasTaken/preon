//! Page-fault path for data/instruction aborts.
//!
//! Reached from the synchronous-exception dispatch in [`crate::handlers`]
//! once the exception class is identified as an abort. Today it classifies
//! the fault and reports it, then parks — there is no address space to
//! repair against yet. This is the seam where demand paging, copy-on-write,
//! and userspace fault handling will plug in: the decode ([`Esr::fault_status`],
//! [`Esr::is_write`], `FAR_EL1`) already distinguishes the cases that future
//! recovery will branch on.

use kernel_arch::exceptions::ExceptionClass::{InstrAbortLowerEl, InstrAbortSameEl};
use kernel_arch::reg::Esr;
use kernel_arch::{read_sysreg, wfe_loop};
use kernel_builtin::kernel_uart_direct_log;

use crate::types::TrapFrame;

/// Decode and report a synchronous abort, then park.
///
/// Reads the live `ESR_EL1`/`FAR_EL1`, classifies the fault
/// ([`FaultStatus`] + level), determines the access kind, and dumps a
/// focused report to the emergency UART. Fatal for now (`wfe_loop`):
/// recovery belongs to later phases.
///
/// The access kind is taken from the exception class, not blindly from
/// `WnR`: instruction aborts are always fetches ("execute"), while data
/// aborts are "write"/"read" per [`Esr::is_write`] — querying `WnR` on an
/// instruction abort would read an unrelated bit.
pub fn handle_page_fault(frame: &TrapFrame) {
    let esr = Esr::current();
    let far = read_sysreg!(far_el1);

    // FAR is only trustworthy when FnV == 0 (see `Esr::far_valid`).
    let is_far_valid = esr.far_valid();

    let status = esr.fault_status();
    let access = match esr.class() {
        InstrAbortSameEl | InstrAbortLowerEl => "execute",
        _ => {
            if esr.is_write() {
                "write"
            } else {
                "read"
            }
        }
    };
    let elr = frame.elr().as_u64();
    kernel_uart_direct_log!(
        "=== PAGE FAULT ===\nSTATUS: {}\nLEVEL: {:?}\nACCESS: {}\nFAR: {:#018x}\nFAR_VALID: {}\nELR: {:#018x}\nESR: {:#018x}",
        status.description(),
        status.level(),
        access,
        far,
        is_far_valid,
        elr,
        esr.raw()
    );
    wfe_loop!()
}
