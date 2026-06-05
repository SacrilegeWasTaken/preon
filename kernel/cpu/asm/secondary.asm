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
    mov     x19, x0                         // save boot_data pointer

    // Enable FP/SIMD before any Rust call.
    mrs     x1, cpacr_el1
    orr     x1, x1, #(3 << 20)
    msr     cpacr_el1, x1
    isb

    // Install the shared exception vector table.
    adrp    x1, vector_table
    add     x1, x1, :lo12:vector_table
    msr     vbar_el1, x1
    isb

    // Switch to the per-CPU stack (boot_data.stack_top).
    ldr     x1, [x19, #8]
    mov     sp, x1

    ldr     x0, [x19]
    bl      install_current_cpu_local

    mov     x0, x19
    bl      secondary_cpu_main


1:  wfe
    b       1b
