/*
 * AiOS ARM Linker Script (Raspberry Pi)
 * Kernel loaded at 0x8000 by the GPU bootloader
 */

ENTRY(_start)

SECTIONS {
    . = 0x8000;

    .text : {
        KEEP(*(.text.boot))
        *(.text*)
    }

    .rodata : {
        *(.rodata*)
    }

    .data : {
        *(.data*)
    }

    . = ALIGN(4);
    __bss_start = .;
    .bss : {
        *(.bss*)
        *(COMMON)
    }
    __bss_end = .;

    . = ALIGN(4096);
    __heap_start = .;

    /DISCARD/ : {
        *(.comment)
        *(.note*)
        *(.ARM.attributes)
    }
}
