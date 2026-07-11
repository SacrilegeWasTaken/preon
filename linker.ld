ENTRY(_start)

KERNEL_IMAGE_BASE = 0xFFFFFFFF80000000;
PHYS_LOAD_BASE = 0x40080000;
IMAGE_PA_BASE = 0x40000000;
IMAGE_TEXT_PA = 0x40100000;

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
        . = ALIGN(4096);
        __idmap_l0      = .;    . += 0x1000;
        __idmap_l1      = .;    . += 0x1000;
        __idmap_l2_uart = .;    . += 0x1000;
        __idmap_l3_uart = .;    . += 0x1000;
        __idmap_l2_ram  = .;    . += 0x1000;
        __kernel_l0     = .;    . += 0x1000;
        __kernel_l1     = .;    . += 0x1000;
        __kernel_l2     = .;    . += 0x1000;
        __image_l1      = .;    . += 0x1000;
        __device_l1     = .;    . += 0x1000;
        __device_l2     = .;    . += 0x1000;
        __device_l3     = .;    . += 0x1000;
        *(.boot.bss .boot.bss.*)
    }
    . = ALIGN(4096);
    __boot_bss_end = .;


    .percpu (NOLOAD) : ALIGN(4096) {
        __percpu_start = .;
        *(.percpu .percpu.*)
        . = ALIGN(4096);
        __percpu_end = .;
    }


    . = KERNEL_IMAGE_BASE + (IMAGE_TEXT_PA - IMAGE_PA_BASE);


    .text : AT(IMAGE_TEXT_PA) {
        __text_start = .;
        KEEP(*(.text.vectors));
        *(.text*);
        . = ALIGN(4096);
        __text_end = .;
    }

    .rodata : ALIGN(4096) { 
        __rodata_start = .;
        *(.rodata*);
        . = ALIGN(4096);
        __rodata_end = .;
    }
    .data : ALIGN(4096) {
        __data_start = .;
        *(.data*);
        . = ALIGN(4096);
        __data_end = .;
    }

    .bss (NOLOAD) : ALIGN(4096) {
        __bss_start = .;
        *(.bss*) *(COMMON);
        . = ALIGN(4096);
        __bss_end = .;
    }

    .stack (NOLOAD) : ALIGN(16) {
        __stack_bottom = .;
        . += 0x40000;
        . = ALIGN(16);
        __stack_top = .;
    }

    __bss_start_pa = __bss_start - KERNEL_IMAGE_BASE + IMAGE_PA_BASE;
    __bss_end_pa = __bss_end - KERNEL_IMAGE_BASE + IMAGE_PA_BASE;
    __stack_top_pa = __stack_top - KERNEL_IMAGE_BASE + IMAGE_PA_BASE;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.eh_frame*)
    }
}
