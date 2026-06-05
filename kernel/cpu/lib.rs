#![no_std]

use core::arch::global_asm;

pub mod psci;
pub mod smp;
pub mod types;

pub use smp::{
    current_cpu, install_current_cpu_local, secondary_cpu_main, BringUpError, CpuData,
    SecondaryBootData, Smp, MAX_CPUS, STACK_SIZE,
};
pub use types::{CpuId, Mpidr};

global_asm!(include_str!("asm/secondary.asm"));
