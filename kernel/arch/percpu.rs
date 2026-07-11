//! Per-CPU variables, Linux-style.
//!
//! Every CPU owns a private copy of the `.percpu` template section. A CPU's
//! copy lives in its [`AREAS`] slot; `TPIDR_EL1` holds that slot's
//! *offset* from the template base (never a pointer), so the one register
//! serves the SMP control block and every other per-CPU static alike.
//!
//! To reach a per-CPU static, add a CPU's offset to the variable's address in
//! the template. [`this_cpu_ptr!`] does this for the current CPU (offset read
//! from `TPIDR_EL1`); [`per_cpu_ptr!`] does it for an arbitrary CPU (offset
//! from [`OFFSETS`]) — needed to fill a CPU's data *before* it installs
//! its own `TPIDR_EL1`.

use crate::{MAX_CPUS, read_sysreg, write_sysreg};

/// Read the calling CPU's per-CPU area offset from `TPIDR_EL1`.
pub fn this_cpu_offset() -> usize {
    read_sysreg!(tpidr_el1) as usize
}

/// # Safety
/// The `offset` must be a valid per-CPU offset for the calling CPU.
pub unsafe fn set_this_cpu_offset(offset: usize) {
    write_sysreg!(tpidr_el1, offset as u64);
}

/// Pointer to CPU `$cpu`'s copy of the `.percpu` static `$var`.
///
/// Uses [`offset_of_cpu`], so it is valid for any CPU — including one that has
/// not yet installed its own `TPIDR_EL1`. That is what lets the primary fill a
/// secondary's control block before waking it.
#[macro_export]
macro_rules! per_cpu_ptr {
    ($cpu:expr,$var:path) => {
        ($crate::percpu::offset_of_cpu($cpu) + (&raw const $var as usize)) as *mut _
    };
}

/// Pointer to the calling CPU's copy of the `.percpu` static `$var`.
///
/// Uses [`this_cpu_offset`] (`TPIDR_EL1`), so it is only correct once the
/// current CPU has run [`init`] (primary) or installed its offset (secondary).
#[macro_export]
macro_rules! this_cpu_ptr {
    ($var:path) => {
        ($crate::percpu::this_cpu_offset() + (&raw const $var as usize)) as *mut _
    };
}

use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicUsize, Ordering},
};

/// Each CPU's per-CPU area offset, indexed by CPU id. Published by [`init`].
static OFFSETS: [AtomicUsize; MAX_CPUS] = [const { AtomicUsize::new(0) }; MAX_CPUS];

// Bounds of the `.percpu` template section, emitted by the linker script.
unsafe extern "C" {
    static __percpu_start: u8;
    static __percpu_end: u8;
}

/// Size of one per-CPU area. Must be at least the `.percpu` template size;
/// [`init`] asserts this.
const PERCPU_MAX: usize = 4096;

/// One CPU's per-CPU area. Wrapped in `UnsafeCell` because per-CPU stores go
/// through raw pointers derived from an immutable `static`, which would
/// otherwise be undefined behaviour.
#[repr(C)]
struct Area(UnsafeCell<[u8; PERCPU_MAX]>);
unsafe impl Sync for Area {}
/// Backing storage for every CPU's per-CPU area.
static AREAS: [Area; MAX_CPUS] = [const { Area(UnsafeCell::new([0; PERCPU_MAX])) }; MAX_CPUS];

/// Compute every CPU's area offset and install the primary's in `TPIDR_EL1`.
pub fn init() {
    let start = &raw const __percpu_start as usize;
    let size = (&raw const __percpu_end as usize) - start;
    assert!(size <= PERCPU_MAX, "per-CPU template exceeds area size");
    for cpu in 0..MAX_CPUS {
        let area = AREAS[cpu].0.get() as usize;
        // template is NOLOAD (zero) → area already zero, but be explicit
        OFFSETS[cpu].store(area - start, Ordering::Relaxed);
    }
    unsafe { set_this_cpu_offset(OFFSETS[0].load(Ordering::Relaxed)) }; // primary = CPU 0
}

/// The per-CPU area offset for `cpu`, valid after [`init`]. Backs
/// [`per_cpu_ptr!`].
pub fn offset_of_cpu(cpu: usize) -> usize {
    OFFSETS[cpu].load(Ordering::Relaxed)
}
