# toolchain

Everything used to build code **for** unnamed (the OS has no proper name
yet — the kernel is `preon`), as opposed to the OS itself.

## Layout

- `target/` — rustc/LLVM target specification JSON files. The kernel is
  built for `aarch64-unknown-none` (a stock target); userspace processes
  will be built for `aarch64-unknown-unnamed`, a custom target spec living
  here. The JSON tells rustc and LLVM the data layout, linker flavor,
  panic strategy, atomic widths, and any pre/post-link arguments.

- `runtime/` — `toolchain_runtime`, the Rust crate every userspace
  process links against. Equivalent in spirit to `libc` on a Unix:
  provides `_start`, syscall wrappers, an allocator hook, a userspace
  `panic_handler`, and minimal codegen stubs (`memcpy`, `memset`, etc.)
  that LLVM lowers operations to.

## Not in scope

- A C compiler. We use upstream `clang` if and when we need to compile
  C for unnamed — there is no plan to write our own front end.
- Forking LLVM. The custom target spec is the only LLVM-side artifact;
  it slots into stock `rustc`.

## When this becomes real

Only after Phase 6 (userspace bring-up) starts. Until then everything
here is a placeholder marking intent.
