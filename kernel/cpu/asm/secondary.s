/*
 *
 *  Secondary-CPU entry (.text) — landed on by PSCI CPU_ON.
 *
 *  boot.s's MMU-enable sequence in miniature, run by every non-boot CPU. On
 *  entry x0 = PA of the SecondaryBootData the primary passed as the PSCI ctx
 *  argument. PSCI guarantees EL1 with interrupts masked, but not the FP/SIMD
 *  trap or VBAR_EL1, so we configure those before any Rust code runs.
 *
 *  SecondaryBootData layout (see kernel_cpu::smp::SecondaryBootData):
 *    [0] percpu_offset    [8] stack_top    [16] ttbr1_root
 *
 *  Register conventions:
 *    x19 — SecondaryBootData pointer (PA on entry, rebased to its linear VA in
 *          the upper half); preserved across the MMU flip
 *    x18 — linear-map base scratch
 *
 */

.section .text

.global secondary_entry
.type secondary_entry, %function

.extern secondary_cpu_main
.extern install_current_cpu_local
.extern vector_table

secondary_entry:
    // Stash the boot-data pointer; x0 is reused as scratch below.
    mov  x19, x0

    // CPACR_EL1.FPEN = 0b11: allow FP/SIMD at EL1/EL0 so Rust code won't trap.
    mrs  x1, cpacr_el1
    orr x1, x1, #(3<<20)
    msr cpacr_el1, x1
    isb

    // MAIR_EL1 / TCR_EL1 must match boot.s exactly (and kernel_mm::attrs/tcr),
    // so this CPU agrees with the primary on attribute slots and the 48-bit
    // VA/PA translation regime.
    movz x0, #0x44FF
    movk x0, #0x0400, lsl #16
    msr mair_el1, x0

    movz x0, #0x3510
    movk x0, #0xB550, lsl #16
    movk x0, #0x0025, lsl #32
    msr tcr_el1,  x0
    isb

    // TTBR0 = shared identity trampoline (keeps PA execution valid across the
    // SCTLR.M flip); TTBR1 = the kernel root the primary built, taken from
    // boot_data.ttbr1_root ([x19, #16]).
    ldr  x0, =__idmap_l0
    msr  ttbr0_el1, x0
    ldr  x0, [x19, #16]
    msr  ttbr1_el1, x0

    // Publish the table stores, drop stale EL1 TLB entries, then serialize.
    dsb ish
    tlbi vmalle1is
    dsb ish
    isb

    // SCTLR_EL1.M = 1: translation on. The next fetch runs through the tables.
    mrs  x0, sctlr_el1
    orr x0, x0, #1
    msr sctlr_el1, x0
    isb

    // Jump to the upper-half continuation at its image VA (leaves the low PC).
    ldr  x16, =secondary_high
    br   x16

secondary_high:
    // Rebase the boot-data pointer into the linear map (OR in 0xFFFF_8000...),
    // since the identity view is about to become unnecessary and the bare PA is
    // not part of the upper-half map.
    movz x18, #0xFFFF, lsl #48
    movk x18, #0x8000, lsl #32
    orr  x19, x19, x18

    // Install this CPU's stack from boot_data.stack_top ([x19, #8]).
    ldr  x1, [x19, #8]
    mov sp, x1

    // Install this CPU's per-CPU area: x0 = boot_data.percpu_offset ([x19, #0]),
    // which install_current_cpu_local writes into TPIDR_EL1.
    ldr  x0, [x19, #0]
    bl install_current_cpu_local

    // Point VBAR_EL1 at the shared vector table before any trap can fire.
    ldr  x1, =vector_table
    msr vbar_el1, x1
    isb

    // Hand off to Rust with x0 = &SecondaryBootData.
    mov  x0, x19
    bl secondary_cpu_main
