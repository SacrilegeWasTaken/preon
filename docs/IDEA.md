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

A personality also gets its own **filesystem view**. A Linux process is
handed a namespace rooted at a private `/compat/linux` subtree, so its
`/lib`, `/etc`, and `/proc` are *that subtree's* — never the native
system's. The ABI server rewrites the process's paths into the subtree and
forwards them to the native VFS; small Linux-flavored servers (an
`lx_procfs`, an `lx_devfs`) supply the synthetic trees. The native
filesystem stays pristine, and a Linux binary running `rm -rf /` can only
scour its own sandbox — it holds no capability to anything outside it.

That sandbox is not a prison, though. The root task can **bind** native
subtrees into the Linux namespace — a chosen workspace appears at, say,
`/home/you/projects` and resolves straight to the native VFS — while the
ABI server's path map lets user paths through and rewrites only the system
ones. A Linux editor then reads and writes native files as a first-class
citizen, bounded not by a wall but by exactly the capabilities its
namespace was granted (no GPU port bound in → it cannot draw). Making the
ABI server behave like a real Linux kernel over a foreign VFS raises a set
of concrete problems — `execve` interpreter paths, `stat` struct
translation, advisory locks, `inotify`, open-then-`unlink` lifetimes —
collected in [`LINUX_ABI.md`](LINUX_ABI.md).

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

## The file protocol and per-process namespaces

preon has no single global filesystem tree. Following Plan 9 and QNX, the
"filesystem" is a **namespace of IPC channels**: a name resolves to a
capability for the server that backs it, and one small, uniform request
protocol drives every server the same way.

- **Everything reachable is a named channel.** A disk file, a device, a
  network connection, a window on the screen — each is reached by walking a
  path to the server that owns it and speaking the same protocol. Mounting
  is not a disk operation; it is binding a server's endpoint capability to a
  prefix in a namespace. The VFS server is a router: longest-prefix match on
  the path, strip the prefix, forward the remainder (with the caller's reply
  capability) to the owning server.
- **A small, uniform file protocol.** Every server — on-disk FS, device,
  a `/proc`-style generator, a GPU — answers the same handful of requests:
  walk a path, open (returning a per-session capability), read / write,
  stat, close. (Plan 9's 9P is the model: a few verbs, no special cases.)
  Because the protocol is uniform, a namespace entry can be *anything* that
  implements it — a file on an SSD, a synthetic file computed on demand, a
  filesystem that fetches over the network. Nothing above notices.
- **Per-process namespaces.** There is no root shared by the whole system.
  The root task hands each process a namespace assembled from the mounts it
  should see — read-only system libraries here, a network endpoint there, a
  scratch directory it may write. A process cannot reach what is not in its
  namespace, because it holds no capability to that server's endpoint.
  Sandboxing needs no `chroot` and no ambient-authority checks; the ports
  simply are not there.
- **Asynchronous, zero-copy I/O.** Bulk data never rides inside IPC
  messages. A read is: hand the server a capability to a shared buffer, post
  a short request, and continue; the FS and block drivers DMA straight into
  that buffer and drop a notification when it lands. The model is
  io_uring / completion ports, not a blocking `read()` — asynchrony is the
  default, not an add-on.

---

## The system library stack

Foreign source expects `malloc`, `printf`, `pthread_create`. preon supplies
those through a layered stack — thinnest at the bottom, each layer one step
closer to kernel IPC — so the same `printf` works whether the caller is a
native program or a ported Linux binary.

- **`libsys`** — the floor: raw `svc` stubs for the capability syscalls and
  the IPC primitives. A few kilobytes, no policy. Everything is built on it;
  if the syscall ABI changes, only `libsys` moves.
- **`libpreon_{io,mem,thread}`** — thin shims that give POSIX-shaped calls a
  home: `write` / `read` become IPC to the VFS, `mmap` / `brk` become IPC to
  the memory server, `pthread_create` becomes a thread syscall. This is where
  "the Unix call" turns into "the preon message".
- **A ported `libc`** (musl or newlib) — the standard C surface with its
  Linux syscall layer swapped for calls into `libpreon_*`. Programs link it
  and see a normal C library.
- **`libc++` / `libunwind`** — ride unchanged on top: the C++ standard
  library depends only on `libc` and the allocator, never on the OS, so it
  builds for preon with no porting. Unwinding reuses the same register /
  context format the kernel already defines for exception entry.

Native servers and drivers skip most of this: they are `#![no_std]` Rust over
`core` + `alloc` (a `#[global_allocator]` that pulls pages from the kernel)
talking straight to `libsys`. The heavy `libc` / `libc++` stack is only for
foreign or POSIX-style software.

Linking is **static first** — a self-contained ELF the loader can jump to
before any FS server exists (the root task especially). A userspace dynamic
linker (`ld.so`) that maps shared objects and resolves relocations is a later
convenience, not a bring-up dependency.

---

## Toolchain and language support

preon aims to be a first-class compiler target, not a fork of a compiler.

- **Now — a local target.** A custom target spec (`aarch64-unknown-preon`)
  plus Rust's `-Z build-std` compiles `core` / `alloc` / `std` against
  preon's own `libc`. That is enough to build the whole system while the
  syscall ABI is still moving; nothing needs upstreaming yet.
- **Later — upstream the target.** Once the ABI settles, teach **LLVM** the
  `preon` OS triple (a small, well-scoped patch: an `OSType` entry, ELF
  defaults, tests). That single change lights up the whole LLVM front-end
  fleet — clang, rustc, zig, and anything else that lowers through LLVM can
  then emit preon binaries. Then teach **rustc** the target spec and an `std`
  backend (the `libc` crate and `library/std/src/sys`), moving preon from
  Rust's Tier 3 (target in-tree) toward Tier 2 (`rustup target add`).
- **Codegen is not the runtime.** A compiler that emits preon binaries is not
  the same as software that runs: each language still needs its runtime wired
  to preon (Rust `std`, the C `libc`, Zig's own syscall layer). Codegen is the
  paperwork; the runtime shims are the work — exactly the volume the porting
  toolkits (see the README) exist to absorb.

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
