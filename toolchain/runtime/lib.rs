#![no_std]

// Placeholder for the unnamed userspace runtime — the equivalent of a libc
// for processes running on top of the kernel.
//
// Eventually provides:
//   - `_start` entry point that receives argv/envp/auxv from the kernel,
//     sets up the userspace stack frame, and calls into `main`.
//   - Syscall wrappers: one Rust function per kernel syscall, lowering
//     to `svc #imm16` with the canonical x0..x7 argument layout.
//   - `#[global_allocator]` so `alloc::Box`, `Vec`, `String` work in
//     userspace; backed by an `mmap`-style syscall against the kernel.
//   - Userspace `panic_handler` — prints through a `write` syscall and
//     exits with a non-zero status instead of busy-looping.
//   - Compiler-required intrinsics (`memcpy`, `memset`, `memmove`) when
//     LLVM lowers Rust operations to them and we have no libc.
//
// Linked in by every unnamed userspace binary via a small target spec
// (`toolchain/target/aarch64-unknown-unnamed.json`).
