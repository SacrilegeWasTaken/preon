//! RAM discovery — iterate the `memory` nodes of the flattened device tree.

use fdt::Fdt;
use kernel_arch::mm::PhysAddr;

pub struct RamRegion {
    pub base: PhysAddr,
    pub size: usize,
}

pub fn for_each_region<F>(fdt: &Fdt, mut f: F)
where
    F: FnMut(RamRegion),
{
    for r in fdt.memory().regions() {
        if let Some(size) = r.size {
            f(RamRegion {
                base: PhysAddr::new(r.starting_address as usize),
                size,
            });
        }
    }
}
