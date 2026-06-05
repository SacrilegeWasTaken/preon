#![no_std]

/// Read a system register by name, e.g. `read_sysreg!(esr_el1)`.
///
/// Expands to a 64-bit `mrs` returning the value as `u64`.
#[macro_export]
macro_rules! read_sysreg {
    ($name:ident) => {{
        let val: u64;
        unsafe {
            core::arch::asm!(
                concat!("mrs {}, ", stringify!($name)),
                out(reg) val,
                options(nomem, nostack, preserves_flags),
            );
        }
        val
    }};
}

/// Write a 64-bit value to a system register by name.
///
/// e.g. `write_sysreg!(vbar_el1, addr)`.
#[macro_export]
macro_rules! write_sysreg {
    ($name:ident, $val:expr) => {{
        let v: u64 = $val;
        unsafe {
            core::arch::asm!(
                concat!("msr ", stringify!($name), ", {}"),
                in(reg) v,
                options(nomem, nostack),
            );
        }
    }};
}

/// Exception Syndrome Register (`ESR_EL1`) value.
///
/// Bit layout: `EC[31:26]`, `IL[25]`, `ISS[24:0]`. Helpers below
/// decode the standard sub-fields without exposing the raw integer
/// arithmetic to call sites.
#[derive(Debug, Copy, Clone)]
pub struct Esr(u64);

impl Esr {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let v = read_sysreg!(esr_el1);
        Self(v)
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
