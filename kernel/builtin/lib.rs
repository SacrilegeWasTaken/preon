#![no_std]

pub mod sync;
pub mod uart;

#[macro_export]
macro_rules! wfe_loop {
    () => {
        unsafe {
            loop {
                core::arch::asm!("wfe");
            }
        }
    };
}
