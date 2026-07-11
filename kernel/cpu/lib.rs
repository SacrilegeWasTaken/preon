#![no_std]

use core::arch::global_asm;

pub mod psci;
pub mod smp;
pub mod types;

pub use smp::{BringUpError, CpuData, STACK_SIZE, SecondaryBootData, Smp};
pub use types::{CpuId, Mpidr};

global_asm!(include_str!("asm/secondary.s"));
