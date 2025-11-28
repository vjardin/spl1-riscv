ENTRY(_start);

/* QEMU virt: place firmware at 0x80000000 */
MEMORY {
    ROM (rx)  : ORIGIN = 0x80000000, LENGTH = 256K
    RAM (rwx) : ORIGIN = 0x80040000, LENGTH = 256K
}

SECTIONS {
    .text : {
        KEEP(*(.text.init))     /* <- ensure _start is first */
        *(.text*)
        *(.rodata*)
    } > ROM

    .bss (NOLOAD) : {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        __bss_end = .;
    } > RAM

    .stack (NOLOAD) : {
        . = ALIGN(16);
        _stack_top = . + 0x4000; /* 16 KiB stack */
    } > RAM
}
