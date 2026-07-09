use crate::mm::Level;
use crate::reg::{Dfsc, Esr};

use self::FaultStatus::{AccessFlag, AddressSize, Alignment, Other, Permission, Translation};

/// A decoded data/instruction-abort fault status (`ESR_EL1.DFSC`/`IFSC`).
///
/// This is the *what happened* of a memory fault — the kernel's policy for
/// *what to do* about it lives in `kernel_exceptions`. The distinction
/// drives demand-paging: a [`Translation`] fault means the page isn't
/// mapped (fault it in), a [`Permission`] fault means it is mapped but the
/// access is disallowed (copy-on-write, protection), and an [`AccessFlag`]
/// fault means it's mapped but the access flag was clear (aging / lazy AF).
/// The `level` is the translation-table level at which the walk faulted.
#[derive(Clone, Copy, Debug)]
pub enum FaultStatus {
    /// Output address exceeds the configured PA size at `level`.
    AddressSize { level: Level },
    /// No valid descriptor at `level` — the page is not mapped.
    Translation { level: Level },
    /// Descriptor valid but its Access Flag is clear at `level`.
    AccessFlag { level: Level },
    /// Mapped, but the access violates the entry's permissions
    /// (e.g. write to read-only, execute on NX) at `level`.
    Permission { level: Level },
    /// Unaligned access that the memory type doesn't allow.
    Alignment,
    /// Anything else (external abort, parity/ECC, TLB conflict, …),
    /// carrying the raw 6-bit code for diagnostics.
    Other(u8),
}

impl FaultStatus {
    /// Human-readable name of the fault class, without the level (a
    /// `&'static str` can't carry the dynamic level — read it from
    /// [`FaultStatus::level`]). Mirrors [`ExceptionClass::description`].
    pub fn description(self) -> &'static str {
        match self {
            FaultStatus::AddressSize { .. } => "Address size fault",
            FaultStatus::Translation { .. } => "Translation fault",
            FaultStatus::AccessFlag { .. } => "Access flag fault",
            FaultStatus::Permission { .. } => "Permission fault",
            FaultStatus::Alignment => "Alignment fault",
            FaultStatus::Other(_) => "Other / External fault",
        }
    }
    /// Decode a raw fault status code (`ESR_EL1.DFSC`/`IFSC`).
    ///
    /// Codes `0x00..=0x0F` are the four levelled classes: the high two bits
    /// `[3:2]` select the class and the low two `[1:0]` the table level, so
    /// they split cleanly into (class, level). Codes `>= 0x10` (alignment,
    /// external aborts, parity, …) do **not** carry a level in `[1:0]` and
    /// are matched by their exact value, falling back to [`Other`].
    pub fn from_dfsc(dfsc: Dfsc) -> Self {
        let dfsc = dfsc.raw();
        match dfsc {
            0x00..=0x0F => {
                let level = Level::from_index(dfsc & 0b11);
                match dfsc >> 2 {
                    0b00 => FaultStatus::AddressSize { level },
                    0b01 => FaultStatus::Translation { level },
                    0b10 => FaultStatus::AccessFlag { level },
                    0b11 => FaultStatus::Permission { level },
                    // `dfsc >> 2` over 0x00..=0x0F is exactly 0..=3, so this
                    // arm is unreachable. Safe `unreachable!()` is marked
                    // cold/noreturn by LLVM and costs nothing on the happy
                    // path — `unreachable_unchecked` would only drop the cold
                    // branch for no real gain, at the price of UB risk.
                    _ => unreachable!(),
                }
            }
            0x21 => FaultStatus::Alignment,
            other => FaultStatus::Other(other),
        }
    }

    /// The translation-table level the fault was detected at, or `None` for
    /// classes that have no level ([`Alignment`], [`Other`]).
    pub fn level(&self) -> Option<Level> {
        match self {
            AddressSize { level }
            | Translation { level }
            | AccessFlag { level }
            | Permission { level } => Some(*level),
            Alignment | Other(_) => None,
        }
    }
}
/// Exception class decoded from `ESR_EL1.EC`.
#[derive(Debug, Clone, Copy)]
pub enum ExceptionClass {
    Unknown,
    TrappedFp,
    SvcAArch64,
    InstrAbortLowerEl,
    InstrAbortSameEl,
    PcAlignment,
    DataAbortLowerEl,
    DataAbortSameEl,
    SpAlignment,
    TrappedWfi,
    Brk,
    Other(u8),
}

impl ExceptionClass {
    /// Construct from the raw 6-bit `EC` field of `ESR_EL1`.
    pub fn from_ec(ec: u8) -> Self {
        match ec {
            0x00 => Self::Unknown,
            0x07 => Self::TrappedFp,
            0x15 => Self::SvcAArch64,
            0x20 => Self::InstrAbortLowerEl,
            0x21 => Self::InstrAbortSameEl,
            0x22 => Self::PcAlignment,
            0x24 => Self::DataAbortLowerEl,
            0x25 => Self::DataAbortSameEl,
            0x26 => Self::SpAlignment,
            0x2F => Self::TrappedWfi,
            0x3C => Self::Brk,
            other => Self::Other(other),
        }
    }

    /// Convenience wrapper preserved for callers that already have a raw ESR.
    pub fn from_esr(esr: u64) -> Self {
        Esr::from_raw(esr).class()
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Unknown => "Unknown reason (e.g. udf instruction)",
            Self::TrappedFp => "Trapped FP/SIMD/SVE access",
            Self::SvcAArch64 => "SVC system call from AArch64",
            Self::InstrAbortLowerEl => "Instruction abort from lower EL",
            Self::InstrAbortSameEl => "Instruction abort from same EL",
            Self::PcAlignment => "PC alignment fault",
            Self::DataAbortLowerEl => "Data abort from lower EL",
            Self::DataAbortSameEl => "Data abort from same EL",
            Self::SpAlignment => "SP alignment fault",
            Self::TrappedWfi => "Trapped WFI/WFE",
            Self::Brk => "Software breakpoint (brk)",
            Self::Other(_) => "Other / unhandled exception class",
        }
    }
}

/*
 *
 *  Formal verification (Kani model-checking harnesses)
 *
 *  Compiled only under `cargo kani`. Both decoders must be total over their
 *  raw input — a fault code the kernel can't classify would be a live bug.
 *
 */

#[cfg(kani)]
mod verification {
    use super::*;
    use crate::reg::Dfsc;

    /// `from_dfsc` classifies every 6-bit fault code without panicking, and
    /// the levelled classes (raw ≤ 0x0F) are exactly the ones carrying a level.
    #[kani::proof]
    fn fault_status_total() {
        let raw: u8 = kani::any();
        let fs = FaultStatus::from_dfsc(Dfsc::new(raw));

        match fs {
            FaultStatus::AddressSize { .. }
            | FaultStatus::Translation { .. }
            | FaultStatus::AccessFlag { .. }
            | FaultStatus::Permission { .. } => {
                assert!(raw <= 0x0F);
                assert!(fs.level().is_some());
            }
            FaultStatus::Alignment | FaultStatus::Other(_) => {
                assert!(fs.level().is_none());
            }
        }
        assert!(!fs.description().is_empty());
    }

    /// `ExceptionClass::from_ec` is total over every `EC` value and always
    /// yields a non-empty description.
    #[kani::proof]
    fn exception_class_total() {
        let ec: u8 = kani::any();
        let class = ExceptionClass::from_ec(ec);
        assert!(!class.description().is_empty());
    }
}
