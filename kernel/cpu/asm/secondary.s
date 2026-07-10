.section .text

.global secondary_entry
.type secondary_entry, %function

.extern secondary_cpu_main
.extern install_current_cpu_local
.extern vector_table

// Secondary CPU entry, jumped to by PSCI CPU_ON.
//
// On entry x0 holds the SecondaryBootData pointer the primary passed as
// the PSCI ctx argument. PSCI guarantees EL1 with interrupts masked, but
// not the FP/SIMD trap or VBAR_EL1 state, so configure those before any
// Rust code runs.


secondary_entry:
    mov  x19, x0

    mrs  x1, cpacr_el1
    orr x1, x1, #(3<<20)
    msr cpacr_el1, x1
    isb

    movz x0, #0x44FF
    movk x0, #0x0400, lsl #16
    msr mair_el1, x0
    
    movz x0, #0x3510
    movk x0, #0xB550, lsl #16
    movk x0, #0x0025, lsl #32
    msr tcr_el1,  x0
    isb

    ldr  x0, =__idmap_l0
    msr  ttbr0_el1, x0
    ldr  x0, [x19, #16]
    msr  ttbr1_el1, x0

    dsb ish
    tlbi vmalle1is
    dsb ish
    isb
    
    mrs  x0, sctlr_el1
    orr x0, x0, #1
    msr sctlr_el1, x0
    isb

    ldr  x16, =secondary_high
    br   x16

secondary_high:
    movz x18, #0xFFFF, lsl #48
    movk x18, #0x8000, lsl #32
    orr  x19, x19, x18

    ldr  x1, [x19, #8]
    mov sp, x1
    ldr  x0, [x19, #0]
    
    bl install_current_cpu_local
    
    ldr  x1, =vector_table
    msr vbar_el1, x1
    isb
    mov  x0, x19
    
    bl secondary_cpu_main
