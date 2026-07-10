<div align="center">

```
██████╗ ██████╗ ███████╗ ██████╗ ███╗   ██╗
██╔══██╗██╔══██╗██╔════╝██╔═══██╗████╗  ██║
██████╔╝██████╔╝█████╗  ██║   ██║██╔██╗ ██║
██╔═══╝ ██╔══██╗██╔══╝  ██║   ██║██║╚██╗██║
██║     ██║  ██║███████╗╚██████╔╝██║ ╚████║
╚═╝     ╚═╝  ╚═╝╚══════╝ ╚═════╝ ╚═╝  ╚═══╝
```

![arch](https://img.shields.io/badge/arch-aarch64-blue)
![target](https://img.shields.io/badge/target-aarch64--unknown--none-orange)
![rust](https://img.shields.io/badge/rust-2024-red)
![deps](https://img.shields.io/badge/deps-fdt%20only-green)
![style](https://img.shields.io/badge/style-microkernel-purple)
![status](https://img.shields.io/badge/status-bring--up-yellow)

## DISCLAIMER! A note on AI slop

Code in this repo is written line by line by me, with LLM assistance
for spec lookup, code review, and doc writing. Every commit reflects
a design I can explain and defend without prompts. This isn't a generated
kernel — it's a hand-built one with a smart reference assistant.

</div>

---

## Overview

Preon is a microkernel written from scratch in Rust for the ARMv8-A
architecture, booting on the QEMU `virt` machine. No external crates
(excluding `fdt` because I'm not a psycho) — just `core` and a
`global_asm!` boot stub. The goal is to grow a seL4-style microkernel
(small TCB, drivers in userspace, capability-based), referencing
FreeBSD (mostly) / seL4 / Linux kernel code where it helps.

The kernel runs in the upper half via TTBR1, with three distinct
virtual-address regions:

| Region | Base | Purpose | Memory attrs |
|---|---|---|---|
| Linear map | `0xFFFF_8000_0000_0000` | All physical RAM as a flat window | Normal cacheable, RW + NX |
| Image | `0xFFFF_FFFF_8000_0000` | Kernel `.text`/`.rodata`/`.data`/`.bss`/stack | Per-section RO+X / RO+NX / RW+NX |
| Device | `0xFFFF_C000_0000_0000` (planned) | MMIO (UART, GIC, timer) | Device-nGnRE, RW + NX |

See [`docs/BOOT_CONTRACT.md`](docs/BOOT_CONTRACT.md) for the full bring-up
sequence and the invariants each stage establishes.

## Build & run

The only host requirement is [Nix](https://nixos.org/download) with
flakes enabled. The flake provides a pinned Rust toolchain (with the
`aarch64-unknown-none` target), LLVM, and QEMU — nothing else needs to
exist on `$PATH`.

```
make shell    # nix develop  — interactive dev shell with all tooling
make build    # nix run .#build — cargo build --release + llvm-objcopy → build/Image
make run      # nix run .#run   — build, then qemu-system-aarch64 with our flags
make clean    # nix run .#clean — cargo clean + rm build/
```

`make` is a thin wrapper around `nix run .#…`; use whichever you prefer.
Extra QEMU flags can be passed through:

```
nix run .#run -- -d guest_errors,unimp -D /tmp/qemu.log
```

Exit QEMU with `Ctrl-A` then `x`.

### Without Nix

If you'd rather use your own toolchain, install:

- Rust (stable) with target `aarch64-unknown-none`,
  `rust-src` and `llvm-tools-preview` components,
- LLVM (for `llvm-objcopy`),
- QEMU (with `aarch64` system emulation).

Then:

```
cargo build --release
llvm-objcopy -O binary target/aarch64-unknown-none/release/kernel build/Image
qemu-system-aarch64 -M virt -cpu cortex-a72 -m 2G -nographic -kernel build/Image
```

## Roadmap

The plan is grouped into phases. Each phase unlocks the next: nothing
further makes sense until its prerequisites are in place. See
[`docs/IDEA.md`](docs/IDEA.md) for the layered architecture the kernel
is built toward.

### Phase 0 — Bring-up

- [x] Freestanding `no_std` / `no_main` binary, custom target `aarch64-unknown-none`
- [x] Linker script with fixed kernel load base `0x40080000`
- [x] Boot stub in `global_asm!`: EL2 → EL1 drop, `CPACR_EL1.FPEN`, stack, BSS clear
- [x] PL011 UART driver (busy-wait TX), `RawUart` for emergency / panic output
- [x] `SpinLock<T>` + global `UART` singleton with RAII guard
- [x] `kernel_uart_log!` / `kernel_uart_direct_log!` macros with `core::fmt` formatting
- [x] DTB parsing (`fdt`) for RAM regions and CPU enumeration
- [x] Cargo workspace split: `kernel_core` / `kernel_builtin` / `kernel_arch` /
      `kernel_exceptions` / `kernel_cpu` / `kernel_mm`
- [x] `make build` (raw binary via `llvm-objcopy`) + QEMU runner

### Phase 1 — Exceptions and observability

- [x] `VBAR_EL1` vector table (16 slots × 128 B, 2 KB aligned)
- [x] Linux-style entry: `vector_entry` / `impl_handler` / `save_context` /
      `restore_context` / `common_exit` macros, `eret` return path
- [x] `TrapFrame` on the kernel stack, mirrored to a `#[repr(C)]` Rust struct
- [x] Eight typed handlers (EL1 / EL0 × sync / IRQ / FIQ / SError) + `bad_mode`
- [x] `read_sysreg!` / `write_sysreg!` macros
- [x] `ESR_EL1` exception-class decoding into a human-readable string, `Esr` newtype
- [x] Full `TrapFrame` dump on every handler (registers, ELR, SPSR, FAR, decoded ESR)
- [x] `panic_handler` writes through `RawUart` and dumps register state

### Phase 2 — Virtual memory

The kernel now lives in the upper half with separate VA regions for
the linear map and the kernel image. Image is mapped page-by-page with
per-section permissions; linear stays as a coarse 1 GiB-block map of
RAM. See [`docs/BOOT_CONTRACT.md`](docs/BOOT_CONTRACT.md) for the bring-up
ordering.

- [x] `MAIR_EL1`, `TCR_EL1`, `TTBR0_EL1`/`TTBR1_EL1` initial configuration in asm
- [x] Static trampoline page tables in `.boot.bss`, identity-mapped TTBR0 (`.boot` + UART + RAM)
- [x] SCTLR.M flip with the proper `dsb`/`tlbi`/`isb` barrier ceremony
- [x] Trampoline TTBR1 with linear + image regions (1 GiB blocks each)
- [x] Image jump via `br x16` to `kmain` resolved at image VMA
- [x] Page-table walker / builder for 4-level AArch64 translation,
      typed `Level`/`Access`/`Shareability`/`Executable`/`LeafAttrs`
- [x] TLB invalidation primitives (`tlbi vmalle1is`, `dsb ish` / `isb`)
- [x] Bootstrap bump frame allocator (static pool, ~32 pages)
- [x] DTB-driven linear map covering all physical RAM (1 GiB / 2 MiB blocks)
- [x] Runtime kernel map built in Rust, installed via `switch_ttbr1`
- [x] Image region split into per-section 4 KiB pages:
      `.text` RO+X, `.rodata` RO+NX, `.data` RW+NX, `.bss` RW+NX, `.stack` RW+NX
- [x] Linker symbols for every section boundary (`__text_start`/`__text_end`/…)
- [x] Permission enforcement verified end-to-end (write to `.text` → DFSC=0xF, L3 perm fault)
- [x] Device region for MMIO (UART, future GIC/timer) with Device-nGnRE attrs
- [x] UART driver migration from TTBR0 identity to TTBR1 device VA
- [x] TTBR0 teardown (`msr ttbr0_el1, xzr`, `TCR_EL1.EPD0=1`)
- [x] Page-fault handler hook in `el1_sync`: typed abort decode (`ESR`/`FAR`,
      fault status + level + access kind), reports and parks. `el0_sync`
      stays a generic dump until userspace (Phase 7) can fault

### Phase 3 — Production allocator (current)

The bootstrap bump pool is a placeholder. Real allocator unlocks SMP
(per-CPU stacks), userspace (page allocation), and frees the trampoline
page tables back to the system.

- [x] Buddy allocator (Linux-style orders, free-lists by order) — core
      formally verified with Kani (`alloc` split, `free` coalesce, `free_range`
      carve, mass conservation; see [`docs/VERIFICATION.md`](docs/VERIFICATION.md))
- [x] Initialize from DTB memory map minus kernel image + reserved regions
- [x] Replace `kernel_mm::frame::alloc_page` to back onto buddy
- [ ] Reclaim `.boot.bss` trampoline page tables (~36 KiB) after `disable_ttbr0`
- [ ] Memory zones (DMA / normal) hint — can stay flat for now
- [ ] `#[global_allocator]` slab on top of buddy, `alloc::` available

### Phase 4 — SMP (production-class)

The earlier SMP bring-up was removed when the kernel moved to the upper
half — `boot.s` isn't SMP-safe (BSS-clear race, stack collision). To
re-enable SMP we need the buddy allocator (per-CPU stacks) and a clean
secondary entry point.

- [ ] Secondary entry point in `boot.s` (no BSS clear, no MMU build)
- [ ] Per-CPU stacks allocated from buddy (one per online CPU)
- [ ] `TPIDR_EL1` as per-CPU pointer (`install_current_cpu_local`, `current_cpu`)
- [ ] PSCI `CPU_ON` for each secondary, online barrier
- [ ] Spin-locks (atomic → ticket → MCS as contention grows)
- [ ] IPI (software-generated interrupts via GIC)

### Phase 5 — Time and interrupts

- [ ] GICv3 distributor / redistributor initialization
- [ ] Generic Timer (`CNTP_*_EL0`), tick at fixed HZ
- [ ] IRQ dispatch table, registration API for drivers
- [ ] UART RX through interrupts (replace busy-wait read)

### Phase 6 — Tasks and scheduling

- [ ] Kernel-thread abstraction: context, stack, state
- [ ] Context switch (full GP + SIMD save/restore)
- [ ] Round-robin scheduler, preemption on timer tick
- [ ] Per-CPU run queues, work-stealing later
- [ ] Sleep / wakeup primitives, condition variables

### Phase 7 — Userspace

- [ ] EL0 transition: `SPSR_EL1.M = 0`, separate user / kernel stacks
- [ ] Per-process address spaces (separate `TTBR0_EL1` per task)
- [ ] ELF loader, copy-on-write fork
- [ ] System-call entry via `svc`, narrow syscall table (seL4-inspired)
- [ ] Userspace `init` printing through a syscall to the in-kernel UART

### Phase 8 — Capabilities and IPC

- [ ] Capability table per process, kernel-managed handle space
- [ ] Endpoint object, synchronous send / receive / call
- [ ] Reply capabilities, badge identification
- [ ] First userspace server (e.g. a `null` service) talking via IPC
- [ ] Page-rights derivation from memory capabilities

### Phase 9 — Drivers in userspace

- [ ] virtio-blk driver as a userspace server
- [ ] Minimal in-tree filesystem (read-only initramfs first)
- [ ] virtio-net + ARP / ICMP on top
- [ ] UART driver moves out of the kernel; in-kernel `RawUart` kept only for panics

### Phase 10 — Tooling and polish

- [ ] DTB-based device discovery instead of hard-coded MMIO bases
- [ ] PSCI `system_off` / `system_reset` for clean shutdown
- [ ] Shell process as the first interactive userspace program
- [ ] Power management: `wfi`-driven idle, suspend / resume
- [ ] Documentation: IPC ABI, capability model, syscall reference

## Further reading

- [`docs/BOOT_CONTRACT.md`](docs/BOOT_CONTRACT.md) — boot protocol: entry
  conditions, pre-MMU sequence, trampoline tables, MMU enable, runtime
  setup, TTBR0 teardown, invariants per stage
- [`docs/IDEA.md`](docs/IDEA.md) — layered architecture preon is built toward
- [`docs/IPC.md`](docs/IPC.md) — IPC design notes (placeholder)
- [`docs/VERIFICATION.md`](docs/VERIFICATION.md) — Kani model-checking: scope,
  what's provable vs out of reach, how to run, harness inventory
