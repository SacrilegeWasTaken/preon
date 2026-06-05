use fdt::Fdt;

use crate::types::Mpidr;

/// PSCI conduit (privileged call instruction) used by the firmware.
#[derive(Debug, Copy, Clone)]
pub enum Method {
    Hvc,
    Smc,
}

/// PSCI return codes from the SMC Calling Convention.
///
/// Numeric mapping matches ARM DEN 0022; unknown codes pass through as
/// `Unknown` so the caller can still log the raw value.
#[derive(Debug, Copy, Clone)]
pub enum PsciError {
    NotSupported,
    InvalidParameters,
    Denied,
    AlreadyOn,
    OnPending,
    InternalFailure,
    NotPresent,
    Disabled,
    InvalidAddress,
    Unknown(i64),
}

impl PsciError {
    fn from_code(code: i64) -> Self {
        match code {
            -1 => Self::NotSupported,
            -2 => Self::InvalidParameters,
            -3 => Self::Denied,
            -4 => Self::AlreadyOn,
            -5 => Self::OnPending,
            -6 => Self::InternalFailure,
            -7 => Self::NotPresent,
            -8 => Self::Disabled,
            -9 => Self::InvalidAddress,
            other => Self::Unknown(other),
        }
    }
}

/// Parsed `/psci` node giving the kernel everything needed to issue calls.
#[derive(Debug)]
pub struct Psci {
    pub method: Method,
    pub cpu_on_id: u32,
}

impl Psci {
    /// Read the PSCI node from the device tree. Returns `None` if the node
    /// is missing or the `method` field is unrecognised.
    pub fn from_fdt(fdt: &Fdt) -> Option<Self> {
        let node = fdt.find_node("/psci")?;

        let method = match node.property("method")?.as_str()? {
            "hvc" => Method::Hvc,
            "smc" => Method::Smc,
            _ => return None,
        };

        let cpu_on_id = node.property("cpu_on")?.as_usize()? as u32;

        Some(Self { method, cpu_on_id })
    }

    /// Wake the CPU identified by `mpidr`. `entry` is the physical address
    /// jumped to on the secondary, `ctx` is passed in `x0`.
    pub fn cpu_on(&self, mpidr: Mpidr, entry: u64, ctx: u64) -> Result<(), PsciError> {
        let code = self.raw_call(self.cpu_on_id, mpidr.raw(), entry, ctx);
        if code == 0 {
            Ok(())
        } else {
            Err(PsciError::from_code(code))
        }
    }

    fn raw_call(&self, fn_id: u32, a1: u64, a2: u64, a3: u64) -> i64 {
        let fn_id = fn_id as u64;
        match self.method {
            Method::Hvc => psci_call!("hvc #0", fn_id, a1, a2, a3),
            Method::Smc => psci_call!("smc #0", fn_id, a1, a2, a3),
        }
    }
}

// SMCCC: x0..x17 may be clobbered by the call; mark them dead afterwards.
macro_rules! psci_call {
    ($insn:literal, $fn_id:expr, $a1:expr, $a2:expr, $a3:expr) => {{
        let result: i64;
        unsafe {
            core::arch::asm!(
                $insn,
                inout("x0") $fn_id => result,
                in("x1") $a1,
                in("x2") $a2,
                in("x3") $a3,
                lateout("x1") _, lateout("x2") _, lateout("x3") _,
                lateout("x4") _, lateout("x5") _, lateout("x6") _,
                lateout("x7") _, lateout("x8") _, lateout("x9") _,
                lateout("x10") _, lateout("x11") _, lateout("x12") _,
                lateout("x13") _, lateout("x14") _, lateout("x15") _,
                lateout("x16") _, lateout("x17") _,
                options(nostack),
            );
        }
        result
    }};
}

pub(crate) use psci_call;
