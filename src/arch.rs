#[macro_export]
macro_rules! read_sysreg {
    ($name:ident) => {{
        let val: u64;
        unsafe {
            core::arch::asm!(
                concat!("mrs {}, ", stringify!($name)),
                out(reg) val,
                options(nomem, nostack, preserves_flags)
            );
        }
        val
    }};
}

#[macro_export]
macro_rules! write_sysreg {
    ($name:ident, $val:expr) => {{
        let v: u64 = $val;
        unsafe {
            core::arch::asm!(
                concat!("mrs", stringify!($name), ", {}"),
                in(reg) val,
                options(nomem, nostack)
            );
        }
    }};
}

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
    pub fn from_esr(esr: u64) -> Self {
        let ec = ((esr >> 26) & 0x3f) as u8;
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
}

impl ExceptionClass {
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
