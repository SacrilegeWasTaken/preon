//! PL011 UART driver for the QEMU virt machine.
//!
//! Reference: ARM PrimeCell UART (PL011) TRM (DDI 0183).
//!
//! Registers are exposed as typed [`Reg`] handles with per-register
//! access modes. Bit fields use newtype flag types ([`FrFlags`],
//! [`CrFlags`], [`LcrhFlags`], [`InterruptFlags`]) so unrelated bit
//! patterns can never be OR'd together by accident.

use core::fmt;
use core::ops::{BitAnd, BitOr};

use crate::mmio::{ReadOnly, ReadWrite, Reg, WriteOnly};
use crate::sync::SpinLock;

/// Virtual base of the UART in the kernel device region. Equals
/// `pa_to_device_va(UART_PA)` = `KERNEL_DEVICE_BASE + UART_PA`, but is spelled
/// out as a literal because `kernel_builtin` sits below `kernel_mm` and cannot
/// call into `layout`. Must stay in sync with `kernel_mm::layout` and the
/// device branch in `boot.s`; the runtime map keys off the same VA.
const UART_BASE: usize = 0xFFFF_C000_0900_0000;

/// Physical address of the PL011 register block on QEMU `virt`. The single
/// source of truth for the UART PA, consumed by `kernel_mm::kernel_map` to
/// build the device mapping (kept hardcoded until DTB-driven discovery).
pub const UART_PA: usize = 0x0900_0000;

/*
 *
 * REGISTERS
 *
 */

/// Data Register. The low byte on write is sent out TX; on read it returns
/// the next byte from RX with error bits in the high half.
pub const UART_DR: Reg<u32, ReadWrite> = unsafe { Reg::new(UART_BASE) };

/// Receive Status / Error Clear.
pub const UART_RSR: Reg<u32, ReadWrite> = unsafe { Reg::new(UART_BASE + 0x004) };

/// Flag Register (status).
pub const UART_FR: Reg<FrFlags, ReadOnly> = unsafe { Reg::new(UART_BASE + 0x018) };

/// IrDA Low-Power Counter.
pub const UART_ILPR: Reg<u32, ReadWrite> = unsafe { Reg::new(UART_BASE + 0x020) };

/// Integer Baud Rate Divisor.
pub const UART_IBRD: Reg<u32, ReadWrite> = unsafe { Reg::new(UART_BASE + 0x024) };

/// Fractional Baud Rate Divisor.
pub const UART_FBRD: Reg<u32, ReadWrite> = unsafe { Reg::new(UART_BASE + 0x028) };

/// Line Control.
pub const UART_LCRH: Reg<LcrhFlags, ReadWrite> = unsafe { Reg::new(UART_BASE + 0x02C) };

/// Control.
pub const UART_CR: Reg<CrFlags, ReadWrite> = unsafe { Reg::new(UART_BASE + 0x030) };

/// Interrupt FIFO Level Select.
pub const UART_IFLS: Reg<u32, ReadWrite> = unsafe { Reg::new(UART_BASE + 0x034) };

/// Interrupt Mask Set/Clear.
pub const UART_IMSC: Reg<InterruptFlags, ReadWrite> = unsafe { Reg::new(UART_BASE + 0x038) };

/// Raw Interrupt Status.
pub const UART_RIS: Reg<InterruptFlags, ReadOnly> = unsafe { Reg::new(UART_BASE + 0x03C) };

/// Masked Interrupt Status.
pub const UART_MIS: Reg<InterruptFlags, ReadOnly> = unsafe { Reg::new(UART_BASE + 0x040) };

/// Interrupt Clear.
pub const UART_ICR: Reg<InterruptFlags, WriteOnly> = unsafe { Reg::new(UART_BASE + 0x044) };

/// DMA Control.
pub const UART_DMACR: Reg<u32, ReadWrite> = unsafe { Reg::new(UART_BASE + 0x048) };

/*
 *
 * FLAG TYPES
 *
 */

/// Boilerplate for a newtype that holds a bit-set over `u32`. Each
/// invocation declares the type, its associated constants, and the
/// standard `BitOr` / `BitAnd` / `contains` helpers.
macro_rules! bitflags_u32 {
    (
        $(#[$type_meta:meta])*
        $name:ident { $( $(#[$variant_meta:meta])* $variant:ident = $value:expr ; )+ }
    ) => {
        $(#[$type_meta])*
        #[derive(Debug, Copy, Clone, PartialEq, Eq)]
        #[repr(transparent)]
        pub struct $name(u32);

        impl $name {
            $(
                $(#[$variant_meta])*
                pub const $variant: Self = Self($value);
            )+

            pub const NONE: Self = Self(0);

            pub const fn raw(self) -> u32 {
                self.0
            }

            pub const fn from_raw(raw: u32) -> Self {
                Self(raw)
            }

            pub const fn contains(self, other: Self) -> bool {
                (self.0 & other.0) == other.0
            }
        }

        impl BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
        }

        impl BitAnd for $name {
            type Output = Self;
            fn bitand(self, rhs: Self) -> Self { Self(self.0 & rhs.0) }
        }
    };
}

bitflags_u32! {
    /// Flag Register (0x018) bits.
    FrFlags {
        /// Clear To Send (modem).
        CTS = 1 << 0;
        /// Data Set Ready (modem).
        DSR = 1 << 1;
        /// Data Carrier Detect (modem).
        DCD = 1 << 2;
        /// Transmitter busy: shift register still has data.
        BUSY = 1 << 3;
        /// Receive FIFO empty.
        RXFE = 1 << 4;
        /// Transmit FIFO full.
        TXFF = 1 << 5;
        /// Receive FIFO full.
        RXFF = 1 << 6;
        /// Transmit FIFO empty.
        TXFE = 1 << 7;
        /// Ring Indicator (modem).
        RI = 1 << 8;
    }
}

bitflags_u32! {
    /// Line Control Register (0x02C) bits.
    LcrhFlags {
        /// Send break (TX held low).
        BRK = 1 << 0;
        /// Parity enable.
        PEN = 1 << 1;
        /// Even parity select.
        EPS = 1 << 2;
        /// Two stop bits.
        STP2 = 1 << 3;
        /// FIFOs enable.
        FEN = 1 << 4;
        /// Word length 5 bits (WLEN = 0b00).
        WLEN_5 = 0b00 << 5;
        /// Word length 6 bits.
        WLEN_6 = 0b01 << 5;
        /// Word length 7 bits.
        WLEN_7 = 0b10 << 5;
        /// Word length 8 bits.
        WLEN_8 = 0b11 << 5;
        /// Sticky parity select.
        SPS = 1 << 7;
    }
}

bitflags_u32! {
    /// Control Register (0x030) bits.
    CrFlags {
        /// UART enable (master switch).
        UARTEN = 1 << 0;
        /// IrDA SIR enable.
        SIREN = 1 << 1;
        /// IrDA low-power mode.
        SIRLP = 1 << 2;
        /// Loopback enable.
        LBE = 1 << 7;
        /// Transmit enable.
        TXE = 1 << 8;
        /// Receive enable.
        RXE = 1 << 9;
        /// Data Terminal Ready (modem).
        DTR = 1 << 10;
        /// Request To Send (modem).
        RTS = 1 << 11;
        /// User-defined output 1.
        OUT1 = 1 << 12;
        /// User-defined output 2.
        OUT2 = 1 << 13;
        /// Hardware RTS flow control.
        RTSEN = 1 << 14;
        /// Hardware CTS flow control.
        CTSEN = 1 << 15;
    }
}

bitflags_u32! {
    /// Interrupt mask / status / clear bits (IMSC / RIS / MIS / ICR).
    InterruptFlags {
        /// Ring indicator modem interrupt.
        RIM = 1 << 0;
        /// CTS modem interrupt.
        CTSM = 1 << 1;
        /// DCD modem interrupt.
        DCDM = 1 << 2;
        /// DSR modem interrupt.
        DSRM = 1 << 3;
        /// Receive interrupt.
        RX = 1 << 4;
        /// Transmit interrupt.
        TX = 1 << 5;
        /// Receive timeout.
        RT = 1 << 6;
        /// Framing error.
        FE = 1 << 7;
        /// Parity error.
        PE = 1 << 8;
        /// Break error.
        BE = 1 << 9;
        /// Overrun error.
        OE = 1 << 10;
    }
}

/*
 *
 * DRIVERS
 *
 */

/// Zero-sized handle for direct, lock-free MMIO writes. Used by panic
/// and exception paths where taking the lock could deadlock.
pub struct UartRaw;

impl UartRaw {
    /// Send one byte, blocking on the TX FIFO if it's full.
    pub fn write_byte(&mut self, b: u8) {
        while UART_FR.read().contains(FrFlags::TXFF) {}
        UART_DR.write(b as u32);
    }
}

impl fmt::Write for UartRaw {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            self.write_byte(b);
        }
        Ok(())
    }
}

/// Singleton driver behind the global [`UART`] lock. Delegates to
/// [`UartRaw`] internally; the wrapper exists so the lock guard hands
/// out a named type rather than the raw byte writer.
pub struct UartDriver {
    raw: UartRaw,
}

impl fmt::Write for UartDriver {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.raw.write_str(s)
    }
}

pub static UART: SpinLock<UartDriver> = SpinLock::new(UartDriver { raw: UartRaw });

/*
 *
 * PRINT MACROS
 *
 */

/// Serialized log line through the UART `SpinLock`. Default logging path.
#[macro_export]
macro_rules! kernel_log {
    ($($arg:tt)*) => {{
        use core::fmt::Write as _;
        let _ = writeln!($crate::uart::UART.lock(), $($arg)*);
    }};
}

/// Unlocked log line straight to the UART MMIO. For contexts where taking the
/// lock is unsafe or deadlock-prone: panic, exception dumps, early boot.
#[macro_export]
macro_rules! kernel_log_raw {
    ($($arg:tt)*) => {{
        use core::fmt::Write as _;
        let _ = writeln!($crate::uart::UartRaw, $($arg)*);
    }};
}
