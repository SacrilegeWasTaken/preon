<div align="center">

```
                             
                             
   ███████╗██╗  ██╗ ██████╗  
   ██╔════╝╚██╗██╔╝██╔═══██╗ 
   █████╗   ╚███╔╝ ██║   ██║ 
   ██╔══╝   ██╔██╗ ██║   ██║ 
   ███████╗██╔╝ ██╗╚██████╔╝ 
   ╚══════╝╚═╝  ╚═╝ ╚═════╝  
```

![arch](https://img.shields.io/badge/arch-aarch64-blue)
![target](https://img.shields.io/badge/target-aarch64--unknown--none-orange)
![rust](https://img.shields.io/badge/rust-2024-red)
![deps](https://img.shields.io/badge/deps-fdt%20only-green)
![style](https://img.shields.io/badge/style-microkernel-purple)
![status](https://img.shields.io/badge/status-bring--up-yellow)

</div>

## Overview

The real working kernel written from scratch in Rust for the ARMv8-A architecture, booting
on the QEMU `virt` machine. No external crates (excluding fdt because I'm not 
a psycho) — just `core` and a `global_asm!` boot stub. The goal is to write a 
micro-kernel referencing freebsd(mostly)/seL4/linux kernels code. No LLM codegen 
was ever used, but, LLMs WILL be used to write docs, and they ARE used to search 
ARMv8-A docs and for general OS-dev educational purposes. I'm just too lazy to
educate myself the old-fashioned way.

## Waypoints

The plan is grouped into phases. Each phase unlocks the next: nothing further
makes sense until its prerequisites are in place.

### Phase 0 — Bring-up

- [x] Freestanding `no_std` / `no_main` binary, custom target `aarch64-unknown-none`
- [x] Linker script with fixed kernel base `0x40080000`
- [x] Boot stub in `global_asm!`: EL2 → EL1 drop, `CPACR_EL1.FPEN`, stack, BSS clear
- [x] PL011 UART driver (busy-wait TX), `RawUart` for emergency / panic output
- [x] `SpinLock<T>` + global `UART` singleton with RAII guard
- [x] `kernel_uart_log!` / `kernel_uart_direct_log!` macros with `core::fmt` formatting
- [x] DTB parsing (`fdt`) for RAM regions and CPU enumeration
- [x] Cargo workspace split: `kernel_core` / `kernel_builtin` / `kernel_arch` /
      `kernel_exceptions` / `kernel_cpu`
- [x] `make image` (raw binary via `llvm-objcopy`) + QEMU runner

### Phase 1 — Exceptions and observability

- [x] `VBAR_EL1` vector table (16 slots × 128 B, 2 KB aligned)
- [x] Linux-style entry: `vector_entry` / `impl_handler` / `save_context` /
      `restore_context` / `common_exit` macros, `eret` return path
- [x] `TrapFrame` on the kernel stack, mirrored to a `#[repr(C)]` Rust struct
- [x] Eight typed handlers (EL1 / EL0 × sync / IRQ / FIQ / SError) + `bad_mode`
- [x] `read_sysreg!` / `write_sysreg!` macros
- [x] `ESR_EL1` exception class decoding into a human-readable string
- [ ] Full `TrapFrame` dump on panic (registers, ELR, SPSR, FAR, decoded ESR)
- [x] `panic_handler` writes through `RawUart` and decodes location info

### Phase 2 — SMP bring-up (current)

- [x] PSCI parser (`/psci` node from DTB: `method`, `cpu_on` function ID)
- [ ] `Psci::cpu_on` via `hvc` / `smc` with full SMCCC clobber list
- [ ] Statically-allocated per-CPU stacks and `CpuData` array
- [ ] `TPIDR_EL1` per-CPU pointer: `install_current_cpu_local` / `current_cpu`
- [ ] `secondary_entry` trampoline completes CPU-local init (FP, VBAR)
- [ ] `secondary_cpu_main` Rust entry, online barrier via `AtomicU64` bitmap
- [ ] Primary CPU enumerates secondaries from DTB and brings them up

### Phase 3 — Virtual memory

- [ ] Physical frame allocator (bitmap)
- [ ] Page table walker / builder for 4-level AArch64 translation
- [ ] `MAIR_EL1`, `TCR_EL1`, `TTBR0_EL1`, `TTBR1_EL1` configuration
- [ ] Identity-mapped boot transition into MMU, kernel relocated to upper half
- [ ] TLB invalidation primitives (`tlbi`, with the right `dsb`/`isb`)
- [ ] Kernel heap allocator (`#[global_allocator]`), `alloc` available
- [ ] Per-CPU IRQ stacks, guard pages
- [ ] Page-fault handler hook in `el1_sync` / `el0_sync`

### Phase 4 — Time and interrupts

- [ ] GICv3 distributor / redistributor initialization
- [ ] Generic Timer (`CNTP_*_EL0`), tick at fixed HZ
- [ ] IRQ dispatch table, registration API for drivers
- [ ] UART RX through interrupts (replace busy-wait read)
- [ ] IPI (inter-processor interrupts) for SMP coordination

### Phase 5 — Tasks and scheduling

- [ ] Kernel-thread abstraction: context, stack, state
- [ ] Context switch (full GP + SIMD save/restore)
- [ ] Round-robin scheduler, preemption on timer tick
- [ ] Per-CPU run queues, work-stealing later
- [ ] Sleep / wakeup primitives, condition variables

### Phase 6 — Userspace

- [ ] EL0 transition: `SPSR_EL1.M = 0`, separate user / kernel stacks
- [ ] Per-process address spaces (separate `TTBR0_EL1` per task)
- [ ] ELF loader, copy-on-write fork
- [ ] System-call entry via `svc`, narrow syscall table (seL4-inspired)
- [ ] Userspace `init` printing through a syscall to the in-kernel UART

### Phase 7 — Capabilities and IPC

- [ ] Capability table per process, kernel-managed handle space
- [ ] Endpoint object, synchronous send / receive / call
- [ ] Reply capabilities, badge identification
- [ ] First userspace server (e.g. a `null` service) talking via IPC
- [ ] Page-rights derivation from memory capabilities

### Phase 8 — Drivers in userspace

- [ ] virtio-blk driver as a userspace server
- [ ] Minimal in-tree filesystem (read-only initramfs first)
- [ ] virtio-net + ARP / ICMP on top
- [ ] UART driver moves out of the kernel; in-kernel `RawUart` kept only for panics

### Phase 9 — Tooling and polish

- [ ] DTB-based device discovery instead of hard-coded MMIO bases
- [ ] PSCI `system_off` / `system_reset` for clean shutdown
- [ ] Shell process as the first interactive userspace program
- [ ] Power management: `wfi`-driven idle, suspend / resume
- [ ] Documentation: IPC ABI, capability model, syscall reference
