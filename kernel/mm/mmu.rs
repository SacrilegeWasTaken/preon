//! Post-MMU MMU operations.
//!
//! Pre-MMU setup is done in boot.s assembly. This module hosts runtime
//! MMU manipulations like TTBR switching, TLB invalidation, etc.

use crate::frame::PhysAddr;

/// Switch TTBR1_EL1 to a new kernel-side page table root and flush
/// stale TLB state. Caller commits to:
/// - `new_root` is a 4 KiB-aligned, valid root table covering at least
///   the currently-executing kernel image and stack.
/// - No other CPU concurrently touches TTBR1 during the call.
///
/// # Safety
/// Using a malformed root will fault the next memory access through
/// upper-half VA.
pub unsafe fn switch_ttbr1(new_root: PhysAddr) {
    unsafe {
        // Install the new root.
        core::arch::asm!("msr ttbr1_el1, {}", in(reg) new_root.as_u64(), options(nostack));

        // Make the TTBR write globally observable, drop stale TLB
        // entries, wait for invalidation to complete, then synchronize
        // pipeline so subsequent fetches use the new map.
        core::arch::asm!(
            "dsb ish",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            options(nostack, preserves_flags),
        );
    }
}
