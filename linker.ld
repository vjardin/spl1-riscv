ENTRY(_start);

/* QEMU virt memory map for XIP from NOR flash
 *
 *  - FLASH: CFI pflash0 at 0x2000_0000, 32 MiB
 *  - RAM  : DRAM at 0x8000_0000 (we use 1 MiB for SPL stack/BSS)
 */

MEMORY
{
    FLASH (rx)  : ORIGIN = 0x20000000, LENGTH = 32M
    RAM   (rwx) : ORIGIN = 0x80000000, LENGTH = 1M
}

SECTIONS
{
    /* SPL code and rodata live in flash (XIP) */
    .text : ALIGN(4)
    {
        KEEP(*(.text.init))     /* our _start stub */
        *(.text*)
        *(.rodata*)
    } > FLASH

    /* BSS in RAM */
    .bss (NOLOAD) : ALIGN(4)
    {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        __bss_end = .;
    } > RAM

    /* Stack: just define a symbol at top of our 1 MiB RAM */
    .stack (NOLOAD) : ALIGN(16)
    {
        _stack_top = ORIGIN(RAM) + LENGTH(RAM);
    } > RAM
}
