use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};

use kernel_builtin::{kernel_uart_log, wfe_loop};

use crate::psci::{Psci, PsciError};
use crate::types::{CpuId, Mpidr};

pub const MAX_CPUS: usize = 16;
pub const STACK_SIZE: usize = 64 * 1024;

/// Per-CPU control block reached via `TPIDR_EL1`.
///
/// Layout matches what the assembler trampoline expects when reading
/// the pointer published through `install_current_cpu_local`.
#[repr(C, align(64))]
pub struct CpuData {
    pub cpu_id: u32,
    pub mpidr: u64,
    pub stack_top: usize,
}

/// Data block handed to a secondary CPU through the PSCI `ctx` argument.
#[repr(C)]
pub struct SecondaryBootData {
    pub cpu_data_ptr: *const CpuData,
    pub stack_top: usize,
}

#[repr(C, align(16))]
struct CpuDataCell(UnsafeCell<CpuData>);
unsafe impl Sync for CpuDataCell {}

#[repr(C, align(16))]
struct CpuStack(UnsafeCell<[u8; STACK_SIZE]>);
unsafe impl Sync for CpuStack {}

#[repr(C, align(16))]
struct BootDataCell(UnsafeCell<SecondaryBootData>);
unsafe impl Sync for BootDataCell {}

static CPU_ONLINE: AtomicU64 = AtomicU64::new(0);

static CPU_DATA: [CpuDataCell; MAX_CPUS] = [const {
    CpuDataCell(UnsafeCell::new(CpuData {
        cpu_id: 0,
        mpidr: 0,
        stack_top: 0,
    }))
}; MAX_CPUS];

static SECONDARY_STACKS: [CpuStack; MAX_CPUS] =
    [const { CpuStack(UnsafeCell::new([0u8; STACK_SIZE])) }; MAX_CPUS];

static BOOT_DATA: [BootDataCell; MAX_CPUS] = [const {
    BootDataCell(UnsafeCell::new(SecondaryBootData {
        cpu_data_ptr: core::ptr::null(),
        stack_top: 0,
    }))
}; MAX_CPUS];

unsafe extern "C" {
    fn secondary_entry();
}

/// Errors that can stop SMP bring-up.
#[derive(Debug)]
pub enum BringUpError {
    /// The firmware reports more CPUs than `MAX_CPUS` was built for.
    TooManyCpus,
    /// PSCI rejected `CPU_ON` for a particular CPU.
    Psci(Mpidr, PsciError),
}

/// Typed facade over the kernel's SMP state.
///
/// All state lives in module statics; this is a unit struct exposing a
/// type-bound API. The primary CPU drives every method; secondaries
/// observe results through `Smp::wait_for`.
pub struct Smp;

impl Smp {
    /// Mark `cpu` as online with `Release` ordering, publishing any prior
    /// writes that should be visible to observers of `wait_for`.
    pub fn mark_online(cpu: CpuId) {
        CPU_ONLINE.fetch_or(1u64 << cpu.raw(), Ordering::Release);
    }

    pub fn is_online(cpu: CpuId) -> bool {
        CPU_ONLINE.load(Ordering::Acquire) & (1u64 << cpu.raw()) != 0
    }

    /// Spin until `cpu` calls `mark_online`.
    pub fn wait_for(cpu: CpuId) {
        while !Self::is_online(cpu) {
            core::hint::spin_loop();
        }
    }

    /// Populate `CPU_DATA[cpu]` so that the corresponding CPU sees a valid
    /// control block once `TPIDR_EL1` is set.
    pub fn init_cpu(cpu: CpuId, mpidr: Mpidr) {
        let ptr = CPU_DATA[cpu.as_usize()].0.get();
        unsafe {
            (*ptr).cpu_id = cpu.raw();
            (*ptr).mpidr = mpidr.raw();
            (*ptr).stack_top = Self::stack_top(cpu);
        }
    }

    /// Top of the stack reserved for `cpu`. Stacks grow down, so this is
    /// the address loaded into `SP` when the CPU first runs.
    pub fn stack_top(cpu: CpuId) -> usize {
        let base = SECONDARY_STACKS[cpu.as_usize()].0.get() as usize;
        base + STACK_SIZE
    }

    /// Pointer to `cpu`'s control block. Safe to publish to `TPIDR_EL1`
    /// once `init_cpu` has run for the same `cpu`.
    pub fn cpu_data(cpu: CpuId) -> *const CpuData {
        CPU_DATA[cpu.as_usize()].0.get()
    }

    /// Fill the static `BOOT_DATA` slot for `cpu` and return a pointer
    /// suitable for PSCI's `ctx` argument.
    pub fn prepare_boot_data(cpu: CpuId) -> *const SecondaryBootData {
        let ptr = BOOT_DATA[cpu.as_usize()].0.get();
        unsafe {
            (*ptr).cpu_data_ptr = Self::cpu_data(cpu);
            (*ptr).stack_top = Self::stack_top(cpu);
        }
        ptr as *const _
    }

    /// Physical address of the assembler trampoline secondaries land in.
    pub fn entry_addr() -> u64 {
        secondary_entry as *const () as u64
    }

    /// Bring up every secondary CPU listed in the device tree. The primary
    /// installs its own control block first, then walks the `cpus` node
    /// and issues `CPU_ON` for each non-primary entry.
    ///
    /// Returns once every requested CPU has reached `secondary_cpu_main`
    /// and called `mark_online`.
    pub fn bring_up_all(fdt: &fdt::Fdt, psci: &Psci) -> Result<(), BringUpError> {
        let primary_mpidr = Mpidr::current();

        Self::init_cpu(CpuId::PRIMARY, primary_mpidr);
        install_current_cpu_local(Self::cpu_data(CpuId::PRIMARY));
        Self::mark_online(CpuId::PRIMARY);

        let mut next_idx: u32 = 1;
        for node in fdt.cpus() {
            let mpidr = Mpidr::new(node.ids().first() as u64);
            if mpidr == primary_mpidr {
                continue;
            }
            if next_idx as usize >= MAX_CPUS {
                return Err(BringUpError::TooManyCpus);
            }

            let cpu = CpuId::new(next_idx);
            Self::init_cpu(cpu, mpidr);
            let ctx = Self::prepare_boot_data(cpu) as u64;

            // Make the writes above visible to the secondary before it
            // ever loads from BOOT_DATA / CPU_DATA.
            core::sync::atomic::fence(Ordering::Release);

            psci.cpu_on(mpidr, Self::entry_addr(), ctx)
                .map_err(|e| BringUpError::Psci(mpidr, e))?;

            Self::wait_for(cpu);
            next_idx += 1;
        }

        Ok(())
    }
}

/// Install the per-CPU pointer in `TPIDR_EL1`.
///
/// Called from `secondary.asm` on every secondary right after its stack
/// is set, and from `Smp::bring_up_all` for the primary.
#[unsafe(no_mangle)]
pub extern "C" fn install_current_cpu_local(ptr: *const CpuData) {
    unsafe {
        core::arch::asm!(
            "msr tpidr_el1, {0}",
            "isb",
            in(reg) ptr as u64,
            options(nomem, nostack, preserves_flags),
        );
    }
}

/// Read the current CPU's control block via `TPIDR_EL1`.
///
/// # Safety
/// The caller asserts that `install_current_cpu_local` has already run
/// on this CPU. Otherwise `TPIDR_EL1` points to garbage.
pub fn current_cpu() -> &'static CpuData {
    let ptr: *const CpuData;
    unsafe {
        core::arch::asm!(
            "mrs {0}, tpidr_el1",
            out(reg) ptr,
            options(nomem, nostack, preserves_flags),
        );
        &*ptr
    }
}

/// Entry point for secondary CPUs. Called by `secondary.asm` after it has
/// enabled FP/SIMD, set `VBAR_EL1`, switched to the per-CPU stack, and
/// installed the per-CPU pointer.
#[unsafe(no_mangle)]
pub extern "C" fn secondary_cpu_main(_boot_data: &SecondaryBootData) -> ! {
    let cpu = current_cpu();
    kernel_uart_log!("CPU {} online (mpidr={:#x})", cpu.cpu_id, cpu.mpidr);
    Smp::mark_online(CpuId::new(cpu.cpu_id));
    wfe_loop!()
}
