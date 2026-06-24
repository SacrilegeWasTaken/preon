#![no_std]
//! Exception handling: the `VBAR_EL1` vector table, the saved-context frame,
//! and the handlers reached from it.
//!
//! - [`types`] — the [`types::TrapFrame`] mirrored from `ventry.s`.
//! - [`handlers`] — the per-vector entry points (`#[no_mangle]`, called by
//!   the assembler) and the synchronous-abort dispatch.
//! - [`page_fault`] — abort classification and reporting (future demand
//!   paging seam).
//! - [`panic`] — the kernel panic dump.
//!
//! The vector table itself and the save/restore trampolines live in
//! `asm/ventry.s`; [`ExceptionVectors::install`] publishes its address.

use core::arch::global_asm;

pub mod handlers;
pub mod page_fault;
pub mod panic;
pub mod types;

// Vector table and dispatch trampolines for EL1.
global_asm!(include_str!("asm/ventry.s"));

unsafe extern "C" {
    static vector_table: u8;
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
