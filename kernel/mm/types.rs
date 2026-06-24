//! General types for MMU control

use kernel_arch::mm::PhysAddr;

/// Cache-coherence domain of the mapped region.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Shareability {
    NonShareable,
    OuterShareable,
    /// All CPUs in the inner-shareable domain snoop each other. The
    /// right choice for kernel mappings on SMP.
    InnerShareable,
}

impl Shareability {
    pub const fn bits(self) -> u64 {
        match self {
            Shareability::NonShareable => 0b00,
            Shareability::OuterShareable => 0b10,
            Shareability::InnerShareable => 0b11,
        }
    }
}

/// Cacheability of MMU page-table walks (IRGN/ORGN in TCR_EL1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cacheability {
    NonCacheable,
    WriteBackReadWriteAlloc,
    WriteThroughReadAlloc,
    WriteBackReadAlloc,
}

impl Cacheability {
    pub const fn bits(self) -> u64 {
        match self {
            Cacheability::NonCacheable => 0b00,
            Cacheability::WriteBackReadWriteAlloc => 0b01,
            Cacheability::WriteThroughReadAlloc => 0b10,
            Cacheability::WriteBackReadAlloc => 0b11,
        }
    }
}

/// Translation granule size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granule {
    _4KB,
    _16KB,
    _64KB,
}

impl Granule {
    pub const fn to_tg0_bits(self) -> u64 {
        match self {
            Granule::_4KB => 0b00,
            Granule::_16KB => 0b10,
            Granule::_64KB => 0b01,
        }
    }

    pub const fn to_tg1_bits(self) -> u64 {
        match self {
            Granule::_4KB => 0b10,
            Granule::_16KB => 0b01,
            Granule::_64KB => 0b11,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysAddrSize {
    _32BIT,
    _36BIT,
    _40BIT,
    _42BIT,
    _44BIT,
    _48BIT,
    /// Requires FEAT_LPA; granule-dependent
    _52BIT,
}

impl PhysAddrSize {
    pub const fn bits(self) -> u64 {
        match self {
            PhysAddrSize::_32BIT => 0b000,
            PhysAddrSize::_36BIT => 0b001,
            PhysAddrSize::_40BIT => 0b010,
            PhysAddrSize::_42BIT => 0b011,
            PhysAddrSize::_44BIT => 0b100,
            PhysAddrSize::_48BIT => 0b101,
            PhysAddrSize::_52BIT => 0b110,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtAddrSize(u8);

impl VirtAddrSize {
    pub const fn from_va_bits(va_bits: u8) -> Self {
        assert!(
            va_bits >= 25 && va_bits <= 48,
            "VA size out of supported range"
        );
        Self(va_bits)
    }

    pub const fn tsz_bits(self) -> u64 {
        (64 - self.0) as u64
    }
}

pub trait FrameAllocator {
    fn alloc_page(&mut self) -> PhysAddr;
}
