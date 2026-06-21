//! MMU enable sequence.
//!
//! Bringing up translation on aarch64 is a precise dance: every register
//! the MMU consults must be programmed before `SCTLR_EL1.M` flips, and
//! the instruction stream needs an identity mapping so the very next
//! fetch still resolves to the same physical address it did before the
//! flip. The sequence below is the one prescribed by the ARM ARM.

use crate::{attrs, tcr};

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
    // TODO: rewrite for assembly-driven bringup.
    // Pre-MMU page-table setup will move to boot.s; this function will
    // handle only post-MMU operations (TTBR switch to "real" map, TLB
    // invalidate, etc.).
    attrs::install();
    tcr::install();
}
