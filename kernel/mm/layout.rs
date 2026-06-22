use crate::frame::{PhysAddr, VirtAddr};

/*
*
*   Kernel address-space layout
*
*/

pub const KERNEL_LINEAR_BASE: usize = 0xFFFF_8000_0000_0000;
pub const KERNEL_IMAGE_BASE: usize = 0xFFFF_FFFF_8000_0000;
pub const PHYS_LOAD_BASE: usize = 0x4008_0000;
pub const IMAGE_PA_BASE: usize = 0x4000_0000;
pub const IMAGE_TEXT_PA: usize = 0x4010_0000;

#[inline]
pub const fn pa_to_linear_va(pa: PhysAddr) -> VirtAddr {
    debug_assert!(pa.as_usize() < KERNEL_LINEAR_BASE);
    VirtAddr::new(pa.as_usize() + KERNEL_LINEAR_BASE)
}

#[inline]
pub const fn linear_va_to_pa(va: VirtAddr) -> PhysAddr {
    debug_assert!(KERNEL_LINEAR_BASE <= va.as_usize() && va.as_usize() < KERNEL_IMAGE_BASE);
    PhysAddr::new(va.as_usize() - KERNEL_LINEAR_BASE)
}

#[inline]
pub const fn image_va_to_pa(va: VirtAddr) -> PhysAddr {
    debug_assert!(va.as_usize() >= KERNEL_IMAGE_BASE);
    PhysAddr::new(va.as_usize() - KERNEL_IMAGE_BASE + IMAGE_PA_BASE)
}
