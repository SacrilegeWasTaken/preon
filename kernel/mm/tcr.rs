//! TCR_EL1 (Translation Control Register) configuration.
//!
//! TCR_EL1 controls how virtual addresses are translated for both halves of
//! the address space (TTBR0 for the low half, TTBR1 for the high half).
//! Granule, virtual-address size, page-table cacheability, and
//! shareability all live here. The value below is computed at compile
//! time from named field constants so the actual register bits never
//! appear as a magic number.
//!
//! Reference: ARM Architecture Reference Manual, D13.2.131 (TCR_EL1).

use kernel_arch::{flush_cpu_pipeline, write_sysreg};

/*
 * Field positions (bit shifts inside TCR_EL1)
 */

/// `T0SZ` — log2 of the unused VA bits in the low half. `T0SZ = 16`
/// reserves bits [63:48] for the high half and leaves 48 usable bits.
const T0SZ_SHIFT: u64 = 0;

/// `IRGN0` — inner cacheability of page-table walks via TTBR0.
const IRGN0_SHIFT: u64 = 8;

/// `ORGN0` — outer cacheability of page-table walks via TTBR0.
const ORGN0_SHIFT: u64 = 10;

/// `SH0` — shareability domain for TTBR0 page-table accesses.
const SH0_SHIFT: u64 = 12;

/// `TG0` — granule (page size) for TTBR0. Encoding differs from `TG1`.
const TG0_SHIFT: u64 = 14;

/// `T1SZ` — same role as `T0SZ`, but for the high half.
const T1SZ_SHIFT: u64 = 16;

/// `EPD1` — when 1, disables TTBR1 table walks entirely.
const EPD1_SHIFT: u64 = 23;

/// `IRGN1` — inner cacheability of TTBR1 walks.
const IRGN1_SHIFT: u64 = 24;

/// `ORGN1` — outer cacheability of TTBR1 walks.
const ORGN1_SHIFT: u64 = 26;

/// `SH1` — shareability for TTBR1 walks.
const SH1_SHIFT: u64 = 28;

/// `TG1` — granule for TTBR1. Encoding differs from `TG0` — read carefully.
const TG1_SHIFT: u64 = 30;

/// `IPS` — Intermediate Physical Address size, common to both halves.
const IPS_SHIFT: u64 = 32;

/*
 * Field values
 */

/// 48-bit virtual addresses (`T0SZ = T1SZ = 16`).
const T0SZ_48BIT: u64 = 16;
const T1SZ_48BIT: u64 = 16;

/// Normal memory, write-back, read+write allocate. Applied to page-table
/// walks themselves so the MMU's accesses are cached like ordinary RAM.
const CACHEABILITY_WB_RWALLOC: u64 = 0b01;

/// Inner-shareable domain — all CPUs in the same cluster snoop each other.
const SHAREABILITY_INNER: u64 = 0b11;

/// 4 KiB granule in the `TG0` encoding (`00`).
const GRANULE_TG0_4KB: u64 = 0b00;

/// 4 KiB granule in the `TG1` encoding (`10`). Differs from `TG0` and is
/// a common source of subtle bring-up bugs — leave the comment alone.
const GRANULE_TG1_4KB: u64 = 0b10;

/// 36-bit Intermediate Physical Address — covers 64 GiB, plenty for the
/// 128 MiB QEMU virt setup and future hardware.
const IPS_36BIT: u64 = 0b001;

/// Disable TTBR1 walks until the kernel relocates to the upper half.
const EPD1_DISABLE: u64 = 0b1;

/*
 * Composite value
 */

/// Compile-time TCR_EL1 value installed by [`install`].
///
/// Summary:
///   - 48-bit virtual addresses on both halves (`T0SZ = T1SZ = 16`)
///   - 4 KiB granule for both halves
///   - Inner-shareable, write-back cacheable page-table walks
///   - TTBR1 disabled (upper-half kernel relocation lands later)
///   - 36-bit Intermediate Physical Address (64 GiB)
pub const TCR_VALUE: u64 = (T0SZ_48BIT << T0SZ_SHIFT)
    | (CACHEABILITY_WB_RWALLOC << IRGN0_SHIFT)
    | (CACHEABILITY_WB_RWALLOC << ORGN0_SHIFT)
    | (SHAREABILITY_INNER << SH0_SHIFT)
    | (GRANULE_TG0_4KB << TG0_SHIFT)
    | (T1SZ_48BIT << T1SZ_SHIFT)
    | (EPD1_DISABLE << EPD1_SHIFT)
    | (CACHEABILITY_WB_RWALLOC << IRGN1_SHIFT)
    | (CACHEABILITY_WB_RWALLOC << ORGN1_SHIFT)
    | (SHAREABILITY_INNER << SH1_SHIFT)
    | (GRANULE_TG1_4KB << TG1_SHIFT)
    | (IPS_36BIT << IPS_SHIFT);

/// Program `TCR_EL1` with [`TCR_VALUE`] and synchronize the pipeline.
///
/// Must run before [`super::mmu::enable`] flips `SCTLR_EL1.M`. Order
/// inside the MMU bring-up: MAIR → TCR → TTBR → SCTLR.
#[inline]
pub fn install() {
    write_sysreg!(tcr_el1, TCR_VALUE);
    flush_cpu_pipeline!();
}
