use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU64, Ordering};

use kernel_arch::mm::{PhysAddr, VirtAddr};
use kernel_arch::wfe_loop;
use kernel_builtin::kernel_uart_log;
use kernel_mm::buddy::BUDDY;
use kernel_mm::layout::{image_va_to_pa, pa_to_linear_va};

use crate::psci::{Psci, PsciError};
use crate::types::{CpuId, Mpidr};

pub(crate) use kernel_arch::MAX_CPUS;
pub const STACK_SIZE: usize = 64 * 1024;

/// Per-CPU control block reached via `TPIDR_EL1`.
///
/// Layout matches what the assembler trampoline expects when reading
/// the pointer published through `install_current_cpu_local`.
///
/// Align(128) is performance critical. It's a cacheline alignment.
/// When CPU0 is writing `CPU_DATA[0]` -> cache coherency making
/// the whole line CPU0 exclusive, so CPU1 losing it's copy in cache.
/// So cacheline alignment preventing ping-pong effect between CPUs.
/// Locally data is independent. If they are physically sharing once
/// cacheline -> every write operation is a performance hit!
///
/// - Apple M1/M2/M3... has 128 byte cacheline
/// - Cortex-A53/A57/A72/A75/A76/A77/A78/X1 - 64 bytes
///
/// Align(128) is safe across platforms. The memory cost is
/// 2KB in `.bss` section for each 16 CPUs. Nothing.
#[repr(C, align(128))]
pub struct CpuData {
    pub cpu_id: CpuId,
    pub mpidr: Mpidr,
}

/// Data block handed to a secondary CPU through the PSCI `ctx` argument.
#[repr(C)]
pub struct SecondaryBootData {
    pub cpu_data_ptr: *const CpuData,
    pub stack_top: usize,
    pub ttbr1_root: usize,
}

#[repr(C, align(16))]
struct CpuDataCell(UnsafeCell<CpuData>);
unsafe impl Sync for CpuDataCell {}

#[repr(C, align(16))]
struct BootDataCell(UnsafeCell<SecondaryBootData>);
unsafe impl Sync for BootDataCell {}

/// Bitmap. Will be explained later (or deleted)
/// # Deletion reason:
/// The whole SMP setup is very inefficient for 32+
/// cpus because of static and their affect on memory.
/// Using allocator can be a better option so we can
/// support a lot's of CPU cores/sockers/clusters.
static CPU_ONLINE: AtomicU64 = AtomicU64::new(0);

/// # Safety
///
/// Each CPU writing into it's own region.
static CPU_DATA: [CpuDataCell; MAX_CPUS] = [const {
    CpuDataCell(UnsafeCell::new(CpuData {
        cpu_id: CpuId::PRIMARY,
        mpidr: Mpidr::new(0),
    }))
}; MAX_CPUS];

/// # Safety
///
/// Each CPU writing into it's own region
static BOOT_DATA: [BootDataCell; MAX_CPUS] = [const {
    BootDataCell(UnsafeCell::new(SecondaryBootData {
        cpu_data_ptr: core::ptr::null(),
        stack_top: 0,
        ttbr1_root: 0,
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
    OutOfMemory,
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

    /// Read the current CPU's control block via `TPIDR_EL1`.
    pub fn current() -> &'static CpuData {
        current_cpu()
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
            (*ptr).cpu_id = cpu;
            (*ptr).mpidr = mpidr;
        }
    }

    /// Reference to `cpu`'s control block. Valid for the lifetime of the
    /// kernel — `CPU_DATA` lives in `.bss` forever.
    pub fn cpu_data(cpu: CpuId) -> &'static CpuData {
        // Safety: each CpuData slot is written only by `init_cpu` (called
        // by the primary before the corresponding CPU starts running) and
        // read everywhere else. UnsafeCell + 'static lifetime is sound.
        unsafe { &*CPU_DATA[cpu.as_usize()].0.get() }
    }

    /// Publish `cpu`'s control block to `TPIDR_EL1` on the calling CPU.
    pub fn install_current(cpu: CpuId) {
        install_current_cpu_local(Self::cpu_data(cpu));
    }

    /// Fill the static `BOOT_DATA` slot for `cpu` and return a pointer
    /// suitable for PSCI's `ctx` argument.
    pub fn prepare_boot_data(
        cpu: CpuId,
        root: PhysAddr,
        stack_top: usize,
    ) -> *const SecondaryBootData {
        let ptr = BOOT_DATA[cpu.as_usize()].0.get();
        unsafe {
            (*ptr).cpu_data_ptr = Self::cpu_data(cpu) as *const CpuData;
            (*ptr).stack_top = stack_top;
            (*ptr).ttbr1_root = root.as_usize();
        }
        ptr as *const _
    }

    /// Physical address of the assembler trampoline secondaries land in.
    pub fn entry_addr() -> PhysAddr {
        image_va_to_pa(VirtAddr::new(secondary_entry as *const () as usize))
    }

    /// Bring up every secondary CPU listed in the device tree. The primary
    /// installs its own control block first, then walks the `cpus` node
    /// and issues `CPU_ON` for each non-primary entry.
    ///
    /// Returns once every requested CPU has reached `secondary_cpu_main`
    /// and called `mark_online`.
    pub fn bring_up_all(root: PhysAddr, fdt: &fdt::Fdt, psci: &Psci) -> Result<(), BringUpError> {
        let primary_mpidr = Mpidr::current();

        Self::init_cpu(CpuId::PRIMARY, primary_mpidr);
        Self::install_current(CpuId::PRIMARY);
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

            let stack_top = allock_stack().ok_or(BringUpError::OutOfMemory)?;
            let bd_va = Self::prepare_boot_data(cpu, root, stack_top);
            clean_dcache(bd_va as usize, core::mem::size_of::<SecondaryBootData>());
            let ctx = image_va_to_pa(VirtAddr::new(bd_va as usize));
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
/// is set, and from `Smp::install_current` for the primary.
#[unsafe(no_mangle)]
extern "C" fn install_current_cpu_local(ptr: *const CpuData) {
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
/// Prefer [`Smp::current`] in new code; kept as a free function so it
/// can be reached from places that don't pull in the `Smp` facade.
///
/// # Safety
/// The caller asserts that the current CPU went through bring-up so
/// `TPIDR_EL1` points at a valid `CpuData`.
fn current_cpu() -> &'static CpuData {
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
extern "C" fn secondary_cpu_main(_boot_data: &SecondaryBootData) -> ! {
    let cpu = Smp::current();
    kernel_uart_log!(
        "CPU {} online (mpidr={:#x})",
        cpu.cpu_id.raw(),
        cpu.mpidr.raw()
    );
    Smp::mark_online(cpu.cpu_id);
    wfe_loop!()
}

/// Clean [va, va+len) to the Point of Coherency so a secondary reading it with
/// the MMU off (non-cacheable) sees the primary's cacheable writes. :wa
fn clean_dcache(va: usize, len: usize) {
    const LINE: usize = 64; // over-clean on 128-B lines is fine
    let mut p = va & !(LINE - 1);
    while p < va + len {
        unsafe { core::arch::asm!("dc cvac, {}", in(reg) p, options(nostack, preserves_flags)) };
        p += LINE;
    }
    unsafe { core::arch::asm!("dsb ish", options(nostack, preserves_flags)) };
}

const STACK_ORDER: u8 = 4;

fn allock_stack() -> Option<usize> {
    let pa = BUDDY.get()?.lock().alloc_pages(STACK_ORDER)?;
    Some(pa_to_linear_va(pa).as_usize() + STACK_SIZE)
}
