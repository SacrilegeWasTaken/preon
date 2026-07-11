//! `kernel_builtin` — freestanding primitives shared across the kernel: the
//! UART, spin locks and one-time init, and typed MMIO helpers.
#![no_std]

pub(crate) mod mmio;
pub mod sync;
pub mod uart;
