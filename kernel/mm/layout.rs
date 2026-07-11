//! Kernel address-space layout — the linear map, the image region, and the
//! physical ↔ virtual conversions built on the `image_va_base` seam.

use kernel_arch::mm::{PhysAddr, VirtAddr};

/// Base of the linear map: every physical address is visible at
/// `pa + KERNEL_LINEAR_BASE` as Normal cacheable RW+NX memory.
pub const KERNEL_LINEAR_BASE: usize = 0xFFFF_8000_0000_0000;
/// Base of the kernel image region (`.text`/`.rodata`/`.data`/`.bss`/stack),
/// mapped page-by-page with per-section permissions. Under KASLR this is the
/// *nominal* base; the live base is [`image_va_base`].
pub const KERNEL_IMAGE_BASE: usize = 0xFFFF_FFFF_8000_0000;
/// Base of the device (MMIO) region, Device-nGnRE RW+NX. Intentionally a
/// fixed VA and **never** subject to a KASLR slide — the early debug console
/// must resolve deterministically before any randomization is computed
/// (mirrors the role of Linux's fixmap earlycon).
pub const KERNEL_DEVICE_BASE: usize = 0xFFFF_C000_0000_0000;
/// Physical load address of the kernel image (start of `.text`).
pub const PHYS_LOAD_BASE: usize = 0x4008_0000;
/// Physical base of the 1 GiB region the image lives in (image VA 0 maps here).
pub const IMAGE_PA_BASE: usize = 0x4000_0000;
/// Physical address of `.text` within the image region.
pub const IMAGE_TEXT_PA: usize = 0x4010_0000;

/// Map a physical address into the linear region. Offset-based and slide-free:
/// the linear window is not randomized.
#[inline]
pub const fn pa_to_linear_va(pa: PhysAddr) -> VirtAddr {
    debug_assert!(pa.as_usize() < KERNEL_LINEAR_BASE);
    VirtAddr::new(pa.as_usize() + KERNEL_LINEAR_BASE)
}

/// Inverse of [`pa_to_linear_va`]. The upper bound is [`KERNEL_DEVICE_BASE`]
/// (not the image base) so a device VA can never be misread as linear.
#[inline]
pub const fn linear_va_to_pa(va: VirtAddr) -> PhysAddr {
    debug_assert!(KERNEL_LINEAR_BASE <= va.as_usize() && va.as_usize() < KERNEL_DEVICE_BASE);
    PhysAddr::new(va.as_usize() - KERNEL_LINEAR_BASE)
}

/// Recover the physical address backing an image VA. Routed through
/// [`image_va_base`] (not the raw const) and deliberately **not** `const fn`,
/// so a runtime KASLR slide can be folded into the base without touching any
/// caller. Symbol-derived VAs (e.g. `&__text_start`) already slide with the
/// image under PIE relocation; this keeps the PA recovery consistent.
#[inline]
pub fn image_va_to_pa(va: VirtAddr) -> PhysAddr {
    debug_assert!(va.as_usize() >= image_va_base());
    PhysAddr::new(va.as_usize() - image_va_base() + IMAGE_PA_BASE)
}

/// Map a physical MMIO address into the device region. `const fn` because the
/// device base is fixed (see [`KERNEL_DEVICE_BASE`]).
#[inline]
pub const fn pa_to_device_va(pa: PhysAddr) -> VirtAddr {
    debug_assert!(pa.as_usize() < KERNEL_DEVICE_BASE);
    VirtAddr::new(pa.as_usize() + KERNEL_DEVICE_BASE)
}

/// Single source of truth for the live image base — the one place a future
/// KASLR slide is added (`KERNEL_IMAGE_BASE + slide`). Today a no-op that the
/// inliner folds to the constant, so the indirection costs nothing.
#[inline]
fn image_va_base() -> usize {
    KERNEL_IMAGE_BASE
}

/*
 *
 *  Formal verification (Kani model-checking harnesses)
 *
 *  Compiled only under `cargo kani`. Pure address arithmetic — round-trips
 *  and window-containment for the linear / image / device maps. Invisible to
 *  a normal kernel build because the whole module is `#[cfg(kani)]`.
 *
 */

#[cfg(kani)]
mod verification {
    use super::*;

    /// Span of the linear window: `[LINEAR_BASE, DEVICE_BASE)`. A PA below
    /// this maps into the linear region without reaching the device base.
    const LINEAR_SPAN: usize = KERNEL_DEVICE_BASE - KERNEL_LINEAR_BASE;

    /// `linear_va_to_pa ∘ pa_to_linear_va` is the identity, and the forward
    /// image lands strictly inside the linear window — never aliasing device.
    #[kani::proof]
    fn linear_round_trip() {
        let raw: usize = kani::any();
        kani::assume(raw < LINEAR_SPAN);

        let va = pa_to_linear_va(PhysAddr::new(raw));

        assert!(va.as_usize() >= KERNEL_LINEAR_BASE);
        assert!(va.as_usize() < KERNEL_DEVICE_BASE);
        assert!(linear_va_to_pa(va).as_usize() == raw);
    }

    /// A device VA lands inside `[DEVICE_BASE, IMAGE_BASE)` for any PA that
    /// fits one L0 slot, so an MMIO VA can never be read back as linear.
    #[kani::proof]
    fn device_map_in_window() {
        let raw: usize = kani::any();
        // One L0 slot spans DEVICE_BASE .. DEVICE_BASE + 2^39 (512 GiB).
        kani::assume(raw < (1usize << 39));

        let va = pa_to_device_va(PhysAddr::new(raw));
        assert!(va.as_usize() >= KERNEL_DEVICE_BASE);
        assert!(va.as_usize() < KERNEL_IMAGE_BASE);
    }

    /// `image_va_to_pa` never underflows and stays at/above the image PA base
    /// for any VA at or above the image base.
    #[kani::proof]
    fn image_map_no_underflow() {
        let raw: usize = kani::any();
        kani::assume(raw >= KERNEL_IMAGE_BASE);

        let pa = image_va_to_pa(VirtAddr::new(raw));
        assert!(pa.as_usize() >= IMAGE_PA_BASE);
    }
}
