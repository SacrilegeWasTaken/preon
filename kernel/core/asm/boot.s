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
    ldr     x0, =__idmap_l0     // start addrress of first of 8 buffers
    ldr     x1, =__kernel_l2    // address of the last buffer
    add     x1, x1, #0x1000     // end address of the last page 

.Lbootbss_loop:
    cmp     x0, x1
    b.hs    .Lbootbss_done
    str     xzr, [x0], #8
    b       .Lbootbss_loop

.Lbootbss_done:

    // MAIR 0x0000_0000_0400_44FF
    movz    x0, #0x44FF
    movk    x0, #0x0400, lsl #16
    msr     mair_el1, x0
    
    // TCR 0x0000_0025_B550_3510
    movz    x0, #0x3510
    movk    x0, #0xB550, lsl #16
    movk    x0, #0x0025, lsl #32
    msr     tcr_el1, x0

    isb

    // Build TTBR0 trampoline tables

    // __idmap_l0[0] = __idmap_l1 | TABLE
    ldr     x0, =__idmap_l0
    ldr     x1, =__idmap_l1
    orr     x1, x1, #3
    str     x1, [x0]

    // __idmap_l1[0] = __idmap_l2_uart | TABLE  (0-1 GiB range)
    ldr     x0, =__idmap_l1
    ldr     x1, =__idmap_l2_uart
    orr     x1, x1, #3
    str     x1, [x0]

    // __idmap_l1[1] = __idmap_l2_ram | TABLE  (1-2 GiB range)
    ldr     x1, =__idmap_l2_ram
    orr     x1, x1, #3
    str     x1, [x0, #8]

    // __idmap_l2_uart[72] = __idmap_l3_uart | TABLE
    ldr     x0, =__idmap_l2_uart
    ldr     x1, =__idmap_l3_uart
    orr     x1, x1, #3
    str     x1, [x0, #(72*8)]

    // __idmap_l3_uart[0] = UART page (Device-nGnRE)
    ldr     x0, =__idmap_l3_uart
    ldr     x1, =0x006000000900040F
    str     x1, [x0]

    // __idmap_l2_ram[0] = 2 MiB block (kernel RAM, Normal, RW, exec)
    ldr     x0, =__idmap_l2_ram
    ldr     x1, =0x0040000040000701
    str     x1, [x0]

    // Fill kernel_map (TTBR1)

    // __kernel_l0[256] = __kernel_l1 | TABLE
    ldr     x0, =__kernel_l0
    ldr     x1, =__kernel_l1
    orr     x1, x1, #3
    str     x1, [x0, #(256*8)]

    // __kernel_l1[1] = __kernel_l2 | TABLE
    ldr     x0, =__kernel_l1
    ldr     x1, =__kernel_l2
    orr     x1, x1, #3
    str     x1, [x0, #8]

    // __kernel_l2[0] = 2 MiB block at PA 0x4000_0000
    ldr     x0, =__kernel_l2
    ldr     x1, =0x0040000040000701
    str     x1, [x0]

    // __kernel_l2[1] = 2 MiB block at PA 0x4020_0000
    ldr     x1, =0x0040000040200701
    str     x1, [x0, #8]

    // Write TTBRs 

    // TTBR0 = trampoline root (TCR_EL1.TG0 = 4 KiB, ASID = 0)
    ldr     x0, =__idmap_l0
    msr     ttbr0_el1, x0

    // TTBR1 = kernel_map root (TCR_EL1.TG1 = 4 KiB)
    ldr     x0, =__kernel_l0
    msr     ttbr1_el1, x0

    // Barrier sequence + SCTLR.M flip 

    // Make page-table stores globally observable before TLB invalidate
    dsb     ish

    // Invalidate all TLB entries (Inner Shareable)
    tlbi    vmalle1is

    // Wait for TLB invalidate to complete
    dsb     ish

    // Synchronize pipeline before reading SCTLR_EL1
    isb

    // Flip SCTLR_EL1.M (bit 0)
    mrs     x0, sctlr_el1
    orr     x0, x0, #1
    msr     sctlr_el1, x0

    // First fetch after the flip MUST go through MMU; isb forces re-fetch
    isb

    // Trampoline jump to upper-half kmain 

    mov     x0, x20              // restore DTB pointer in x0
    ldr     x16, =kmain          // x16 = upper-half VMA of kmain
    br      x16                  // absolute branch — through TTBR1

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
