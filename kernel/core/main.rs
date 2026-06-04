#![no_std]
#![no_main]

use core::arch::global_asm;
use core::panic::PanicInfo;

use kernel_builtin::{kernel_uart_direct_log, kernel_uart_log, wfe_loop};

#[derive(Debug)]
pub struct BootInfo {
    pub memory: MemoryInfo,
    pub cpus: CpuInfo,
    pub psci: Option<PsciInfo>,
    pub chosen: ChosenInfo,
}

#[derive(Debug)]
pub struct MemoryInfo {
    pub regions: [MemoryRegion; 8],
    pub region_count: usize,
}

#[derive(Debug, Copy, Clone)]
pub struct MemoryRegion {
    pub base: usize,
    pub size: usize,
}

#[derive(Debug)]
pub struct CpuInfo {
    pub cpu_count: usize,
}

#[derive(Debug)]
pub struct PsciInfo {
    pub method: PsciMethod,
}

#[derive(Debug)]
pub enum PsciMethod {
    Smc,
    Hvc,
}

#[derive(Debug)]
pub struct ChosenInfo {
    pub bootargs: Option<&'static str>,
    pub stdout_path: Option<&'static str>,
}

// Boot. Allow FP & SIMD / Setting the stack pointer / clear .bss section
global_asm!(include_str!("asm/boot.s"));

#[unsafe(no_mangle)]
pub extern "C" fn kmain(dtb: usize) -> ! {
    kernel_exceptions::install();

    kernel_uart_log!("Hello from ExOS!");

    let fdt = unsafe { fdt::Fdt::from_ptr(dtb as *const u8) }.expect("DTB error. Check QEMU conf.");

    let cpus = fdt.cpus().count();
    let memory = fdt.memory();

    let regions = memory.regions();

    let mut starting_addr: *const u8 = core::ptr::null();
    let mut cnt: u64 = 0;
    let mut total_size: usize = 0;
    let mut last_reg_start_addr: *const u8 = core::ptr::null();

    for (i, region) in regions.enumerate() {
        if i == 0 {
            starting_addr = region.starting_address;
        }
        cnt += 1;
        let size = region.size.expect("Size doesn't exist???");
        total_size += size;
        last_reg_start_addr = region.starting_address;
    }

    kernel_uart_direct_log!(
        "RAM {}: {:#p} - {:#p} ({} MiB). REGIONS: {}",
        total_size,
        starting_addr,
        last_reg_start_addr,
        total_size / 1024 / 1024,
        cnt
    );

    kernel_uart_direct_log!("CPUs: {cpus}");

    kernel_uart_direct_log!("Triggering exception...");
    unsafe { core::arch::asm!("udf #0") }
    kernel_uart_direct_log!("This should never print!");

    wfe_loop!();
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    kernel_uart_direct_log!("Kernel panic! {:?}", _info);
    wfe_loop!();
}
