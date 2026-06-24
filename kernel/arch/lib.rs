#![no_std]
//! Architecture primitives for AArch64 — the ISA expressed as Rust types,
//! with no kernel policy attached.
//!
//! - [`reg`] — system-register snapshots and decoders ([`reg::Esr`],
//!   [`reg::Spsr`], …) plus the `read_sysreg!`/`write_sysreg!` macros.
//! - [`mm`] — the address vocabulary ([`mm::PhysAddr`], [`mm::VirtAddr`])
//!   and the translation [`mm::Level`].
//! - [`exceptions`] — exception-class and fault-status decoding.
//!
//! Everything here answers "what does the hardware say"; deciding what to do
//! about it belongs in the higher service crates.

pub mod exceptions;
pub mod mm;
pub mod reg;

/// Cache line size used for padding to avoid false sharing.
/// 128 covers Apple Silicon (Firestorm/Avalanche, 128-byte L1 line) as
/// well as classic 64-byte ARM. Costs an extra cache line of padding
/// per element on 64-byte systems — negligible at our scales
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
pub const CACHELINE_SIZE: usize = 128;
#[cfg(target_arch = "riscv64")]
pub const CACHELINE_SIZE: usize = 64;

/// Read a system register by name, e.g. `read_sysreg!(esr_el1)`.
///
/// Expands to a 64-bit `mrs` returning the value as `u64`.
#[macro_export]
macro_rules! read_sysreg {
    ($name:ident) => {{
        let val: u64;
        unsafe {
            core::arch::asm!(
                concat!("mrs {}, ", stringify!($name)),
                out(reg) val,
                options(nomem, nostack, preserves_flags),
            );
        }
        val
    }};
}

/// Write a 64-bit value to a system register by name.
///
/// e.g. `write_sysreg!(vbar_el1, addr)`.
#[macro_export]
macro_rules! write_sysreg {
    ($name:ident, $val:expr) => {{
        let v: u64 = $val;
        unsafe {
            core::arch::asm!(
                concat!("msr ", stringify!($name), ", {}"),
                in(reg) v,
                options(nomem, nostack),
            );
        }
    }};
}

/// Single `isb` instruction flushing whole CPU pipeline
/// Making all (not really all) changes visible to next instructions
#[macro_export]
macro_rules! flush_cpu_pipeline {
    () => {{
        unsafe {
            core::arch::asm!("isb", options(nostack, preserves_flags));
        }
    }};
}

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
