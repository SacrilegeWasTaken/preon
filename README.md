<div align="center">

```
                              ┌──────────────────────────────────────┐
                              │                                      │
   ███████╗██╗  ██╗ ██████╗   │   bare-metal kernel for aarch64      │
   ██╔════╝╚██╗██╔╝██╔═══██╗  │   no_std · no dependencies · core    │
   █████╗   ╚███╔╝ ██║   ██║  │   targets qemu virt + cortex-a72     │
   ██╔══╝   ██╔██╗ ██║   ██║  │                                      │
   ███████╗██╔╝ ██╗╚██████╔╝  │                                      │
   ╚══════╝╚═╝  ╚═╝ ╚═════╝   └──────────────────────────────────────┘
```

![arch](https://img.shields.io/badge/arch-aarch64-blue)
![target](https://img.shields.io/badge/target-aarch64--unknown--none-orange)
![rust](https://img.shields.io/badge/rust-2024-red)
![deps](https://img.shields.io/badge/dependencies-zero-brightgreen)
![status](https://img.shields.io/badge/status-bring--up-yellow)

</div>

## Overview

A from-scratch kernel written in Rust for the ARMv8-A architecture, booting
on the QEMU `virt` machine. No external crates — just `core` and a
`global_asm!` boot stub. The goal is... I don't give a fuck. No LLM-codegen is ever used.

## Target

- Architecture: aarch64 (ARMv8-A, little-endian)
- CPU model: Cortex-A72
- Platform: QEMU `virt` machine
- Toolchain: `aarch64-unknown-none`

## Memory map (QEMU virt, relevant ranges)

```
  0x08000000 ──┬── GIC distributor
               │
  0x09000000 ──┼── PL011 UART        ◀── kernel serial output
               │
  0x40000000 ──┬── RAM start (128 MB)
               │
  0x40080000 ──┼── kernel load address (entry: _start)
               │
  0x47FFFFFF ──┘
```

## Status

```
[x] boot stub: enable FP/SIMD (CPACR_EL1), set SP, zero BSS
[x] jump into Rust kmain
[x] PL011 UART driver (polling TX)
[x] panic handler (wfe loop)
[ ] exception vectors (VBAR_EL1)
[ ] core::fmt::Write + print!/println! macros
[ ] MMU + page tables
[ ] physical/virtual memory allocators
[ ] scheduler, processes
```




