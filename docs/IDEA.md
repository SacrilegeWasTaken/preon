  Layer 1: Microkernel TCB (minimal)
  ---------------------------------
  - Scheduler            — preemptive, per-CPU run queues
  - MMU manager          — page tables, address spaces
  - IPC                  — sync send/recv, endpoints, async signals
  - Capability system    — kernel handle table per process, rights
  - IRQ router           — deliver hardware IRQs to subscribed processes
  - (no drivers, no VFS, no FS, no networking)

  Layer 2: Universal Object Model (kernel-managed objects)
  ---------------------------------
  - Process              — address space + capability table + threads
  - Thread               — CPU state, stack, scheduler entity
  - AddressSpace         — TTBR0 + page table tree
  - MemoryRegion         — physical pages + permissions, mappable
  - Endpoint             — synchronous IPC rendez-vous point
  - Notification         — async signal bitfield
  - Capability           — typed pointer to kernel object with rights mask

  Layer 3: Userspace Servers (drivers + services)
  ---------------------------------
  - Device drivers       — UART, virtio-blk, virtio-net, gpu, input
  - File-system servers  — initramfs, on-disk FS per type
  - Network stack        — TCP/IP, ARP
  - VFS / namespace      — path resolver, mount table

  Layer 4: ABI Personalities
  ---------------------------------
  - Native runtime       — capability-based, thin shim, seL4-like syscalls
  - Linux runtime        — POSIX translation (open/read/fork/...) into IPC + caps
  - Endpoint + Notification — типы IPC объектов (sync vs async).
