//! `kernel_cpu` — SMP bring-up, the PSCI client, and per-CPU identity types.
#![no_std]

#[cfg(not(kani))]
use core::arch::global_asm;

#[cfg(not(kani))]
pub(crate) mod psci;
#[cfg(not(kani))]
pub(crate) mod smp;
pub(crate) mod types;

#[cfg(not(kani))]
pub use psci::{Psci, PsciError};
#[cfg(not(kani))]
pub use smp::{BringUpError, CpuData, STACK_SIZE, SecondaryBootData, Smp};
pub use types::{CpuId, Mpidr};

#[cfg(not(kani))]
global_asm!(include_str!("asm/secondary.s"));
