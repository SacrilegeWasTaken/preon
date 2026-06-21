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

use crate::types::{Cacheability, Granule, PhysAddrSize, Shareability, VirtAddrSize};

/// Program `TCR_EL1` with [`TCR_VALUE`] and synchronize the pipeline.
///
/// Must run before [`super::mmu::enable`] flips `SCTLR_EL1.M`. Order
/// inside the MMU bring-up: MAIR → TCR → TTBR → SCTLR.
#[inline]
pub fn install() {
    let tcr = TcrConfig {
        ttbr0: TtbrConfig {
            va_size: VirtAddrSize::from_va_bits(48),
            granule: Granule::_4KB,
            inner_cache: Cacheability::WriteBackReadWriteAlloc,
            outer_cache: Cacheability::WriteBackReadWriteAlloc,
            shareability: Shareability::InnerShareable,
            tbi: true,
        },
        ttbr1: Some(TtbrConfig {
            va_size: VirtAddrSize::from_va_bits(48),
            granule: Granule::_4KB,
            inner_cache: Cacheability::WriteBackReadWriteAlloc,
            outer_cache: Cacheability::WriteBackReadWriteAlloc,
            shareability: Shareability::InnerShareable,
            tbi: false,
        }),
        ips: PhysAddrSize::_48BIT,
        asid_from_ttbr1: true,
    };

    write_sysreg!(tcr_el1, tcr.build().raw());
    flush_cpu_pipeline!();
}

#[derive(Clone, Copy, Debug)]
pub struct TtbrConfig {
    va_size: VirtAddrSize,
    granule: Granule,
    inner_cache: Cacheability,
    outer_cache: Cacheability,
    shareability: Shareability,
    tbi: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct TcrConfig {
    ttbr0: TtbrConfig,
    ttbr1: Option<TtbrConfig>,
    ips: PhysAddrSize,
    asid_from_ttbr1: bool,
}

impl TcrConfig {
    /// ASID source choice (0=TTBR0, 1=TTBR1)
    const A1_SHIFT: u8 = 22;

    /// Top-byte ignore for TTBR0
    const TBI0_SHIFT: u8 = 37;

    /// Top-byte ignore for TTBR1
    const TBI1_SHIFT: u8 = 38;

    /// `T0SZ` — log2 of the unused VA bits in the low half. `T0SZ = 16`
    /// reserves bits [63:48] for the high half and leaves 48 usable bits.
    const T0SZ_SHIFT: u8 = 0;

    /// `IRGN0` — inner cacheability of page-table walks via TTBR0.
    const IRGN0_SHIFT: u8 = 8;

    /// `ORGN0` — outer cacheability of page-table walks via TTBR0.
    const ORGN0_SHIFT: u8 = 10;

    /// `SH0` — shareability domain for TTBR0 page-table accesses.
    const SH0_SHIFT: u8 = 12;

    /// `TG0` — granule (page size) for TTBR0. Encoding differs from `TG1`.
    const TG0_SHIFT: u8 = 14;

    /// `T1SZ` — same role as `T0SZ`, but for the high half.
    const T1SZ_SHIFT: u8 = 16;

    /// `EPD1` — when 1, disables TTBR1 table walks entirely.
    const EPD1_SHIFT: u8 = 23;

    /// `IRGN1` — inner cacheability of TTBR1 walks.
    const IRGN1_SHIFT: u8 = 24;

    /// `ORGN1` — outer cacheability of TTBR1 walks.
    const ORGN1_SHIFT: u8 = 26;

    /// `SH1` — shareability for TTBR1 walks.
    const SH1_SHIFT: u8 = 28;

    /// `TG1` — granule for TTBR1. Encoding differs from `TG0` — read carefully.
    const TG1_SHIFT: u8 = 30;

    /// `IPS` — Intermediate Physical Address size, common to both halves.
    const IPS_SHIFT: u8 = 32;

    const fn build(self) -> TcrValue {
        let ttbr0_bits = (self.ttbr0.va_size.tsz_bits() << Self::T0SZ_SHIFT)
            | (self.ttbr0.inner_cache.bits() << Self::IRGN0_SHIFT)
            | (self.ttbr0.outer_cache.bits() << Self::ORGN0_SHIFT)
            | (self.ttbr0.shareability.bits() << Self::SH0_SHIFT)
            | (self.ttbr0.granule.to_tg0_bits() << Self::TG0_SHIFT)
            | ((self.ttbr0.tbi as u64) << Self::TBI0_SHIFT);

        let ttbr1_bits = match self.ttbr1 {
            Some(t1) => {
                (t1.va_size.tsz_bits() << Self::T1SZ_SHIFT)
                    | (t1.inner_cache.bits() << Self::IRGN1_SHIFT)
                    | (t1.outer_cache.bits() << Self::ORGN1_SHIFT)
                    | (t1.shareability.bits() << Self::SH1_SHIFT)
                    | (t1.granule.to_tg1_bits() << Self::TG1_SHIFT)
                    | ((t1.tbi as u64) << Self::TBI1_SHIFT)
            }
            None => {
                assert!(
                    !self.asid_from_ttbr1,
                    "A1=1 requires TTBR1 enabled (ASID source can't be a disabled TTBR)"
                );
                1u64 << Self::EPD1_SHIFT
            }
        };

        let common_bits = ((self.asid_from_ttbr1 as u64) << Self::A1_SHIFT)
            | (self.ips.bits() << Self::IPS_SHIFT);

        TcrValue {
            raw: (ttbr0_bits | ttbr1_bits | common_bits),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct TcrValue {
    raw: u64,
}

impl TcrValue {
    const fn raw(self) -> u64 {
        self.raw
    }
}
