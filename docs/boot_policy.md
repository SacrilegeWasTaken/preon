# Bootloader contract

This document specifies the contract between the bootloader and preon.
It defines what state preon assumes when execution arrives at `_start`
in `kernel/core/asm/boot.s`. Anything not listed here, the kernel is
free to set or change as it sees fit.

The reference loader is QEMU `-kernel` on the `virt` machine; the
contract is written to be compatible with any arm64 loader that
follows the Linux boot protocol (U-Boot, GRUB EFI stub, etc.).

---

## Image format

- Raw flat binary, produced by `llvm-objcopy -O binary` from an ELF
  built with `linker.ld`
- First byte of the binary is the entry instruction (start of
  `.text.boot`)
- The binary is contiguous in PA — gaps between sections (e.g.
  `__boot_bss_end` → `IMAGE_TEXT_PA`) are filled with zeros by
  `objcopy`

## Where to load

The image must be loaded contiguously starting at the physical address
defined by `PHYS_LOAD_BASE` in `linker.ld`:

```
PHYS_LOAD_BASE = 0x40080000
```

This matches the standard arm64 Linux entry on QEMU `virt`. The
bootloader must place the entire binary starting at this PA and
transfer control to that address.

## CPU state at `_start`

Required:

- **Execution state**: AArch64
- **Exception level**: EL1 or EL2 (the boot stub handles both; if at
  EL2, it drops to EL1 via `eret`)
- **MMU**: off (`SCTLR_EL1.M = 0`). The stub asserts this and branches
  to `bad_boot_mmu` if violated.
- **Endianness**: little-endian for both data and instructions
  (`SCTLR_EL1.{EE,E0E} = 0`)

Not required (preon handles these itself):

- FP/SIMD state — `CPACR_EL1.FPEN` may be anything; the stub sets it
- Exception vectors — `VBAR_EL1` is undefined; the stub installs one
  later
- Caches — clean state not required; the stub issues the necessary
  barriers
- Stack pointer — undefined; the stub establishes its own
- DAIF — not modified; we don't enable interrupts at boot
- TLB — assumed flushed (the stub flushes anyway)

## Register contract

At `_start`:

| Register | Contract |
|---|---|
| `x0` | PA of a valid Flattened Device Tree (FDT) blob |
| `x1`, `x2`, `x3` | Zero (Linux protocol; preon does not read them but they must be zero per the protocol) |
| `x4` … `x30` | Undefined (caller-saved) |
| `sp` | Undefined |

## FDT (device tree) requirements

The blob at `x0` must:

- Be a valid FDT (magic `0xd00dfeed`, well-formed header, accessible
  size)
- Remain in memory readable through the linear map after MMU is on
  (typically the bootloader places it in RAM below the kernel image —
  preon does not relocate or copy it)
- Not overlap the kernel image PA range
  (`PHYS_LOAD_BASE` … `__stack_top_pa`)

Required nodes:

| Node | Purpose | Used by |
|---|---|---|
| `/memory` | RAM regions (one or more `reg` entries) | `kernel_map::build_linear` |
| `/cpus` | CPU enumeration (`device_type = "cpu"`, `reg` per CPU) | future SMP bring-up |
| `/psci` | PSCI method (`hvc` or `smc`), `cpu_on` function ID | future SMP bring-up |
| `/chosen/stdout-path` | Reference to a PL011 UART node | kernel UART driver (early console) |

Required device:

- **PL011 UART** at PA `0x09000000` (QEMU `virt` default). Used as the
  early console before any device-region mapping exists. The PA is
  currently hardcoded in `boot.s` trampoline and the UART driver; it
  will become DTB-derived in a later phase.

## Memory map assumptions

| Region | PA | Notes |
|---|---|---|
| RAM | starts at `0x40000000` | At least 128 MiB; preon scales to whatever the DTB reports |
| Kernel image | `0x40080000` … `__stack_top_pa` | Must not overlap with FDT or any bootloader-reserved area |
| UART | `0x09000000` | PL011 register block, ~4 KiB |
| Anything else < `0x40000000` | MMIO (GIC, timer, virtio, …) | Discovered via DTB later; not assumed by the boot stub |

The QEMU `virt` machine satisfies all of these by default.

## Boot CPU

- Exactly one CPU brought up by the bootloader (the "boot CPU")
- All secondaries are parked (typically in a PSCI WFI loop inside
  EL3/EL2 firmware)
- Preon wakes secondaries via PSCI `CPU_ON` when ready (future
  Phase 4 of the roadmap)

## What the bootloader MUST NOT do

- Modify the loaded image after transferring control (preon executes
  in place; rewriting `.text` would corrupt running code)
- Reuse the kernel image PA range for any other purpose
- Pass an invalid, corrupt, or out-of-range FDT pointer in `x0`
- Leave secondaries running outside of WFI — preon does not race
  against pre-existing threads of execution

## What happens after `_start`

The boot stub takes over and performs its own initialization — EL
drop, FP/SIMD enable, BSS clear, page tables, MMU bring-up, jump to
Rust `kmain`. None of that is part of this contract; it is the
kernel's internal responsibility. The next document along the boot
path (TBD) describes the in-kernel bring-up sequence.
