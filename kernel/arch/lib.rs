#![no_std]

/// Physical address. A thin wrapper around `usize` to keep physical and
/// virtual addresses from being mixed up at call sites.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PhysAddr(usize);

impl PhysAddr {
    pub const fn new(raw: usize) -> Self {
        Self(raw)
    }

    pub const fn as_usize(self) -> usize {
        self.0
    }

    pub const fn as_u64(self) -> u64 {
        self.0 as u64
    }
}

impl core::fmt::LowerHex for PhysAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::LowerHex::fmt(&self.0, f)
    }
}

impl core::fmt::UpperHex for PhysAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::UpperHex::fmt(&self.0, f)
    }
}

/// Virtual address. Paired with [`PhysAddr`]; the newtype lets functions
/// declare which kind of address they want and rejects the other at
/// compile time.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VirtAddr(usize);

impl VirtAddr {
    pub const fn new(raw: usize) -> Self {
        Self(raw)
    }

    pub const fn as_usize(self) -> usize {
        self.0
    }

    pub const fn as_u64(self) -> u64 {
        self.0 as u64
    }
}

impl core::fmt::LowerHex for VirtAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::LowerHex::fmt(&self.0, f)
    }
}

impl core::fmt::UpperHex for VirtAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::UpperHex::fmt(&self.0, f)
    }
}

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

/// Cache line size used for padding to avoid false sharing.
/// 128 covers Apple Silicon (Firestorm/Avalanche, 128-byte L1 line) as
/// well as classic 64-byte ARM. Costs an extra cache line of padding
/// per element on 64-byte systems — negligible at our scales
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
pub const CACHELINE_SIZE: usize = 128;
#[cfg(target_arch = "riscv64")]
pub const CACHELINE_SIZE: usize = 64;

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

/// Single `isb` instruction flushing whole CPU pipeline
/// Making all (not really all) changes visible to next instructions
#[macro_export]
macro_rules! flush_cpu_pipeline {
    () => {{
        unsafe {
            core::arch::asm!("isb", options(nostack, preserves_flags));
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
