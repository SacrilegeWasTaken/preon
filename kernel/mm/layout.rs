use crate::frame::{PhysAddr, VirtAddr};

/*
*
*   Kernel address-space layout
*
*/

pub const KERNEL_LINEAR_BASE: usize = 0xFFFF_8000_0000_0000;
pub const KERNEL_IMAGE_BASE: usize = 0xFFFF_8000_0000_0000;
pub const PHYS_LOAD_BASE: usize = 0x40080000;

#[inline]
pub const fn pa_to_linear_va(pa: PhysAddr) -> VirtAddr {
    VirtAddr::new(pa.as_usize() + KERNEL_LINEAR_BASE)
}

#[inline]
pub const fn linear_va_to_pa(va: VirtAddr) -> PhysAddr {
    PhysAddr::new(va.as_usize() - KERNEL_LINEAR_BASE)
}

#[inline]
pub const fn image_va_to_pa(va: VirtAddr) -> PhysAddr {
    PhysAddr::new(va.as_usize() - KERNEL_IMAGE_BASE + PHYS_LOAD_BASE)
}
