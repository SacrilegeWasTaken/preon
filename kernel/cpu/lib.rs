//! `kernel_cpu` — SMP bring-up, the PSCI client, and per-CPU identity types.
#![no_std]

use core::arch::global_asm;

pub(crate) mod psci;
pub(crate) mod smp;
pub(crate) mod types;

pub use psci::{Psci, PsciError};
pub use smp::{BringUpError, CpuData, STACK_SIZE, SecondaryBootData, Smp};
pub use types::{CpuId, Mpidr};

global_asm!(include_str!("asm/secondary.s"));
