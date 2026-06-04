.section .text

.global secondary_entry
.type secondary_entry, %function

.extern secondary_cpu_main
.extern install_current_cpu_local

secondary_entry:
    mov x19, x0

    // SecondaryBootData.stack_top

    ldr x1, [x19, #8]
    mov sp, x1

    // SecondaryBootData.cpu_local

    ldr x0, [x19]

    bl install_current_cpu_local

    mov x0, x19

    bl secondary_cpu_main

1:
    wfe
    b 1b
