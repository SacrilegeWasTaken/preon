.section .text.vectors

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
    mrs x0, sp_el0
    mrs x1, elr_el1
    mrs x2, spsr_el1
    stp x0, x1,   [sp, #248]
    str x2,       [sp, #264]
.endm

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

.macro vector_entry handler
    .balign 0x80
    sub sp, sp, #272
    b \handler
.endm

.balign 2048
.global vector_table
vector_table:
    vector_entry bad_mode
    vector_entry bad_mode
    vector_entry bad_mode
    vector_entry bad_mode

    vector_entry el1_sync
    vector_entry el1_irq
    vector_entry el1_fiq
    vector_entry el1_serror

    vector_entry el0_sync
    vector_entry el0_irq
    vector_entry el0_fiq
    vector_entry el0_serror

    vector_entry bad_mode
    vector_entry bad_mode
    vector_entry bad_mode
    vector_entry bad_mode

.macro impl_handler asm_handler_name rust_handler_name
\asm_handler_name:
    save_context
    mov x0, sp
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

common_exit:
    restore_context
    add sp, sp, #272
    eret
