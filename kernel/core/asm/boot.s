/*
 *
 *  Boot stub (.text.boot) — first instructions the loader jumps to.
 *
 *  Entry contract (see docs/boot_policy.md): AArch64, EL1 or EL2, MMU off,
 *  little-endian, x0 = PA of the FDT blob. We drop to EL1 if needed, set up
 *  a stack, clear BSS, build trampoline page tables, enable the MMU, and
 *  branch to the Rust `kmain` at its upper-half image VA.
 *
 *  Register conventions held across this stub:
 *    x20 — saved FDT pointer (preserved until the jump to kmain)
 *    x0/x1 — scratch for every short build step
 *    x16 — branch target / VA-base scratch near the end
 *
 */

.section .text.boot
.global _start

// SPSR value used for the EL2->EL1 return: DAIF masked (D,A,I,F = 1) and
// M[3:0] = 0b0101 (EL1h, i.e. EL1 using SP_EL1). 0x3c5 = 0x3c0 | 0x5.
.equ SPSR_EL1H, 0x3c5
// HCR_EL2.RW (bit 31): execution state below EL2 is AArch64.
.equ HCR_RW,     (1 << 31)

/*
 *
 *  Exception-level identification
 *
 */

_start:
    // Preserve the FDT pointer; x0 is about to be clobbered by the EL probe.
    mov     x20, x0

    // CurrentEL holds the EL in bits [3:2]; shift down to a plain 1/2/3.
    mrs     x0, CurrentEL
    lsr     x0, x0, #2

    // Already at EL1 — skip the EL2 drop and go straight to EL1 init.
    cmp     x0, #1
    b.eq    .Lel1

    // At EL2 — configure EL2 and `eret` down into EL1.
    cmp     x0, #2
    b.eq    .Lel2

    // EL0 or EL3: outside our boot contract — park forever.
    b       unsupported_el

/*
 *
 *  EL2 -> EL1 drop
 *
 */

.Lel2:
    // Force AArch64 at EL1 (HCR_EL2.RW=1) before the eret commits the state.
    mov     x0, #HCR_RW
    msr     hcr_el2, x0
    isb

    // CNTHCTL_EL2 EL1PCTEN+EL1PCEN = 1: let EL1 read the physical counter and
    // program the physical timer without trapping to EL2.
    mrs     x0, cnthctl_el2
    orr     x0, x0, #3
    msr     cnthctl_el2, x0

    // Zero the virtual-counter offset so EL1's virtual time tracks physical.
    msr     cntvoff_el2, xzr

    // CPTR_EL2=0: do not trap FP/SIMD (or other CPACR-class accesses) to EL2.
    msr     cptr_el2, xzr

    isb

    // Stage the return: SPSR_EL2 selects EL1h with interrupts masked...
    mov     x0, #SPSR_EL1H
    msr     spsr_el2, x0

    // ...and ELR_EL2 is the EL1 entry label we resume at after eret.
    adr     x0, .Lel1
    msr     elr_el2, x0

    // Drop to EL1: PSTATE <- SPSR_EL2, PC <- ELR_EL2.
    eret

/*
 *
 *  EL1 init — stack, FP/SIMD, zero BSS and trampoline page-table buffers
 *
 */

.Lel1:
    // Assert the loader honoured the contract: SCTLR_EL1.M (bit 0) must be 0.
    // If the MMU is already on we cannot trust our flat view — park.
    mrs     x0, sctlr_el1
    tst     x0, #1
    b.ne    bad_boot_mmu

    // Point SP at the top of the linker-reserved stack (PA; MMU still off).
    ldr     x0, =__stack_top_pa
    mov     sp, x0

    // CPACR_EL1.FPEN (bits [21:20] = 0b11): allow FP/SIMD at EL1/EL0 so the
    // Rust code (which may emit SIMD) doesn't trap. isb to apply before use.
    mrs     x0, cpacr_el1
    orr     x0, x0, #(3 << 20)
    msr     cpacr_el1, x0
    isb

    // Zero the kernel .bss [__bss_start_pa, __bss_end_pa). Operate on PA: the
    // image runs at its load PA until the MMU comes up further down.
    ldr     x0, =__bss_start_pa
    ldr     x1, =__bss_end_pa

.Lbss_loop:
    // Walk 8 bytes at a time until the cursor reaches the end (b.hs = >=).
    cmp     x0, x1
    b.hs    .Lbss_done

    // Store xzr (64 zero bits) and post-increment the cursor by 8.
    str     xzr, [x0], #8
    b       .Lbss_loop

.Lbss_done:
    // Zero every static trampoline page-table buffer in one sweep. The range
    // spans the first buffer (__idmap_l0) through the last (__device_l3). The
    // end anchor MUST track whichever buffer the linker places last, or stale
    // RAM leaks in as bogus descriptors on hardware that doesn't pre-zero.
    ldr     x0, =__idmap_l0     // first trampoline-table buffer
    ldr     x1, =__device_l3    // last trampoline-table buffer
    add     x1, x1, #0x1000     // advance past its 4 KiB page -> exclusive end

.Lbootbss_loop:
    cmp     x0, x1
    b.hs    .Lbootbss_done
    str     xzr, [x0], #8
    b       .Lbootbss_loop

.Lbootbss_done:

/*
 *
 *  MAIR_EL1 / TCR_EL1 — memory attributes and translation control
 *
 *  These must mirror the Rust definitions (kernel_mm::attrs::MAIR_VALUE and
 *  kernel_mm::tcr) so trampoline and runtime tables agree on attribute slots.
 *
 */

    // MAIR_EL1 = 0x0000_0000_0400_44FF, four attribute slots:
    //   Attr0 = 0xFF  Normal, Inner+Outer write-back, read/write-allocate
    //   Attr1 = 0x44  Normal, Inner+Outer non-cacheable
    //   Attr2 = 0x00  Device-nGnRnE (strictest)
    //   Attr3 = 0x04  Device-nGnRE  (UART, timers)
    movz    x0, #0x44FF                 // low 16: Attr1:Attr0
    movk    x0, #0x0400, lsl #16        // next 16: Attr3:Attr2
    msr     mair_el1, x0

    // TCR_EL1 = 0x0000_0025_B550_3510. Key fields:
    //   T0SZ=T1SZ=16  -> 48-bit VA for both TTBR0 and TTBR1
    //   TG0=4KiB, TG1=4KiB granule
    //   IRGN/ORGN 0/1 = write-back cacheable page-table walks
    //   SH0/SH1 = inner shareable
    //   IPS=0b101 -> 48-bit physical address size
    movz    x0, #0x3510                 // bits [15:0]
    movk    x0, #0xB550, lsl #16        // bits [31:16]
    movk    x0, #0x0025, lsl #32        // bits [47:32] (IPS etc.)
    msr     tcr_el1, x0

    // Make MAIR/TCR visible to the table walker before we build/install tables.
    isb

/*
 *
 *  TTBR0 trampoline tables — identity map (VA == PA) of the low region
 *
 *  Just enough flat mapping to survive the SCTLR.M flip while still executing
 *  from PA: the UART page (so early prints keep working) and one RAM block
 *  covering the running image. Torn down once the kernel switches fully to
 *  the upper half. Descriptor low bits: 0b11 = table/page, 0b01 = block.
 *
 */

    // L0[0] -> L1 table. Index 0 because identity VAs start at 0; "| 3" tags
    // it as a valid table descriptor pointing at the next level.
    ldr     x0, =__idmap_l0
    ldr     x1, =__idmap_l1
    orr     x1, x1, #3
    str     x1, [x0]

    // L1[0] -> L2 table covering the 0..1 GiB identity range (holds the UART).
    ldr     x0, =__idmap_l1
    ldr     x1, =__idmap_l2_uart
    orr     x1, x1, #3
    str     x1, [x0]

    // L1[1] -> L2 table covering the 1..2 GiB range (holds the RAM block).
    // x0 still points at __idmap_l1; entry 1 is at byte offset 8.
    ldr     x1, =__idmap_l2_ram
    orr     x1, x1, #3
    str     x1, [x0, #8]

    // L2[72] -> L3 table. 72 = bits[29:21] of PA 0x0900_0000 (UART), i.e.
    // the 72nd 2 MiB slot inside the first GiB.
    ldr     x0, =__idmap_l2_uart
    ldr     x1, =__idmap_l3_uart
    orr     x1, x1, #3
    str     x1, [x0, #(72*8)]

    // L3[0] = UART leaf, raw descriptor 0x0060_0000_0900_040F:
    //   PA[47:12]=0x09000 (0x0900_0000) | AF=1 | SH=00 non-shareable
    //   | AP=00 EL1 RW | AttrIndx=3 (Device-nGnRE) | bits[1:0]=11 page
    //   | UXN=1,PXN=1 (execute-never). No "| 3": bits already set.
    ldr     x0, =__idmap_l3_uart
    ldr     x1, =0x006000000900040F
    str     x1, [x0]

    // L2[0] = 2 MiB block, descriptor 0x0040_0000_4000_0701:
    //   PA=0x4000_0000 | AF=1 | SH=11 inner | AP=00 EL1 RW | AttrIndx=0
    //   (Normal WB) | bits[1:0]=01 block | UXN=1 (EL0 NX), PXN=0 (EL1 exec).
    //   EL1-executable because the image runs from here pre-handoff.
    ldr     x0, =__idmap_l2_ram
    ldr     x1, =0x0040000040000701
    str     x1, [x0]

/*
 *
 *  TTBR1 trampoline tables — upper-half kernel map
 *
 *  Coarse upper-half view that lets the first kmain instructions execute and
 *  print before the fine-grained runtime map (kernel_mm::kernel_map) is built.
 *  Three regions, each a distinct L0 slot (L0 index = VA bits[47:39]):
 *    [256] linear  0xFFFF_8000_0000_0000   [384] device  0xFFFF_C000_0000_0000
 *    [511] image   0xFFFF_FFFF_8000_0000
 *
 */

    // --- Linear region: L0[256] -> L1 table ---
    ldr     x0, =__kernel_l0
    ldr     x1, =__kernel_l1
    orr     x1, x1, #3
    str     x1, [x0, #(256*8)]

    // --- Image region: L0[511] -> image L1 table ---
    ldr     x0, =__kernel_l0
    ldr     x1, =__image_l1
    orr     x1, x1, #3
    str     x1, [x0, #(511*8)]

    // image L1[510] = 1 GiB block at PA 0x4000_0000. 510 = bits[38:30] of the
    // image base 0xFFFF_FFFF_8000_0000. Same Normal/RW/EL1-exec descriptor as
    // the identity RAM block — the image is executed from here post-flip.
    ldr     x0, =__image_l1
    ldr     x1, =0x0040000040000701
    str     x1, [x0, #(510*8)]

    // linear L1[1] = 1 GiB block at PA 0x4000_0000 -> VA 0xFFFF_8000_4000_0000.
    // 1 = bits[38:30] of that VA. One GiB is enough RAM for the trampoline;
    // the runtime map later covers all of physical memory.
    ldr     x0, =__kernel_l1
    ldr     x1, =0x0040000040000701
    str     x1, [x0, #8]

    // --- Device region: L0[384] -> device L1 -> L2 -> L3, page-granular ---
    // Mirrors the identity UART chain but hangs off the upper-half device base
    // so the UART VA survives once TTBR0 is torn down. L2/L3 indices (72,0)
    // equal the identity ones: flat offset preserves PA bits[29:0].

    // L0[384] -> device L1 table. 384 = bits[47:39] of 0xFFFF_C000_0000_0000.
    ldr     x0, =__kernel_l0
    ldr     x1, =__device_l1
    orr     x1, x1, #3
    str     x1, [x0, #(384*8)]

    // device L1[0] -> device L2 table (UART sits in the first GiB).
    ldr     x0, =__device_l1
    ldr     x1, =__device_l2
    orr     x1, x1, #3
    str     x1, [x0]

    // device L2[72] -> device L3 table. 72 = bits[29:21] of PA 0x0900_0000.
    ldr     x0, =__device_l2
    ldr     x1, =__device_l3
    orr     x1, x1, #3
    str     x1, [x0, #(72*8)]

    // device L3[0] = UART leaf, identical descriptor to the identity map.
    ldr     x0, =__device_l3
    ldr     x1, =0x006000000900040F
    str     x1, [x0]


    // __kernel_l2 stays zeroed/unused: the trampoline only needs block-level
    // coverage, so no L2 table is hung under the linear region here.

/*
 *
 *  Install translation roots and enable the MMU
 *
 */

    // TTBR0_EL1 = identity root (low half), TTBR1_EL1 = kernel root (upper).
    ldr     x0, =__idmap_l0
    msr     ttbr0_el1, x0
    ldr     x0, =__kernel_l0
    msr     ttbr1_el1, x0

    // Ordering ceremony before turning translation on:
    //   dsb ish  — publish all the table stores to the walker
    //   tlbi vmalle1is — drop any stale EL1 TLB entries (inner-shareable)
    //   dsb ish  — wait for the invalidate to complete
    //   isb      — flush the pipeline so the next fetch sees a clean state
    dsb     ish
    tlbi    vmalle1is
    dsb     ish
    isb

    // Set SCTLR_EL1.M (bit 0): translation is now live. From the next
    // instruction, every fetch/access goes through the tables we just built.
    mrs     x0, sctlr_el1
    orr     x0, x0, #1
    msr     sctlr_el1, x0
    isb

    // Completely unknown thing somehow fixing the DataAbortSameEl exception
    ldr     x9, =__stack_top
    mov     sp, x9

/*
 *
 *  Trampoline jump into the upper-half Rust entry point
 *
 */

    // Rebuild kmain's argument: the FDT is reachable through the linear map,
    // so OR the saved PA with the linear base 0xFFFF_8000_0000_0000.
    mov     x0, x20
    movz    x16, #0xFFFF, lsl #48       // x16 = 0xFFFF_0000_0000_0000
    movk    x16, #0x8000, lsl #32       // x16 = 0xFFFF_8000_0000_0000 (linear base)
    orr     x0, x0, x16                 // x0 = FDT as a linear-map VA

    // Jump to kmain at its upper-half image VA (the linker resolves =kmain to
    // the image address); br leaves the low PC range for good.
    ldr     x16, =kmain
    br      x16

/*
 *
 *  Park loops — unreachable in nominal boot; each spins in low-power wfe
 *
 */

// Generic halt (currently unreferenced; kept as a catch-all stop).
.Lhalt:
    wfe
    b       .Lhalt

// Entered when CurrentEL is neither EL1 nor EL2 (contract violation).
unsupported_el:
.Lunsupported:
    wfe
    b       .Lunsupported

// Entered when the loader handed us the MMU already enabled.
bad_boot_mmu:
.Lmmu:
    wfe
    b       .Lmmu
