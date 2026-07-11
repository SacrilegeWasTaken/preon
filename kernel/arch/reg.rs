//! Typed decoders for the aarch64 system registers read at exception entry:
//! [`Spsr`], [`Esr`], and the data-fault status code [`Dfsc`].

use crate::exceptions::{ExceptionClass, FaultStatus};
use crate::read_sysreg;

/// Saved Program Status Register snapshot. Holds `PSTATE` at the moment
/// an exception was taken: NZCV flags, DAIF masks, the originating
/// exception level, the active SP selector, and a handful of debug bits.
#[derive(Debug, Copy, Clone)]
pub struct Spsr(u64);

impl Spsr {
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    /// Bits `M[4:0]` — source exception level and SP selector.
    pub const fn mode_bits(self) -> u8 {
        (self.0 & 0b1_1111) as u8
    }

    /// Decoded source mode.
    pub fn mode(self) -> SpsrMode {
        SpsrMode::from_bits(self.mode_bits())
    }
}

impl core::fmt::LowerHex for Spsr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::LowerHex::fmt(&self.0, f)
    }
}

/// Source exception level + SP selector, decoded from `SPSR.M[4:0]`.
///
/// The `t` / `h` suffix distinguishes the SP used at the source EL:
/// `t` means SP_EL0, `h` means SP_ELx of the source level.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SpsrMode {
    El0t,
    El1t,
    El1h,
    El2t,
    El2h,
    El3t,
    El3h,
    AArch32(u8),
    Unknown(u8),
}

impl SpsrMode {
    pub const fn from_bits(bits: u8) -> Self {
        match bits {
            0b00000 => Self::El0t,
            0b00100 => Self::El1t,
            0b00101 => Self::El1h,
            0b01000 => Self::El2t,
            0b01001 => Self::El2h,
            0b01100 => Self::El3t,
            0b01101 => Self::El3h,
            // 0b10000.. is AArch32 modes (user, FIQ, IRQ, SVC, ...).
            b if b & 0b1_0000 != 0 => Self::AArch32(b),
            other => Self::Unknown(other),
        }
    }
}

/// Exception Syndrome Register (`ESR_EL1`) value.
///
/// Bit layout: `EC[31:26]`, `IL[25]`, `ISS[24:0]`. Helpers below
/// decode the standard sub-fields without exposing the raw integer
/// arithmetic to call sites.
#[derive(Debug, Copy, Clone)]
pub struct Esr(u64);

impl Esr {
    /// Snapshot `ESR_EL1` from the current CPU.
    pub fn current() -> Self {
        Self(read_sysreg!(esr_el1))
    }

    pub const fn raw(self) -> u64 {
        self.0
    }

    /// `EC` field — coarse classifier (load it through `class()` for an enum).
    pub const fn ec_raw(self) -> u8 {
        ((self.0 >> 26) & 0x3F) as u8
    }

    pub fn class(self) -> ExceptionClass {
        ExceptionClass::from_ec(self.ec_raw())
    }

    /// Instruction Length: true if the trapped instruction was 32-bit,
    /// false if 16-bit (T32).
    pub const fn il(self) -> bool {
        (self.0 >> 25) & 1 != 0
    }

    /// Instruction-specific syndrome bits. Interpretation depends on `EC`.
    pub const fn iss(self) -> u32 {
        (self.0 & 0x01FF_FFFF) as u32
    }

    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Raw `DFSC`/`IFSC` field (`ISS[5:0]`) — the fault status code of a
    /// data or instruction abort. Only meaningful for abort exception
    /// classes; for any other `EC` these bits carry unrelated syndrome.
    pub const fn dfsc(self) -> Dfsc {
        Dfsc((self.0 & 0x3F) as u8)
    }

    /// Decode the fault status code into a typed [`FaultStatus`]
    /// (translation / permission / access-flag / … with a level). Call
    /// only on a data or instruction abort.
    pub fn fault_status(self) -> FaultStatus {
        FaultStatus::from_dfsc(self.dfsc())
    }

    /// `WnR` (`ISS[6]`): true if a **write** caused the abort, false for a
    /// read. Defined for *data* aborts only — on instruction aborts (which
    /// are always fetches) this bit belongs to an unrelated field, so the
    /// caller must gate on the exception class first.
    pub const fn is_write(self) -> bool {
        (self.0 >> 6) & 1 != 0
    }

    /// Whether `FAR_EL1` holds a valid faulting address for this abort.
    /// Decoded from `FnV` (`ISS[10]`): the architecture sets `FnV = 1` when
    /// the faulting VA is unknown (some external aborts), in which case
    /// `FAR_EL1` is UNKNOWN and must not be trusted. For the common
    /// translation / permission / access-flag faults `FnV` is always 0.
    pub const fn far_valid(self) -> bool {
        (self.0 >> 10) & 1 == 0
    }
}

/// The 6-bit fault status code (`DFSC`/`IFSC`) extracted from an [`Esr`].
///
/// A thin newtype over the raw `u8` so a fault code can't be silently mixed
/// with other small integers; decode it into a [`FaultStatus`] via
/// [`Esr::fault_status`] (or [`FaultStatus::from_dfsc`]).
#[derive(Clone, Copy, Debug)]
pub struct Dfsc(u8);

impl Dfsc {
    /// Wrap a raw 6-bit fault status code.
    pub const fn new(raw: u8) -> Self {
        Self(raw)
    }

    /// The underlying 6-bit code.
    pub fn raw(&self) -> u8 {
        self.0
    }
}

/*
 *
 *  Formal verification (Kani model-checking harnesses)
 *
 *  Compiled only under `cargo kani`. The register decoders are pure bit
 *  extraction over an arbitrary raw word — CBMC covers the whole 64-bit space.
 *
 */

#[cfg(kani)]
mod verification {
    use super::*;

    /// The `EC` / `IL` / `ISS` accessors tile the low 32 bits of `ESR_EL1`
    /// exactly — no gap, no overlap. Guards the shift/mask constants.
    #[kani::proof]
    fn esr_fields_tile_low_word() {
        let raw: u64 = kani::any();
        let esr = Esr::from_raw(raw);

        let ec = esr.ec_raw() as u64;
        let il = esr.il() as u64;
        let iss = esr.iss() as u64;

        assert!(ec < 64 && iss < (1 << 25));
        let reassembled = (ec << 26) | (il << 25) | iss;
        assert!(reassembled == (raw & 0xFFFF_FFFF));
    }

    /// The abort-syndrome accessors read their architected `ISS` sub-fields:
    /// `DFSC = ISS[5:0]`, `WnR = ISS[6]`, `FnV = ISS[10]` (so `far_valid` is
    /// its inverse). These are the bits the page-fault path branches on, so
    /// pin their positions independently of the tiling proof above.
    #[kani::proof]
    fn esr_abort_syndrome_bits() {
        let raw: u64 = kani::any();
        let esr = Esr::from_raw(raw);

        assert!(esr.dfsc().raw() as u64 == (raw & 0x3F)); // DFSC = ISS[5:0]
        assert!(esr.is_write() == ((raw >> 6) & 1 == 1)); // WnR  = ISS[6]
        assert!(esr.far_valid() == ((raw >> 10) & 1 == 0)); // far_valid = !FnV
    }
}
