//! Top-level exception handlers reached from `ventry.s`.
//!
//! Every slot in the `VBAR_EL1` vector table funnels into one of these via
//! the `impl_handler` macro, which saves a [`TrapFrame`] on the kernel
//! stack and passes it by reference. The functions are `#[no_mangle]` so
//! the assembler can reference them by symbol. Most are dead-ends that dump
//! the frame and park; the synchronous EL1 path additionally classifies the
//! cause and routes aborts to the page-fault handler.

use kernel_arch::exceptions::ExceptionClass;
use kernel_arch::reg::Esr;
use kernel_arch::wfe_loop;

use crate::page_fault::handle_page_fault;
use crate::types::TrapFrame;

#[unsafe(no_mangle)]
extern "C" fn bad_mode_handler(frame: &mut TrapFrame) {
    frame.dump("BAD MODE");
    wfe_loop!()
}

/// Synchronous exception taken at EL1. Same-EL data/instruction aborts are
/// routed to [`handle_page_fault`]; everything else (SVC-from-kernel, `brk`,
/// `udf`, alignment, …) falls through to the generic register dump. Both
/// arms currently terminate in `wfe_loop`, so the trailing park is reached
/// only by the dump arm.
#[unsafe(no_mangle)]
extern "C" fn el1_sync_handler(frame: &mut TrapFrame) {
    match Esr::current().class() {
        ExceptionClass::DataAbortSameEl | ExceptionClass::InstrAbortSameEl => {
            handle_page_fault(frame)
        }
        _ => frame.dump("EL1 SYNC EXCEPTION"),
    }
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
