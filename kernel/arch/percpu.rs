use crate::{MAX_CPUS, read_sysreg, write_sysreg};

pub fn this_cpu_offset() -> usize {
    read_sysreg!(tpidr_el1) as usize
}

/// # Safety
/// The `offset` must be a valid per-CPU offset for the calling CPU.
pub unsafe fn set_this_cpu_offset(offset: usize) {
    write_sysreg!(tpidr_el1, offset as u64);
}

#[macro_export]
macro_rules! this_cpu_ptr {
    ($var:path) => {
        ($crate::percpu::this_cpu_offset() + (&raw const $var as usize)) as *mut _
    };
}
use core::sync::atomic::{AtomicUsize, Ordering};

static OFFSETS: [AtomicUsize; MAX_CPUS] = [const { AtomicUsize::new(0) }; MAX_CPUS];

unsafe extern "C" {
    static __percpu_start: u8;
    static __percpu_end: u8;
}

const PERCPU_MAX: usize = 4096;

#[repr(C)]
struct Area([u8; PERCPU_MAX]);
static AREAS: [Area; MAX_CPUS] = [const { Area([0; PERCPU_MAX]) }; MAX_CPUS];
pub fn init() {
    let start = &raw const __percpu_start as usize;
    let size = (&raw const __percpu_end as usize) - start;
    assert!(size <= PERCPU_MAX, "per-CPU template exceeds area size");
    for cpu in 0..MAX_CPUS {
        let area = &raw const AREAS[cpu] as usize;
        // template is NOLOAD (zero) → area already zero, but be explicit
        OFFSETS[cpu].store(area - start, Ordering::Relaxed);
    }
    unsafe { set_this_cpu_offset(OFFSETS[0].load(Ordering::Relaxed)) }; // primary = CPU 0
}
