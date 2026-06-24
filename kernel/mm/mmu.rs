//! Post-MMU MMU operations.
//!
//! Pre-MMU setup is done in boot.s assembly. This module hosts runtime
//! MMU manipulations like TTBR switching, TLB invalidation, etc.

use kernel_arch::mm::PhysAddr;

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

/// Tear down the low-half (TTBR0) translation regime, committing the CPU
/// to upper-half-only addressing.
///
/// During early boot the trampoline identity-maps the low half through
/// TTBR0 so the kernel can execute from its physical load address while the
/// MMU comes up. Once execution has fully moved into the upper half (image
/// + linear + device regions, all via TTBR1) that identity map is dead
/// weight — and a hazard. This routine retires it in two steps:
///
/// 1. Set `TCR_EL1.EPD0` (bit 7): disable table walks for the TTBR0 VA
///    range. A miss in the low half now raises a level-0 translation fault
///    instead of walking a stale (or zeroed) root. As a side benefit this
///    turns null-pointer dereferences and any dangling identity pointers
///    into clean, catchable faults rather than silent accesses to PA 0.
/// 2. Zero `TTBR0_EL1` so no stale root lingers (belt-and-suspenders; with
///    EPD0 set the walker never consults it anyway).
///
/// The `dsb`/`tlbi`/`dsb`/`isb` sequence publishes the system-register
/// writes, drops any cached low-half TLB entries, waits for completion, and
/// resynchronizes the pipeline so the next fetch observes the new regime.
///
/// After this returns, the trampoline page tables in `.boot.bss` are no
/// longer referenced and may be reclaimed once a real allocator exists.
///
/// # Safety
/// Nothing the kernel touches may resolve through TTBR0 after this call.
/// The caller must guarantee that, by this point, the running code, the
/// stack pointer, the FDT pointer, and every page-table frame are all
/// reachable through TTBR1 (image or linear). Concretely: `SP` must already
/// be rebased onto the image-region stack (not the low boot stack), and the
/// DTB must have been re-based into the linear map. If any live pointer
/// still aims at the low half, the next access through it faults.
pub unsafe fn disable_ttbr0() {
    // TCR_EL1.EPD0 — "Translation table walk disable for TTBR0_EL1".
    const TCR_EPD0: u64 = 1 << 7;

    unsafe {
        core::arch::asm!(
            // Read-modify-write TCR to set EPD0, disabling low-half walks.
            "mrs {tcr}, tcr_el1",
            "orr {tcr}, {tcr}, {epd0}",
            "msr tcr_el1, {tcr}",
            // Drop the stale root so nothing can accidentally walk it.
            "msr ttbr0_el1, xzr",
            // Publish the writes, flush low-half TLB, wait, resync pipeline.
            "dsb ish",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",

            tcr = out(reg) _,
            epd0 = in(reg) TCR_EPD0,
            options(nostack),
        );
    }
}
