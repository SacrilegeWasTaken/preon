//! `kernel_mm` — physical and virtual memory: the allocator stack (bootmem,
//! buddy, slab), page tables, and the kernel address-space layout.
#![no_std]

pub mod attrs;
pub mod bootmem;
pub mod buddy;
pub mod frame;
pub mod kernel_map;
pub mod layout;
pub mod mmu;
pub mod page_table;
pub mod ram;
pub(crate) mod slab;
pub mod tcr;
pub mod types;

#[unsafe(link_section = ".boot.bss")]
#[unsafe(no_mangle)]
static _BOOT_BSS_ANCHOR: u8 = 0;
