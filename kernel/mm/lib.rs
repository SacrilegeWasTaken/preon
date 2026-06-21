#![no_std]

pub mod attrs;
pub mod frame;
pub mod layout;
pub mod page_table;
pub mod tcr;
pub mod types;

#[unsafe(link_section = ".boot.bss")]
#[unsafe(no_mangle)]
static _BOOT_BSS_ANCHOR: u8 = 0;
