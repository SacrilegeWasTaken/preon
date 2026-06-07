//! Typed MMIO register wrapper.
//!
//! Wraps a raw `*mut T` with access-mode markers (read-only, write-only,
//! read/write) so the compiler refuses misuse — calling `.write()` on a
//! `Reg<_, ReadOnly>` doesn't compile, and the wrapper always uses
//! `read_volatile` / `write_volatile` so the optimiser cannot fold or
//! reorder MMIO accesses.
//!
//! Inspired by `tock-registers` and `volatile-register`, but kept thin
//! to avoid pulling in a crate.

use core::marker::PhantomData;
use core::ptr::{read_volatile, write_volatile};

/// Access-mode marker: register can be read.
pub struct ReadOnly;

/// Access-mode marker: register can be written.
pub struct WriteOnly;

/// Access-mode marker: register supports both read and write.
pub struct ReadWrite;

/// Trait satisfied by access modes that permit reads.
pub trait Readable {}
impl Readable for ReadOnly {}
impl Readable for ReadWrite {}

/// Trait satisfied by access modes that permit writes.
pub trait Writable {}
impl Writable for WriteOnly {}
impl Writable for ReadWrite {}

/// Typed handle to an MMIO register at a fixed address.
///
/// `T` is the in-memory layout of the register (a `Copy` value the same
/// size as the underlying word — typically `u32`, or a `#[repr(transparent)]`
/// newtype over `u32`). `M` is the access mode.
#[repr(transparent)]
pub struct Reg<T, M> {
    addr: *mut T,
    _mode: PhantomData<M>,
}

// MMIO addresses are stable for the entire kernel run and the
// `read_volatile` / `write_volatile` primitives are concurrency-safe at
// the bus level — Send / Sync are appropriate.
unsafe impl<T, M> Send for Reg<T, M> {}
unsafe impl<T, M> Sync for Reg<T, M> {}

impl<T, M> Reg<T, M> {
    /// Construct a register handle at a fixed MMIO address.
    ///
    /// # Safety
    /// `addr` must be the physical address of a hardware register that
    /// holds a `T` and supports the access mode `M`. The kernel assumes
    /// any access goes through the volatile pair, never through plain
    /// dereference of the underlying pointer.
    pub const unsafe fn new(addr: usize) -> Self {
        Self {
            addr: addr as *mut T,
            _mode: PhantomData,
        }
    }

    /// Numeric address of the register, useful for debug logging.
    pub fn addr(&self) -> usize {
        self.addr as usize
    }
}

impl<T: Copy, M: Readable> Reg<T, M> {
    /// Read the register through `read_volatile`. The compiler must
    /// emit exactly one bus transaction at the configured address.
    #[inline]
    pub fn read(&self) -> T {
        // Safety: address validity is the obligation taken on by the
        // unsafe `new`; volatile means no folding or reorder happens.
        unsafe { read_volatile(self.addr) }
    }
}

impl<T: Copy, M: Writable> Reg<T, M> {
    /// Write the register through `write_volatile`.
    #[inline]
    pub fn write(&self, value: T) {
        // Safety: see `read`.
        unsafe { write_volatile(self.addr, value) }
    }
}
