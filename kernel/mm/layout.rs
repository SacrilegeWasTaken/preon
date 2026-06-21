use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use crate::frame::{PhysAddr, VirtAddr};

/// Kernel
pub const KERNEL_VA_BASE: usize = 0xFFFF_8000_0000_0000;
static KERNEL_VA_OFFSET: AtomicUsize = AtomicUsize::new(0);

pub fn init(kernel_pa_base: usize) {
    debug_assert!(kernel_pa_base < KERNEL_VA_BASE);
    if KERNEL_VA_OFFSET
        .compare_exchange(0, KERNEL_VA_BASE - kernel_pa_base, Release, Relaxed)
        .is_err()
    {
        panic!("layout::init called twice.")
    }
}

pub fn pa_to_kernel_va(pa: PhysAddr) -> VirtAddr {
    debug_assert!(pa.as_usize() < KERNEL_VA_BASE);
    VirtAddr::new(pa.as_usize() + KERNEL_VA_OFFSET.load(Acquire))
}

pub fn kernel_va_to_pa(va: VirtAddr) -> PhysAddr {
    debug_assert!(va.as_usize() >= KERNEL_VA_BASE);
    PhysAddr::new(va.as_usize() - KERNEL_VA_OFFSET.load(Acquire))
}
