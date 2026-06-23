# Preon — design intent

Preon is a microkernel. The TCB is small, drivers live in userspace,
authority is granted through capabilities. Everything else is built
on top of these three commitments.

This document describes what preon is trying to be, why, and what it
explicitly is not. For the boot contract see
[`boot_policy.md`](boot_policy.md); for the implementation roadmap see
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

- ELF binaries from a standard `aarch64-unknown-none` or
  `aarch64-unknown-linux-gnu` toolchain
- Disk-backed file systems via userspace `virtio-blk` server
- Network via userspace `virtio-net` server + TCP/IP stack
- Eventually: a Linux ABI personality for running unmodified Linux
  userspace under preon

### Two ABIs, one kernel

Preon exposes two ABI personalities:

1. **Native (capability-based).** Thin shim over seL4-style syscalls.
   Programs that opt into capability programming use this. The kernel's
   own syscall table is small and lives at this layer.
2. **Linux compat.** POSIX-like surface (`open`/`read`/`fork`/…)
   translated by a userspace runtime into native IPC + capability
   operations. Lets us run unmodified Linux userspace binaries without
   any of POSIX semantics leaking into the kernel.

The kernel itself doesn't know about POSIX. The Linux personality is a
userspace library that translates Linux syscalls into preon
operations.

---

## Architecture layers

### Layer 1 — Microkernel TCB

What lives inside the privileged boundary:

- **Scheduler** — preemptive, per-CPU run queues
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

### Layer 4 — ABI personalities

The user-facing system-call surface lives here, as userspace
libraries:

- **Native runtime** — direct wrappers around kernel syscalls. Used
  by programs written for preon directly.
- **Linux runtime** — POSIX translation layer. Intercepts
  `open`/`read`/`fork`/… and rewrites them as native IPC + capability
  operations against the appropriate userspace servers.

Adding a new personality (BSD, Plan 9, …) means writing a new
userspace library, not touching the kernel.

---

## What preon is NOT

- **Not a Unix.** No POSIX in the kernel. No global FS namespace. No
  ambient `uid`. Linux compatibility comes from userspace
  translation, not kernel concession.
- **Not formally verified** (yet). The architecture is verifiable in
  principle, but proof work is its own multi-year project. We aim for
  a small enough TCB that proof becomes plausible later — not for
  proof itself.
- **Not a research kernel.** Every feature must justify its existence
  against the goal of running real software with a small TCB. Cool
  ideas that don't serve that goal get deferred or dropped.
- **Not "Linux-fast" at everything.** Microkernels pay an IPC tax.
  We aim for that tax to be small (Linux-comparable on most
  workloads, worse on extremely syscall-heavy ones) but we don't
  pretend it's zero.
- **Not portable to everything.** AArch64 first. Other architectures
  may come, but the design assumes a modern MMU + capability-friendly
  hardware. 32-bit, segmented architectures, etc., are out of scope.

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
- **Linux** — boot protocol, syscall ABI surface, the realities of
  what userspace expects to see. We don't follow Linux's kernel
  design, but we follow what Linux userspace assumes about its
  environment.

---

## Current status

See [`../README.md`](../README.md) for the phase-by-phase roadmap and
what's been built so far. As of this writing, preon boots on QEMU
virt, runs in the upper half via TTBR1 with separated linear/image
regions, and enforces per-section permissions (.text RO+X,
.rodata RO+NX, .data/.bss/.stack RW+NX). No userspace yet.
