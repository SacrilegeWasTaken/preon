//! MMU enable sequence.
//!
//! Bringing up translation on aarch64 is a precise dance: every register
//! the MMU consults must be programmed before `SCTLR_EL1.M` flips, and
//! the instruction stream needs an identity mapping so the very next
//! fetch still resolves to the same physical address it did before the
//! flip. The sequence below is the one prescribed by the ARM ARM.

use kernel_arch::{flush_cpu_pipeline, read_sysreg, write_sysreg};

use crate::{attrs, identity, tcr};

/// `SCTLR_EL1.M` — MMU enable. When set, all loads/stores/fetches go
/// through translation.
const SCTLR_M: u64 = 1 << 0;

/// Build identity mappings, install MAIR / TCR / TTBR0, flush the TLB,
/// and flip `SCTLR_EL1.M`. After this returns the CPU sees the world
/// through translated addresses; the identity map ensures the
/// instruction stream survives the flip.
///
/// # Safety
/// Must only be called once per CPU, with the MMU currently disabled.
/// The caller commits to:
///   - `SCTLR_EL1.M` being clear on entry,
///   - the kernel image, stack, and PL011 UART being inside the regions
///     that [`identity::build`] covers,
///   - no other code path racing to touch `MAIR_EL1`, `TCR_EL1`,
///     `TTBR0_EL1`, or `SCTLR_EL1` during the call.
pub unsafe fn enable() {
    // Memory types must be ready before any TTBR write — page-table
    // entries reference MAIR slots by index.
    attrs::install();

    // Translation control must be ready before SCTLR.M for the same
    // reason: MMU consults it on the first walk.
    tcr::install();

    // Identity-mapped root. Lives inside the bootstrap pool, so it is
    // already covered by `map_ram` and survives the flip.
    let root_pa = identity::build();
    write_sysreg!(ttbr0_el1, root_pa.as_u64());

    // Make every config register write globally visible, then wipe any
    // stale TLB state and serialize the pipeline before flipping.
    unsafe {
        core::arch::asm!(
            "dsb ish",
            "tlbi vmalle1is",
            "dsb ish",
            "isb",
            options(nostack, preserves_flags),
        );
    }

    // Flip SCTLR.M. From here on every memory access translates.
    let sctlr = read_sysreg!(sctlr_el1) | SCTLR_M;
    write_sysreg!(sctlr_el1, sctlr);

    // The next instruction fetch must already see the new SCTLR; an
    // `isb` purges the pipeline so the CPU re-fetches through the MMU.
    flush_cpu_pipeline!();
}
