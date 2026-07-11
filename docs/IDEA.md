# Preon — design intent

Preon is a microkernel. The TCB is small, drivers live in userspace,
authority is granted through capabilities. Everything else is built
on top of these three commitments.

The ambition is a full operating system on that base — one that runs
**everywhere** (server, desktop, embedded, reaching toward
microcontrollers), **across architectures** (ARM64 today; ARM32, x86-64,
RISC-V through a HAL), stays secure by treating **verification as a
design driver** rather than an afterthought, and runs **foreign software**
(Linux binaries) through userspace ABI-personality servers without any of
it leaking into the privileged core.

This document describes what preon is trying to be, why, and what it
explicitly is not. For the boot contract see
[`BOOT_CONTRACT.md`](BOOT_CONTRACT.md); for the implementation roadmap see
[`../README.md`](../README.md).

---

## Why a microkernel

Modern monolithic kernels carry decades of accumulated functionality
inside the privileged boundary. Linux has ~30 million lines of
kernel-mode code; any of them, if compromised, runs with full kernel
authority. The attack surface scales with the codebase, and so does
the cost of auditing it.

A microkernel inverts this: privileged code is small enough to audit
(or, in seL4's case, formally verify — ~10K lines of C). Everything
else — file systems, network stacks, device drivers — runs in
userspace, isolated by hardware translation and IPC.

Preon doesn't aim to be a research kernel. It aims to be a kernel one
person can hold in their head end-to-end, where each design decision
can be defended on first principles.

---

## What preon is

### A microkernel in the seL4 tradition

- **Capability-based authority.** No global namespaces, no ambient
  permissions. To map memory, send a message, or bind to an IRQ, the
  caller must hold a capability granting that exact right. The kernel
  mediates every operation against the capability table.
- **Synchronous IPC.** Send / receive / call as the primary
  communication primitive. Async signals layer on top.
- **Tiny TCB.** Scheduler, MMU manager, IPC engine, capability system,
  IRQ router — that's the kernel. No drivers, no VFS, no FS, no
  network stack in the privileged half.

### A practical kernel, not a demonstration

Preon aims to run real software, not to illustrate concepts:

- ELF binaries from a standard `aarch64-unknown-none` toolchain
- Disk-backed file systems via userspace `virtio-blk` server
- Network via userspace `virtio-net` server + TCP/IP stack

### One kernel ABI, many userspace personalities

The kernel exposes a single ABI — a thin shim over seL4-style capability
syscalls. It knows nothing about POSIX or any foreign convention.

Foreign ABIs live *outside* the kernel as **personality servers**. A
process is tagged with a personality (native, Linux, …); its syscalls are
shifted and routed over IPC to the matching server, and a userspace
**Linux ABI server** translates Linux syscalls into preon capability
operations. Compatibility is bought with IPC, not with kernel bloat: the
privileged core stays capability-native no matter how many personalities
run above it (the classic microkernel move — L4Linux, NT subsystems).

### The pillars, explicitly

- **Ubiquity** — one kernel from servers to embedded, MMU tiers first.
- **Multi-arch via a thin HAL** — ARM64 → ARM32, x86-64, RISC-V.
- **Verification-first** — model-check the logic cores as they land.
- **ABI personalities** — Linux (and more) via userspace servers over IPC.
- **Heterogeneous scheduling** — P/E-core aware, hint-driven placement.
- **KASLR** — the kernel image is slide-relocatable (the `image_va_base`
  seam already exists) so its layout isn't a fixed target.

---

## Architecture layers

### Layer 0 — Hardware Abstraction Layer

The one place the ISA and platform show through: page tables, exception
entry, per-CPU registers, timers, the boot contract. A narrow HAL keeps
the layers above architecture-agnostic. ARM64 is the first backend;
ARM32, x86-64, and RISC-V slot in here. A thin HAL is also what keeps the
verified core portable — the model-checked logic doesn't move when the
backend does.

### Layer 1 — Microkernel TCB

What lives inside the privileged boundary:

- **Scheduler** — preemptive, per-CPU run queues; heterogeneous-core aware
  (P/E cores) with a flexible priority/hint mechanism, so userspace can
  steer placement without scheduling *policy* living in the kernel
- **MMU manager** — page tables, address spaces, page-fault routing
- **IPC engine** — synchronous send / receive / call, endpoints,
  notifications
- **Capability system** — kernel-managed handle table per process,
  rights mask, derivation rules
- **IRQ router** — hardware IRQs converted into notifications and
  delivered to subscribed processes

That's the full list. Nothing else.

### Layer 2 — Universal Object Model

The kernel exposes a small set of typed objects, manipulated through
capabilities:

- **Process** — address space + capability table + threads
- **Thread** — CPU state, stack, scheduler entity
- **AddressSpace** — TTBR0 + page-table tree
- **MemoryRegion** — physical pages + permissions, mappable into one
  or more AddressSpaces
- **Endpoint** — synchronous IPC rendezvous point
- **Notification** — asynchronous signal bitfield
- **Capability** — typed reference to a kernel object with a rights mask

Every resource the kernel allocates is one of these objects.
Userspace cannot ask for "memory" abstractly — only "create a
MemoryRegion backed by these physical pages, with these rights."

### Layer 3 — Userspace servers

Outside the privileged boundary:

- **Device drivers** — UART (after early bring-up), virtio-blk,
  virtio-net, GPU, input
- **File-system servers** — initramfs first, then on-disk FS per type
- **Network stack** — TCP/IP, ARP
- **VFS / namespace** — path resolution, mount table

Each is a userspace process holding capabilities to the resources it
needs (MemoryRegion for MMIO, Notification for the device's IRQ,
Endpoint for client requests). The kernel doesn't know what a "file"
or a "socket" is.

### Layer 4 — Native ABI runtime

The user-facing system-call surface lives here as a userspace library:
direct wrappers around the kernel's capability syscalls, used by
programs written for preon. Keeping the surface in userspace lets it
evolve without touching the kernel.

---

## What preon is NOT

- **Not a Unix.** No POSIX in the kernel. No global FS namespace. No
  ambient `uid`. Foreign conventions, if ever wanted, stay entirely in
  userspace and never leak into the kernel.
- **Not a single whole-kernel proof — but verification-first.** We don't
  claim a full-kernel proof (that is seL4's decade-long achievement).
  Verification is instead a *design driver*: pure logic cores —
  allocators, page-table and fault decoding, address arithmetic — are
  model-checked with Kani as they are written, and the TCB is kept small
  enough that deeper proof stays plausible. Correctness is engineered in,
  not bolted on.
- **Not a research kernel.** Every feature must justify its existence
  against the goal of running real software with a small TCB. Cool
  ideas that don't serve that goal get deferred or dropped.
- **Not "Linux-fast" at everything.** Microkernels pay an IPC tax.
  We aim for that tax to be small (Linux-comparable on most
  workloads, worse on extremely syscall-heavy ones) but we don't
  pretend it's zero.
- **Not tied to one architecture.** ARM64 is the first target, but
  multi-arch is a goal, not an afterthought: ARM32, x86-64, and RISC-V
  are meant to arrive through the HAL (Layer 0). The design assumes memory
  protection — an MMU on the server/desktop/embedded tiers; the deep
  microcontroller tier (MPU-only, no MMU) is a longer-term aspiration with
  a necessarily different protection model. Segmented architectures stay
  out of scope.

---

## Guiding principles

When designing a feature or evaluating a change, these are the
questions we ask:

1. **Does it belong in the TCB?** If it can run in userspace, it
   should.
2. **What capability mediates the operation?** Every privileged
   operation is gated by a capability — no ambient authority.
3. **Is the failure mode local?** Bugs in a userspace server should
   crash that server, not the kernel.
4. **Can we explain the entire path?** From syscall entry to return
   value, on one whiteboard. If not, it's too complex.
5. **What does Linux do here, and why?** Linux's design isn't binding,
   but the friction of departure should be a conscious decision.

---

## Inspirations

- **seL4** — TCB scope, capability model, IPC primitives. The North
  Star of "small kernel, big claims, proven correct."
- **FreeBSD** — code style, naming conventions, internal structure.
  When in doubt about how to organize something inside the kernel, we
  look at FreeBSD (especially `sys/vm/` and the scheduler).
- **Linux** — boot protocol and the realities of arm64 bring-up. We
  reference its kernel implementation where it helps; we don't follow
  its kernel design.

---

## Current status

See [`../README.md`](../README.md) for the phase-by-phase roadmap and
what's been built so far. As of this writing, preon boots on QEMU virt,
runs in the upper half via TTBR1 with separated linear/image regions and
per-section permissions, has a **Kani-verified physical allocator stack**
(buddy + memblock-style bootmem + slab `#[global_allocator]`), and brings
up **all secondary CPUs (SMP)** in the upper half via a per-CPU MMU
trampoline. No userspace yet — the capability/IPC layer is next.
