//! Core CPU identity types: the logical [`CpuId`] and the hardware [`Mpidr`].

/// Logical CPU index, used to address per-CPU arrays (stacks, data, boot info).
///
/// `CpuId::PRIMARY` is reserved for the boot CPU. Secondary CPUs are
/// assigned sequentially during SMP bring-up; the mapping is not the same
/// as the hardware MPIDR.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct CpuId(u32);

impl CpuId {
    pub const PRIMARY: Self = Self(0);

    pub const fn new(idx: u32) -> Self {
        Self(idx)
    }

    pub const fn raw(self) -> u32 {
        self.0
    }

    pub const fn as_usize(self) -> usize {
        self.0 as usize
    }
}

/// Multiprocessor Affinity Register value identifying a physical CPU.
///
/// Only the affinity bits (AFF0..AFF3 in `MPIDR_EL1[39:0]`) are kept;
/// the U and MT flags and reserved bits are masked off so equality
/// compares logical identity rather than register layout.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Mpidr(u64);

impl Mpidr {
    // AFF0..AFF2 in [23:0] and AFF3 in [39:32]; strips RES1, U, MT and
    // reserved bits so that
    // MPIDR_EL1 of a primary CPU compares equal to its DTB `cpu@N/reg`.
    const MASK: u64 = 0x00FF_00FF_FFFF;

    pub const fn new(raw: u64) -> Self {
        Self(raw & Self::MASK)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Read the current CPU's MPIDR_EL1.
    #[cfg(not(kani))]
    pub fn current() -> Self {
        let value: u64;
        unsafe {
            core::arch::asm!(
                "mrs {}, mpidr_el1",
                out(reg) value,
                options(nomem, nostack, preserves_flags),
            );
        }
        Self::new(value)
    }
}

/*
 *
 *  Formal verification (Kani model-checking harnesses)
 *
 */

#[cfg(kani)]
mod verification {
    use super::*;

    /// Construction preserves all four affinity fields and clears every
    /// non-affinity bit.
    #[kani::proof]
    fn mpidr_keeps_only_affinity_fields() {
        let raw: u64 = kani::any();
        assert!(Mpidr::new(raw).raw() == (raw & Mpidr::MASK));
    }

    /// Affinity level 3 participates in CPU identity rather than being
    /// discarded as it was by the old low-24-bit mask.
    #[kani::proof]
    fn aff3_distinguishes_cpus() {
        let low_affinity: u64 = kani::any::<u32>() as u64 & 0x00FF_FFFF;
        let aff3_a: u8 = kani::any();
        let aff3_b: u8 = kani::any();
        kani::assume(aff3_a != aff3_b);

        let a = low_affinity | ((aff3_a as u64) << 32);
        let b = low_affinity | ((aff3_b as u64) << 32);
        assert!(Mpidr::new(a) != Mpidr::new(b));
    }
}
