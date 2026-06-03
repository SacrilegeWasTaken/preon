#![allow(dead_code)]

// PL011 UART register map for QEMU virt machine.
// Reference: ARM PrimeCell UART (PL011) Technical Reference Manual (DDI 0183).

use core::fmt;
use core::ptr::{read_volatile, write_volatile};

use crate::sync::SpinLock;

pub const UART_BASE: usize = 0x0900_0000;

pub const UART_DR: *mut u32 = UART_BASE as *mut u32;
pub const UART_RSR: *mut u32 = (UART_BASE + 0x004) as *mut u32;
pub const UART_FR: *const u32 = (UART_BASE + 0x018) as *const u32;
pub const UART_ILPR: *mut u32 = (UART_BASE + 0x020) as *mut u32;
pub const UART_IBRD: *mut u32 = (UART_BASE + 0x024) as *mut u32;
pub const UART_FBRD: *mut u32 = (UART_BASE + 0x028) as *mut u32;
pub const UART_LCRH: *mut u32 = (UART_BASE + 0x02C) as *mut u32;
pub const UART_CR: *mut u32 = (UART_BASE + 0x030) as *mut u32;
pub const UART_IFLS: *mut u32 = (UART_BASE + 0x034) as *mut u32;
pub const UART_IMSC: *mut u32 = (UART_BASE + 0x038) as *mut u32;
pub const UART_RIS: *const u32 = (UART_BASE + 0x03C) as *const u32;
pub const UART_MIS: *const u32 = (UART_BASE + 0x040) as *const u32;
pub const UART_ICR: *mut u32 = (UART_BASE + 0x044) as *mut u32;
pub const UART_DMACR: *mut u32 = (UART_BASE + 0x048) as *mut u32;

pub const FR_CTS: u32 = 1 << 0;
pub const FR_DSR: u32 = 1 << 1;
pub const FR_DCD: u32 = 1 << 2;
pub const FR_BUSY: u32 = 1 << 3;
pub const FR_RXFE: u32 = 1 << 4;
pub const FR_TXFF: u32 = 1 << 5;
pub const FR_RXFF: u32 = 1 << 6;
pub const FR_TXFE: u32 = 1 << 7;
pub const FR_RI: u32 = 1 << 8;

pub const LCRH_BRK: u32 = 1 << 0;
pub const LCRH_PEN: u32 = 1 << 1;
pub const LCRH_EPS: u32 = 1 << 2;
pub const LCRH_STP2: u32 = 1 << 3;
pub const LCRH_FEN: u32 = 1 << 4;
pub const LCRH_WLEN_5: u32 = 0b00 << 5;
pub const LCRH_WLEN_6: u32 = 0b01 << 5;
pub const LCRH_WLEN_7: u32 = 0b10 << 5;
pub const LCRH_WLEN_8: u32 = 0b11 << 5;
pub const LCRH_SPS: u32 = 1 << 7;

pub const CR_UARTEN: u32 = 1 << 0;
pub const CR_SIREN: u32 = 1 << 1;
pub const CR_SIRLP: u32 = 1 << 2;
pub const CR_LBE: u32 = 1 << 7;
pub const CR_TXE: u32 = 1 << 8;
pub const CR_RXE: u32 = 1 << 9;
pub const CR_DTR: u32 = 1 << 10;
pub const CR_RTS: u32 = 1 << 11;
pub const CR_OUT1: u32 = 1 << 12;
pub const CR_OUT2: u32 = 1 << 13;
pub const CR_RTSEN: u32 = 1 << 14;
pub const CR_CTSEN: u32 = 1 << 15;

pub const INT_RIM: u32 = 1 << 0;
pub const INT_CTSM: u32 = 1 << 1;
pub const INT_DCDM: u32 = 1 << 2;
pub const INT_DSRM: u32 = 1 << 3;
pub const INT_RX: u32 = 1 << 4;
pub const INT_TX: u32 = 1 << 5;
pub const INT_RT: u32 = 1 << 6;
pub const INT_FE: u32 = 1 << 7;
pub const INT_PE: u32 = 1 << 8;
pub const INT_BE: u32 = 1 << 9;
pub const INT_OE: u32 = 1 << 10;

#[allow(non_camel_case_types)]
pub struct UartRaw;

impl UartRaw {
    pub fn write_byte(&mut self, b: u8) {
        unsafe {
            while (read_volatile(UART_FR) & FR_TXFF) != 0 {}
            write_volatile(UART_DR, b as u32);
        }
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

pub static UART: SpinLock<UartDriver> = SpinLock::new(UartDriver { raw: UartRaw });

#[allow(non_camel_case_types)]
pub struct UartDriver {
    raw: UartRaw,
}

impl fmt::Write for UartDriver {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.raw.write_str(s)
    }
}

#[macro_export]
macro_rules! kernel_uart_log {
    ($($arg:tt)*) => {{
        use core::fmt::Write as _;
        let _ = writeln!($crate::uart::UART.lock(), $($arg)*);
    }};
}

#[macro_export]
macro_rules! kernel_uart_direct_log {
    ($($arg:tt)*) => {{
        use core::fmt::Write as _;
        let _ = writeln!($crate::uart::UartRaw, $($arg)*);
    }};
}
