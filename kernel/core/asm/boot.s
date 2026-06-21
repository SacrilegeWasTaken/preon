.section .text.boot
.global _start

.equ SPSR_EL1H, 0x3c5
.equ HCR_RW,     (1 << 31)

_start:
    // preserve DTB pointer across init
    mov     x20, x0

    // identify current exception level
    mrs     x0, CurrentEL
    lsr     x0, x0, #2

    cmp     x0, #1
    b.eq    .Lel1

    cmp     x0, #2
    b.eq    .Lel2

    b       unsupported_el

.Lel2:
    // EL1 executes AArch64
    mov     x0, #HCR_RW
    msr     hcr_el2, x0
    isb

    // allow EL1 physical timer access
    mrs     x0, cnthctl_el2
    orr     x0, x0, #3
    msr     cnthctl_el2, x0

    // remove virtual timer offset
    msr     cntvoff_el2, xzr

    // disable FP traps from EL2
    msr     cptr_el2, xzr

    isb

    // return into EL1h
    mov     x0, #SPSR_EL1H
    msr     spsr_el2, x0

    adr     x0, .Lel1
    msr     elr_el2, x0

    eret

.Lel1:
    // boot protocol requires MMU disabled
    mrs     x0, sctlr_el1
    tst     x0, #1
    b.ne    bad_boot_mmu

    // establish kernel stack ASAP
    ldr     x0, =__stack_top_pa
    mov     sp, x0

    // enable FP/SIMD
    mrs     x0, cpacr_el1
    orr     x0, x0, #(3 << 20)
    msr     cpacr_el1, x0
    isb

    // clear BSS
    ldr     x0, =__bss_start_pa
    ldr     x1, =__bss_end_pa

.Lbss_loop:
    cmp     x0, x1
    b.hs    .Lbss_done

    str     xzr, [x0], #8
    b       .Lbss_loop

.Lbss_done:
    // restore DTB pointer
    mov     x0, x20
    bl      kmain

.Lhalt:
    wfe
    b       .Lhalt

unsupported_el:
.Lunsupported:
    wfe
    b       .Lunsupported

bad_boot_mmu:
.Lmmu:
    wfe
    b       .Lmmu
