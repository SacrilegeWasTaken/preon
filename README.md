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

## The bet — hand-build the core, port the world with AI

Every from-scratch OS dies the same way: it boots, and then nobody uses it,
because it has no drivers and no software. Writing all of that by hand
isn't a plan — it's several lifetimes, and not one person's.

So preon doesn't try. Only two things are written by hand: **the kernel**
and **Vanguard**, the init process. Everything above that is *ported*, not
authored, by two AI frameworks built for the job:

- **Driver Porting ToolKit** — ingests open-source drivers from other
  kernels (Linux and FreeBSD first), reads their register maps and DMA
  setup, and rewrites them as sandboxed preon userspace drivers.
- **Software Porting ToolKit** — helps developers see where preon differs
  from a classic OS and port applications accordingly — either keeping
  cross-OS compatibility or targeting preon natively.

Using AI here isn't a gimmick; it's the only sane answer to a volume of
work no individual could ever do by hand. The leverage *is* the strategy.

And it's safe **because of** the microkernel design, not despite the AI.
Every driver, every server, every ported program runs in **userspace** — a
sloppy or even malicious port cannot reach the kernel. When ported code
faults, the kernel contains it and restarts the service; the system stays
up. Capabilities make that airtight: nothing holds authority it wasn't
explicitly handed.

The goal is blunt — **out-build the incumbents as a single developer** and
move the world onto a modern, flexible OS:

- **For developers and enthusiasts** — you can change literally anything
  in the system *except* the memory manager, the scheduler, and the IPC
  core (plus a couple of small pieces). Everything else is userspace and
  yours to replace.
- **For everyone else** — reliability you can feel: a tiny, hardened kernel
  plus capability isolation means a misbehaving driver or app degrades one
  service, never the whole machine.

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

The north star behind the phases: a **verification-first** microkernel
that runs **everywhere** (server → embedded), **across architectures** via
a HAL, and hosts **foreign software** (Linux) through userspace
ABI-personality servers. The near-term phases build the core that makes
that possible.

### Phase 0 — Bring-up

- [x] Freestanding `no_std` / `no_main` binary, custom target `aarch64-unknown-none`
- [x] Linker script with fixed kernel load base `0x40080000`
- [x] Boot stub in `global_asm!`: EL2 → EL1 drop, `CPACR_EL1.FPEN`, stack, BSS clear
- [x] PL011 UART driver (busy-wait TX), `RawUart` for emergency / panic output
- [x] `SpinLock<T>` + global `UART` singleton with RAII guard
- [x] `kernel_log!` (locked) / `kernel_log_raw!` (unlocked, panic / exception) macros with `core::fmt`
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
- [x] Reclaim `.boot.bss` trampoline page tables (~48 KiB) after `disable_ttbr0`
- [ ] Memory zones (DMA / normal) hint — can stay flat for now
- [x] `#[global_allocator]` slab on top of buddy (size-class free-lists,
      buddy-backed pages), Kani-verified core; `alloc::` available. Per-CPU
      caches and slab reclaim deferred to Phase 4

### Phase 4 — SMP (production-class)

Secondaries are brought up in the upper half: PSCI `CPU_ON` lands each on
a physical entry, a per-CPU MMU trampoline installs the runtime translation,
and it resumes in the kernel's virtual address space. Confirmed on
`qemu -smp 4` — all secondaries reach `secondary_cpu_main`.

- [x] Secondary entry point (`secondary.s`): MMU trampoline onto the runtime
      root (identity TTBR0 to survive the `SCTLR.M` flip), no BSS clear / build
- [x] Per-CPU stacks allocated from the buddy (one per online CPU)
- [x] `TPIDR_EL1` as per-CPU pointer (`install_current_cpu_local`, `current_cpu`)
- [x] PSCI `CPU_ON` for each secondary, online barrier (`mark_online` / `wait_for`)
- [x] Spin-locks (`SpinLock`, single-init `Once`); ticket / MCS deferred until
      contention shows
- [ ] Kernel stack overflow protection — **mechanism to be decided**. A guard
      page (invalid in the MMU) is cheap defense-in-depth but needs L3
      block-splitting in the linear map or a dedicated stack VA region; the
      seL4-tradition answer is a small run-to-completion per-CPU stack with a
      proven depth bound (and revisiting today's 64 KiB size). Think it through
      before building
- [ ] Per-CPU slab caches + slab reclaim (fast path on a `TPIDR_EL1`-local
      free-list; slow path the current global slab)
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
- [ ] Heterogeneous-core (P/E) awareness + a flexible priority/hint API so
      userspace steers placement — scheduling *policy* stays out of the kernel
- [ ] Sleep / wakeup primitives, condition variables

### Phase 7 — Userspace

- [ ] EL0 transition: `SPSR_EL1.M = 0`, separate user / kernel stacks
- [ ] KASLR — randomize the kernel image slide (the `image_va_base` seam
      is already in place for it)
- [ ] Per-process address spaces (separate `TTBR0_EL1` per task)
- [ ] ELF loader, copy-on-write fork
- [ ] System-call entry via `svc`, narrow syscall table (seL4-inspired)
- [ ] `libsys` — the floor of the system library stack: raw `svc` stubs for
      the capability syscalls + IPC primitives, so userspace never hand-writes
      assembly. Static linking + a minimal in-process allocator (bump /
      linked-list) pulling pages from the kernel
- [ ] Userspace `init` (**Vanguard**, the root task) printing through a
      syscall to the in-kernel UART; grows into namespace assembly and
      capability distribution once IPC lands (Phase 8)

### Phase 8 — Capabilities and IPC

- [ ] Capability table per process, kernel-managed handle space
- [ ] Endpoint object, synchronous send / receive / call
- [ ] Reply capabilities, badge identification
- [ ] First userspace server (e.g. a `null` service) talking via IPC
- [ ] Page-rights derivation from memory capabilities

### Phase 9 — Drivers in userspace

- [ ] virtio-blk driver as a userspace server
- [ ] Minimal in-tree filesystem (read-only initramfs first)
- [ ] VFS server as a **namespace of IPC channels** (see
      [`docs/IDEA.md`](docs/IDEA.md)): a uniform file protocol
      (walk / open / read / write / stat / close), longest-prefix mount
      routing, per-process namespaces, async zero-copy reads via shared-memory
      buffers. `devfs` / `procfs` are ordinary servers mounted into a namespace
- [ ] virtio-net + ARP / ICMP on top
- [ ] UART driver moves out of the kernel; in-kernel `RawUart` kept only for panics

### Phase 10 — Tooling and polish

- [ ] DTB-based device discovery instead of hard-coded MMIO bases
- [ ] PSCI `system_off` / `system_reset` for clean shutdown
- [ ] Shell process as the first interactive userspace program
- [ ] Power management: `wfi`-driven idle, suspend / resume
- [ ] Documentation: IPC ABI, capability model, syscall reference

### Beyond the phases — the long horizon

Vision-level work from [`docs/IDEA.md`](docs/IDEA.md), sequenced loosely
once the core kernel stands:

- **Multi-arch via the HAL** — factor the ARM64-specific bits (Layer 0)
  behind a narrow interface, then bring up a second backend (RISC-V or
  x86-64), then ARM32
- **The system library stack** — `libpreon_{io,mem,thread}` shims over
  `libsys`; a ported `libc` (musl or newlib) whose syscall layer targets
  those shims; `libc++` / `libunwind` riding unchanged on `libc`; and a
  userspace dynamic linker (`ld.so`) once static-only linking gets old.
  Native servers stay `#![no_std]` over `core` + `alloc`
- **Toolchain & language targets** — an `aarch64-unknown-preon` target:
  a local JSON spec + `-Z build-std` now, then upstream the `preon` OS triple
  to **LLVM** (one small patch lights up clang / rustc / zig at once) and a
  target spec + `std` backend to **rustc**, moving from Rust Tier 3 (in-tree)
  toward Tier 2 (`rustup target add`)
- **ABI personalities** — a userspace Linux ABI server; a process tagged
  with a personality has its syscalls shifted and routed to it over IPC, and
  is handed a private `/compat/linux` namespace with native subtrees bound in,
  so Linux binaries edit native files as first-class citizens without any of
  it leaking into the kernel. Design + the open problems (`execve` interp
  paths, `stat` translation, locks, `inotify`, `unlink` lifetimes) live in
  [`docs/LINUX_ABI.md`](docs/LINUX_ABI.md)
- **Deeper verification** — grow the Kani-checked surface as subsystems
  land; keep the TCB small enough that whole-subsystem proofs stay plausible
- **The microcontroller tier** — MPU-based protection for no-MMU targets,
  a different and later protection model

## Further reading

- [`docs/BOOT_CONTRACT.md`](docs/BOOT_CONTRACT.md) — boot protocol: entry
  conditions, pre-MMU sequence, trampoline tables, MMU enable, runtime
  setup, TTBR0 teardown, invariants per stage
- [`docs/IDEA.md`](docs/IDEA.md) — layered architecture preon is built toward
- [`docs/LINUX_ABI.md`](docs/LINUX_ABI.md) — running Linux binaries: native
  file access via bound namespaces, and the kernel behaviours the ABI server
  must fake
- [`docs/IPC.md`](docs/IPC.md) — IPC design notes (placeholder)
- [`docs/VERIFICATION.md`](docs/VERIFICATION.md) — Kani model-checking: scope,
  what's provable vs out of reach, how to run, harness inventory
