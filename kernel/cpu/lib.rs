#![no_std]

use kernel_builtin::wfe_loop;

pub mod psci;

/// Cache-line aligned
#[repr(C, align(64))]
pub struct CpuData {
    pub cpu_id: u32,
    pub mpidr: u64,
    pub stack_top: usize,
}

#[repr(C)]
pub struct SecondaryBootData {
    pub cpu_data_ptr: *const CpuData,
    pub stack_top: usize,
}

pub fn install_current_cpu_local(ptr: *const CpuData) {
    todo!()
}

pub fn current_cpu() -> &'static CpuData {
    todo!()
}

pub fn secondary_cpu_main(boot_data: &SecondaryBootData) -> ! {
    wfe_loop!()
}
