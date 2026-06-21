ENTRY(_start)

KERNEL_VA_BASE = 0xFFFF800000000000;
PHYS_LOAD_BASE = 0x40080000;

SECTIONS {
    . = PHYS_LOAD_BASE;

    __boot_start = .;
    .boot : {
        KEEP(*(.text.boot))
        *(.boot .boot.*)
    }
    . = ALIGN(4096);
    __boot_end = .;
    __boot_bss_start = .;
   .boot.bss (NOLOAD) : {
        *(.boot.bss .boot.bss.*)
    }
    . = ALIGN(4096);
    __boot_bss_end = .;


    . += KERNEL_VA_BASE;


    .text : AT(ADDR(.text) - KERNEL_VA_BASE) {
        KEEP(*(.text.vectors))
        *(.text*)
    }

    .rodata : AT(ADDR(.rodata) - KERNEL_VA_BASE) ALIGN(8) { *(.rodata*) }
    .data : AT(ADDR(.data) - KERNEL_VA_BASE) ALIGN(8) { *(.data*) }

    .bss (NOLOAD) : AT(ADDR(.bss) - KERNEL_VA_BASE) ALIGN(16) {
        __bss_start = .;
        *(.bss*) *(COMMON);
        . = ALIGN(16);
        __bss_end = .;
    }

    . = ALIGN(16);
    . += 0x40000;
    __stack_top = .;

    __bss_start_pa = __bss_start - KERNEL_VA_BASE;
    __bss_end_pa = __bss_end - KERNEL_VA_BASE;
    __stack_top_pa = __stack_top - KERNEL_VA_BASE;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
