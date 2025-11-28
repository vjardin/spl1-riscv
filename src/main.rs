#![no_std]
#![no_main]

mod arch;         // _start entry in global_asm!
mod logger;       // UART + slog!
mod flash_intel;  // NOR driver
mod bootmeta;     // A/B metadata

use core::panic::PanicInfo;

use crate::bootmeta::{BootBank, BootMeta};
use crate::flash_intel::IntelFlash;
use crate::logger::uart_puts;

// Flash layout constants (must match prepare_flash.sh)
const FLASH_BASE: usize       = 0x2000_0000;            // QEMU pflash0 base
const FLASH_BLOCK_SIZE: usize = 128 * 1024;             // 128 KiB
const META_OFFSET: usize      = FLASH_BLOCK_SIZE * 255; // last block of 32 MiB
const META_SIZE: usize        = FLASH_BLOCK_SIZE;

const MAX_TRIALS: u32 = 4;

// Where QEMU would load OpenSBI fw_jump.bin (TODO)
const OPENSBI_BASE: usize = 0x8020_0000;

// QEMU virt (current) DTB address we see in our logs.
const QEMU_VIRT_DTB_ADDR: usize = 0x0000_0000_8fe0_0000;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    uart_puts("PANIC in SPL1\r\n");
    loop {}
}

// Decide at runtime if we should touch NOR flash (record_boot).
fn should_record_boot(dtb_pa: usize) -> bool {
    // For now: very simple heuristic.
    //  - On QEMU virt: dtb_pa == 0x8fe00000  -> skip writes
    //  - On real HW : dtb_pa likely different -> enable writes
    if dtb_pa == QEMU_VIRT_DTB_ADDR {
        slog!(
            "Likely detected QEMU virt DTB at 0x{:016x}, will NOT write to NOR",
            dtb_pa
        );
        false
    } else {
        slog!(
            "DTB at 0x{:016x} != QEMU_VIRT_DTB_ADDR, enabling NOR writes",
            dtb_pa
        );
        true
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn spl_main(hartid: usize, dtb_pa: usize) -> ! {
    slog!("spl1 starting (hartid={}, dtb=0x{:016x})", hartid, dtb_pa);

    let flash = IntelFlash {
        base: FLASH_BASE,
        block_size: FLASH_BLOCK_SIZE,
    };
    let meta = BootMeta::new(&flash, META_OFFSET, META_SIZE);

    let (a_count, b_count, next_idx) = meta.scan();
    slog!(
        "boot trials: bank A = {}, bank B = {}, next_idx = {}",
        a_count,
        b_count,
        next_idx
    );

    let bank = meta.choose_bank(MAX_TRIALS);
    slog!("chosen bank (for info): {:?}", bank);

    if should_record_boot(dtb_pa) {
        match meta.record_boot(bank) {
            Ok(()) => {
                slog!("recorded new boot trial for {:?}", bank);
            }
            Err(e) => {
                slog!("WARNING: failed to record boot trial: {:?}", e);
            }
        }
    } else {
        slog!("(QEMU) skipping record_boot(): no NOR writes from SPL1");
    }

    slog!("spl1 ok, next load opensbi, bye");

    // For now, just loop so we keep QEMU alive and see the messages.
    loop {
        unsafe { core::arch::asm!("wfi") }
    }

    // TODO:
    // jump_to_opensbi(hartid, dtb_pa);
}

#[allow(dead_code)]
fn jump_to_opensbi(hartid: usize, dtb_pa: usize) -> ! {
    let entry_ptr = OPENSBI_BASE as *const ();
    let entry: extern "C" fn(usize, usize) -> ! =
        unsafe { core::mem::transmute(entry_ptr) };
    entry(hartid, dtb_pa)
}
