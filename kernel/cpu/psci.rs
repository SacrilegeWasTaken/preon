use fdt::Fdt;

macro_rules! psci_call {
    ($insn:literal, $fn_id:expr, $a1:expr, $a2:expr, $a3:expr) => {{
        let result: i64;
        unsafe {
            core::arch::asm!(
                $insn,
                inout("x0") $fn_id => result,
                in("x1") $a1,
                in("x2") $a2,
                in("x3") $a3,
                lateout("x1") _, lateout("x2") _, lateout("x3") _,
                lateout("x4") _, lateout("x5") _, lateout("x6") _,
                lateout("x7")_, lateout("x8") _, lateout("x9") _,
                lateout("x10") _, lateout("x11")_, lateout("x12") _,
                lateout("x13") _, lateout("x14") _, lateout("x15") _,
                lateout("x16") _, lateout("x17") _,
                options(nostack),
          );
      }
      result
    }};
}

#[derive(Debug)]
pub enum Method {
    Hvc,
    Smc,
}

#[derive(Debug)]
pub struct Psci {
    pub method: Method,
    pub cpu_on_id: u32,
}

const MAX_CPUS: usize = 8;
const STACK_SIZE: usize = 64 * 1024;

impl Psci {
    pub fn from_fdt(fdt: &Fdt) -> Option<Self> {
        let node = fdt.find_node("/psci")?;

        let method = match node.property("method")?.as_str()? {
            "hvc" => Method::Hvc,
            "smc" => Method::Smc,
            _ => return None,
        };

        let cpu_on_id = node.property("cpu_on")?.as_usize()? as u32;

        Some(Self { method, cpu_on_id })
    }

    pub fn cpu_on(&self, cpu: u64, entry: u64, ctx: u64) -> i64 {
        let fn_id = self.cpu_on_id as u64;
        match self.method {
            Method::Hvc => psci_call!("hvc #0", fn_id, cpu, entry, ctx),
            Method::Smc => psci_call!("smc #0", fn_id, cpu, entry, ctx),
        }
    }
}
