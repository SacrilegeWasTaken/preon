/*
 *
 *  EL1 exception vector table (.text.vectors) and save/restore trampolines.
 *
 *  VBAR_EL1 points at `vector_table`. On any exception the CPU jumps to the
 *  matching 0x80-byte slot, which reserves a 272-byte trap frame on the kernel
 *  stack, spills all state into it, and calls the Rust handler with a
 *  `&mut TrapFrame` (x0 = frame pointer). On return the frame is reloaded and
 *  `eret` resumes the interrupted code.
 *
 *  The frame layout below mirrors kernel_exceptions::types::TrapFrame exactly;
 *  the TrapFrame `*_OFFSET` / `SIZE` constants name these same byte offsets:
 *    [0 .. 240)  x0..x29 (15 register pairs)      [240] x30
 *    [248] SP_EL0    [256] ELR_EL1    [264] SPSR_EL1    frame size = 272
 *
 */

.section .text.vectors

/*
 *
 *  Context save / restore
 *
 */

// Spill the interrupted context into the 272-byte frame at [sp], which the
// `vector_entry` slot already reserved (sub sp, #272).
.macro save_context
    stp x0, x1,   [sp, #0]
    stp x2, x3,   [sp, #16]
    stp x4, x5,   [sp, #32]
    stp x6, x7,   [sp, #48]
    stp x8, x9,   [sp, #64]
    stp x10, x11, [sp, #80]
    stp x12, x13, [sp, #96]
    stp x14, x15, [sp, #112]
    stp x16, x17, [sp, #128]
    stp x18, x19, [sp, #144]
    stp x20, x21, [sp, #160]
    stp x22, x23, [sp, #176]
    stp x24, x25, [sp, #192]
    stp x26, x27, [sp, #208]
    stp x28, x29, [sp, #224]
    str x30,      [sp, #240]
    // Exception-return state lives in system registers; copy it into the frame
    // so a handler can inspect or edit it (e.g. advance ELR, adjust SPSR).
    mrs x0, sp_el0
    mrs x1, elr_el1
    mrs x2, spsr_el1
    stp x0, x1,   [sp, #248]    // SP_EL0, ELR_EL1
    str x2,       [sp, #264]    // SPSR_EL1
.endm

// Inverse of save_context: restore the return state into the system registers
// first, then the general-purpose registers, leaving the frame ready to pop.
.macro restore_context
    ldr x2,       [sp, #264]
    ldp x0, x1,   [sp, #248]
    msr spsr_el1, x2
    msr elr_el1,  x1
    msr sp_el0,   x0
    ldr x30,      [sp, #240]
    ldp x28, x29, [sp, #224]
    ldp x26, x27, [sp, #208]
    ldp x24, x25, [sp, #192]
    ldp x22, x23, [sp, #176]
    ldp x20, x21, [sp, #160]
    ldp x18, x19, [sp, #144]
    ldp x16, x17, [sp, #128]
    ldp x14, x15, [sp, #112]
    ldp x12, x13, [sp, #96]
    ldp x10, x11, [sp, #80]
    ldp x8, x9,   [sp, #64]
    ldp x6, x7,   [sp, #48]
    ldp x4, x5,   [sp, #32]
    ldp x2, x3,   [sp, #16]
    ldp x0, x1,   [sp, #0]
.endm

/*
 *
 *  Vector table
 *
 */

// One vector slot. The architecture allots exactly 0x80 bytes per vector, so
// the slot just reserves the trap frame and branches to the shared stub.
.macro vector_entry handler
    .balign 0x80
    sub sp, sp, #272           // reserve the trap frame
    b \handler
.endm

// VBAR_EL1 requires 2 KiB alignment. The 16 entries are four groups of four,
// in the architectural order. We service EL1 (SP_ELx) and EL0-AArch64 traps;
// the SP_EL0 and AArch32 groups are contract violations routed to `bad_mode`.
.balign 2048
.global vector_table
vector_table:
    // Current EL with SP_EL0 — unused (the kernel runs on SP_ELx).
    vector_entry bad_mode
    vector_entry bad_mode
    vector_entry bad_mode
    vector_entry bad_mode

    // Current EL with SP_ELx — kernel-mode sync / IRQ / FIQ / SError.
    vector_entry el1_sync
    vector_entry el1_irq
    vector_entry el1_fiq
    vector_entry el1_serror

    // Lower EL, AArch64 — traps from EL0 (userspace, once it exists).
    vector_entry el0_sync
    vector_entry el0_irq
    vector_entry el0_fiq
    vector_entry el0_serror

    // Lower EL, AArch32 — unsupported execution state.
    vector_entry bad_mode
    vector_entry bad_mode
    vector_entry bad_mode
    vector_entry bad_mode

/*
 *
 *  Handler stubs and shared exit
 *
 */

// Each stub finishes the save, hands the frame pointer to its Rust handler
// (x0 = sp = &mut TrapFrame), then joins the common return path.
.macro impl_handler asm_handler_name rust_handler_name
\asm_handler_name:
    save_context
    mov x0, sp                 // x0 = &mut TrapFrame
    bl \rust_handler_name
    b common_exit
.endm

.section .text
impl_handler bad_mode bad_mode_handler
impl_handler el1_sync el1_sync_handler
impl_handler el1_irq el1_irq_handler
impl_handler el1_fiq el1_fiq_handler
impl_handler el1_serror el1_serror_handler
impl_handler el0_sync el0_sync_handler
impl_handler el0_irq el0_irq_handler
impl_handler el0_fiq el0_fiq_handler
impl_handler el0_serror el0_serror_handler

// Reload the (possibly handler-modified) frame, pop it, and return.
common_exit:
    restore_context
    add sp, sp, #272
    eret
