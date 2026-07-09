# Formal verification with Kani

Preon uses [Kani](https://model-checking.github.io/kani/) to model-check the
kernel's **pure logic cores**. This document states honestly what that buys us,
what it cannot touch, how to run it, and which harnesses exist.

---

## What Kani is (and is not) here

Kani is a **bounded model checker** for Rust, backed by CBMC. For a given
harness it symbolically explores *all* inputs (within the declared bounds) and
proves properties — no panic, no arithmetic overflow, no out-of-bounds, plus
any `assert!` postconditions we write. Where inputs are unbounded integers, the
proof is exhaustive over the whole value space, not a sample.

This is **not** seL4-style whole-kernel functional verification. It does not
prove the kernel "correct." It proves that specific, pure functions satisfy
specific properties. That is exactly where silent, hard-to-test corruption bugs
live (index math, bit encoding, address translation), so the payoff is high for
the surface it covers.

### Out of scope — permanently, by construction

CBMC cannot see hardware or assembly, so these are **not** verifiable with Kani
and never will be:

- `boot.s`, `ventry.s`, `secondary.s`, and the `read_sysreg!` / `write_sysreg!`
  macros — everything behind `asm!` / `global_asm!` is opaque.
- `mmu::switch_ttbr1`, `mmu::disable_ttbr0` — inline asm.
- MMIO: `kernel_builtin::mmio::Reg`, the PL011 `uart` driver — `read_volatile` /
  `write_volatile` with no device model.
- The MMU actually walking the tables. Kani proves the *descriptor arithmetic*
  that builds an entry; it cannot prove the hardware reads it back the same way.
- SMP timing / memory ordering (`SpinLock`, `fence`, atomics). Kani's
  concurrency support is limited; this is not its strength.

### In scope — the verifiable surface

Pure, total functions over integers and bit-fields:

| Module | Property under test | Status |
|---|---|---|
| `kernel_arch::mm` (`Level`) | index/shift math tiles VA bits [47:12]; `from_index` total; `next_level` progression + L3 panic | **done** |
| `kernel_arch::reg` | `ESR` field accessors (`EC`/`IL`/`ISS`) tile bits [31:0] | **done** |
| `kernel_arch::exceptions` | `FaultStatus::from_dfsc`, `ExceptionClass::from_ec` totality | **done** |
| `kernel_mm::layout` | PA↔VA round-trips; region-window containment; region ordering | **done** |
| `kernel_mm::page_table` | `Entry::{block,page,table}` → `output_addr` round-trip; block/table disjoint; invalid inert | **done** |
| `kernel_mm::buddy` | full allocator core: free-list ops, `alloc` split, `free` coalesce (inverse of `alloc`), `free_range` carve — all with mass conservation | **done** |

The buddy allocator's whole core is verified: `push_front` / `unlink` /
`pop_front` list integrity, `alloc` (scan + split + `free_frames`), `free`
(coalesce up, head = lower buddy), and `free_range` (greedy aligned carve).
`alloc_free_round_trip` proves `free ∘ alloc = id`. Verification is over a
**bounded model** (a 4-frame backing store, orders ≤ 2, `#[kani::unwind(12)]`):
exhaustive for those configs, where every split/coalesce/carve branch is
reachable, but not a proof for arbitrary `N`. The `MAX_ORDER-1` cap path and
ranges larger than `2^10` frames are correct by inspection, outside the model.

### Deliberately not covered

- `attrs::MAIR_VALUE`, `tcr::TcrConfig::build` — compile-time constants; pin
  with `const _: () = assert!(...)` or a unit test, not Kani.
- `types::VirtAddrSize`, `cpu::types::Mpidr` — thin masking/arithmetic; low
  payoff. Easy to add if a bug ever points here.
- `TrapFrame` field offsets vs the hand-written offsets in `ventry.s` — a real
  invariant, but it cross-checks *assembly*, so it belongs in a
  `const _: () = assert!(offset_of!(TrapFrame, elr_el1) == 31*8 + 8)` static
  assertion, not a Kani harness.

---

## Where the harnesses live

Each harness is a `#[cfg(kani)]` module appended to the file it verifies (it
needs access to that module's private items). The `kani` cfg is set **only** by
`cargo kani`, so a normal `cargo build` compiles none of it — zero impact on the
kernel image.

Because normal builds never set `kani`, each crate carrying harnesses declares
the cfg so `cargo build` stays warning-clean:

```toml
[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ['cfg(kani)'] }
```

Add those two lines to any further crate that gains `#[cfg(kani)]` harnesses.

---

## Running it

Kani is **not** part of the Nix toolchain — it ships its own CBMC and rustc, so
install it out of band:

```
cargo install --locked kani-verifier
cargo kani setup
```

Then:

```
make verify        # cargo kani -p kernel_arch -p kernel_mm, host machine model
```

### The forced-target snag

The workspace's `.cargo/config.toml` pins `build.target =
"aarch64-unknown-none"` for the kernel build. Kani analyses on the **host**
machine model and needs `std` scaffolding for its harness runner, so `make
verify` overrides the target with the detected host triple via
`CARGO_BUILD_TARGET`.

Two things to confirm on the **first** `cargo kani` run — neither is verified
here because Kani is not yet installed in this environment:

1. **Edition 2024.** The crates are `edition = "2024"`; Kani pins its own rustc,
   so use a Kani release new enough to accept it.
2. **Target override.** If Kani rejects `CARGO_BUILD_TARGET`, the fallback is a
   dedicated `verification/` crate that is **not** under the root
   `.cargo/config.toml`, depending on the kernel crates by path and hosting the
   harnesses there instead of inline.

---

## CI sketch

Kani ships an official GitHub Action. A minimal job:

```yaml
name: verify
on: [push, pull_request]
jobs:
  kani:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: model-checking/kani-github-action@v1
        with:
          args: "-p kernel_arch -p kernel_mm"
        env:
          CARGO_BUILD_TARGET: x86_64-unknown-linux-gnu
```

Not committed as a workflow yet — the repo has no `.github/` tree. Add it once
`make verify` is green locally.

---

## Gotchas

- **Never compare `&str` / slice *content* (`==`) over symbolically-selected
  data.** CBMC lowers it to a `memcmp` builtin; when the pointer is symbolic
  (e.g. a `&'static str` chosen by a `match`), it can't bound the compare length
  and unwinds the loop over the whole rodata string table — thousands of
  iterations, no termination. Symptom: `Unwinding loop memcmp.0 iteration NNNN`
  climbing without end. Compare `.as_ptr()` + `.len()` (same static → identical
  fat pointer), or just discriminants. `.is_empty()` / `.len()` are fine —
  they're integer checks, not memcmp.
- If a harness genuinely needs a bounded loop, cap it with `#[kani::unwind(N)]`
  rather than letting CBMC guess.

## Harness inventory

### `kernel_arch::mm` (`kernel/arch/mm.rs`)

- `level_indices_tile_va` — the four 9-bit level indices reassemble VA bits
  [47:12] exactly (no gap/overlap); guards `index_shift`.
- `from_index_is_total` — `Level::from_index` never panics for any `u8` and
  decodes the low two bits correctly.
- `next_level_progresses` — L0 → L1 → L2 → L3 without panic.
- `next_level_l3_panics` (`#[kani::should_panic]`) — L3 has no next level.

### `kernel_arch::reg` (`kernel/arch/reg.rs`)

- `esr_fields_tile_low_word` — `EC` / `IL` / `ISS` reassemble bits [31:0] of
  `ESR_EL1` exactly; guards the shift/mask constants.

### `kernel_arch::exceptions` (`kernel/arch/exceptions.rs`)

- `fault_status_total` — every 6-bit fault code classifies without panic;
  levelled classes (raw ≤ 0x0F) are exactly those with a level.
- `exception_class_total` — `from_ec` total over every `EC`, description
  non-empty.

### `kernel_mm::layout` (`kernel/mm/layout.rs`)

- `linear_round_trip` — `linear_va_to_pa ∘ pa_to_linear_va == id` and the
  forward image stays inside `[LINEAR_BASE, DEVICE_BASE)`.
- `device_map_in_window` — a device VA stays inside `[DEVICE_BASE, IMAGE_BASE)`.
- `image_map_no_underflow` — `image_va_to_pa` never underflows, result
  ≥ `IMAGE_PA_BASE`.

### `kernel_mm::page_table` (`kernel/mm/page_table.rs`)

- `page_encoding_round_trip` / `table_encoding_round_trip` /
  `block_encoding_round_trip` — `output_addr` recovers the input PA; type-bit
  predicates read back correctly (block XOR table/page falls out of these).
- `invalid_entry_is_inert` — `Entry::invalid()` has no valid bit, no address.

### `kernel_mm::buddy` (`kernel/mm/buddy.rs`)

Address math:
- `pfn_pa_round_trip` — `pfn_of` and `pa_of` are mutual inverses in range.
- `buddy_is_involution` — `buddy_pfn` is an involution (buddy of buddy = self).

Free-list ops:
- `push_front_links_head` — two pushes leave a correctly LIFO-linked list.
- `unlink_head` / `unlink_tail` / `unlink_middle` — removal relinks all four
  `(prev, next)` combinations; head removal moves `free_area`.
- `push_pop_round_trip` — `push_front` then `pop_front` returns the frame and
  empties the list.

`alloc`:
- `alloc_oom` — every list empty ⇒ `None` for any order.
- `alloc_splits_correctly` — order-2 block, symbolic request `o ≤ 2`: base
  returned Allocated at `o`, each split deposits the upper buddy half,
  `free_frames -= 2^o`.

`free`:
- `free_no_coalesce` — non-free buddy ⇒ block just lands in `free_area[order]`.
- `free_upper_buddy_coalesces` — freeing the upper buddy merges with head at
  the lower PFN.
- `alloc_free_round_trip` — `free ∘ alloc = id` with 0/1/2 coalesce levels.

`free_range` (init carve):
- `free_range_full_is_one_block` — aligned power-of-two range ⇒ one maximal
  block.
- `free_range_unaligned_tail` — `[1,4)` carves into order-0 @ 1 + order-1 @ 2.
- `free_range_conserves_mass` — over every sub-range of the 4-frame space,
  `free_frames` grows by exactly `count`.
