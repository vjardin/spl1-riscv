use core::arch::global_asm;

// Put _start in a dedicated .text.init section, which we KEEP first
// in linker.ld
global_asm!(
    r#"
    .section .text.init
    .globl _start
_start:
    // Set up stack pointer (symbol provided by linker.ld)
    la sp, _stack_top

    // For now we ignore a0/a1 contents and just jump to spl_main.
    j spl_main
"#
);
