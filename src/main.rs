#![no_std]
#![no_main]

use core::arch::global_asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

// Put _start in a dedicated .text.init section, which we KEEP first
// in linker.ld so the entry point is exactly where we expect.

global_asm!(
    r#"
    .section .text.init
    .globl _start
_start:
    // Set up stack pointer
    la sp, _stack_top

    // For now we ignore a0/a1 contents and just jump to spl_main.
    j spl_main
"#
);

// UART logging (NS16550)
const UART0_BASE: *mut u8 = 0x1000_0000 as *mut u8; // QEMU virt UART

#[inline(always)]
fn uart_putc(b: u8) {
    unsafe {
        core::ptr::write_volatile(UART0_BASE, b);
    }
}

fn uart_puts(s: &str) {
    for &b in s.as_bytes() {
        uart_putc(b);
    }
}

struct UartWriter;

impl Write for UartWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        uart_puts(s);
        Ok(())
    }
}

macro_rules! slog {
    ($($arg:tt)*) => {{
        let mut w = UartWriter;
        let _ = write!(w, "[{}:{}] ", file!(), line!());
        let _ = writeln!(w, $($arg)*);
    }};
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    uart_puts("PANIC in SPL1\r\n");
    loop {}
}

// Intel-style CFI NOR
mod flash_intel {
    use core::result::Result;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FlashError {
        Timeout,
        ProgramError,
        EraseError,
        VppError,
        Protected,
    }

    /// Very small Intel CFI NOR (pflash_cfi01) driver, 8-bit commands.
    pub struct IntelFlash {
        pub base: usize,
        pub block_size: usize,
    }

    impl IntelFlash {
        const CMD_READ_ARRAY: u8 = 0xFF;
        const CMD_READ_STATUS: u8 = 0x70;
        const CMD_CLEAR_STATUS: u8 = 0x50;
        const CMD_BLOCK_ERASE_SETUP: u8 = 0x20;
        const CMD_BLOCK_ERASE_CONFIRM: u8 = 0xD0;
        const CMD_PROGRAM: u8 = 0x40;

        const SR_WSM_READY: u8 = 1 << 7;
        const SR_ERASE_ERROR: u8 = 1 << 5;
        const SR_PROGRAM_ERROR: u8 = 1 << 4;
        const SR_VPP_ERROR: u8 = 1 << 3;
        const SR_PROTECT_ERROR: u8 = 1 << 1;

        #[inline(always)]
        fn write_cmd8(&self, offset: usize, cmd: u8) {
            unsafe {
                let ptr = (self.base + offset) as *mut u8;
                core::ptr::write_volatile(ptr, cmd);
            }
        }

        #[inline(always)]
        fn write_data8(&self, offset: usize, data: u8) {
            unsafe {
                let ptr = (self.base + offset) as *mut u8;
                core::ptr::write_volatile(ptr, data);
            }
        }

        #[inline(always)]
        fn read_u8(&self, offset: usize) -> u8 {
            unsafe {
                let ptr = (self.base + offset) as *const u8;
                core::ptr::read_volatile(ptr)
            }
        }

        #[inline(always)]
        fn enter_read_status(&self) {
            self.write_cmd8(0, Self::CMD_READ_STATUS);
        }

        #[inline(always)]
        fn clear_status(&self) {
            self.write_cmd8(0, Self::CMD_CLEAR_STATUS);
        }

        #[inline(always)]
        fn read_status(&self) -> u8 {
            self.read_u8(0)
        }

        #[inline(always)]
        fn return_to_read_array(&self) {
            self.write_cmd8(0, Self::CMD_READ_ARRAY);
        }

        fn wait_ready(&self, max_polls: usize) -> Result<u8, FlashError> {
            self.enter_read_status();
            let mut i = 0;
            loop {
                let sr = self.read_status();
                if sr & Self::SR_WSM_READY != 0 {
                    return Ok(sr);
                }
                i += 1;
                if i >= max_polls {
                    return Err(FlashError::Timeout);
                }
            }
        }

        /// Read `buf.len()` bytes starting from `flash_offset`.
        pub fn read_slice(&self, flash_offset: usize, buf: &mut [u8]) {
            for (i, b) in buf.iter_mut().enumerate() {
                *b = self.read_u8(flash_offset + i);
            }
        }

        /// Read a little-endian u32 from flash.
        pub fn read_u32_le(&self, flash_offset: usize) -> u32 {
            let mut tmp = [0u8; 4];
            self.read_slice(flash_offset, &mut tmp);
            u32::from_le_bytes(tmp)
        }

        /// Erase a single block by index (0-based).
        pub fn block_erase(&self, block_index: usize) -> Result<(), FlashError> {
            let block_base = block_index * self.block_size;

            self.clear_status();
            self.write_cmd8(block_base, Self::CMD_BLOCK_ERASE_SETUP);
            self.write_cmd8(block_base, Self::CMD_BLOCK_ERASE_CONFIRM);

            let sr = self.wait_ready(1_000_000)?;

            if sr & Self::SR_ERASE_ERROR != 0 {
                self.return_to_read_array();
                return Err(FlashError::EraseError);
            }
            if sr & Self::SR_VPP_ERROR != 0 {
                self.return_to_read_array();
                return Err(FlashError::VppError);
            }
            if sr & Self::SR_PROTECT_ERROR != 0 {
                self.return_to_read_array();
                return Err(FlashError::Protected);
            }

            self.return_to_read_array();
            Ok(())
        }

        /// Program a single byte at `offset`.
        pub fn program_byte(&self, offset: usize, value: u8) -> Result<(), FlashError> {
            self.clear_status();
            self.write_cmd8(offset, Self::CMD_PROGRAM);
            self.write_data8(offset, value);

            let sr = self.wait_ready(1_000_000)?;

            if sr & Self::SR_PROGRAM_ERROR != 0 {
                self.return_to_read_array();
                return Err(FlashError::ProgramError);
            }
            if sr & Self::SR_VPP_ERROR != 0 {
                self.return_to_read_array();
                return Err(FlashError::VppError);
            }
            if sr & Self::SR_PROTECT_ERROR != 0 {
                self.return_to_read_array();
                return Err(FlashError::Protected);
            }

            self.return_to_read_array();
            Ok(())
        }

        /// Program arbitrary data at `flash_offset`.
        /// Caller must honor NOR semantics (only 1->0 bit transitions).
        pub fn program(
            &self,
            flash_offset: usize,
            data: &[u8],
        ) -> Result<(), FlashError> {
            for (i, b) in data.iter().enumerate() {
                let dst_off = flash_offset + i;
                let current = self.read_u8(dst_off);

                // Only allow 1→0 transitions
                if (*b | current) != current {
                    return Err(FlashError::ProgramError);
                }

                self.program_byte(dst_off, *b)?;
            }
            Ok(())
        }
    }
}

// Boot metadata (A/B)
mod bootmeta {
    use core::result::Result;
    use crate::flash_intel::{FlashError, IntelFlash};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BootBank {
        A,
        B,
    }

    pub struct BootMeta<'a> {
        flash: &'a IntelFlash,
        meta_offset: usize,
        meta_size: usize,
    }

    impl<'a> BootMeta<'a> {
        const ERASED_WORD: u32 = 0xFFFF_FFFF;
        const TOKEN_BANK_A: u32 = 0x1111_1111;
        const TOKEN_BANK_B: u32 = 0x0000_0000;

        pub const WORD_SIZE: usize = core::mem::size_of::<u32>();

        pub const fn new(
            flash: &'a IntelFlash,
            meta_offset: usize,
            meta_size: usize,
        ) -> Self {
            BootMeta {
                flash,
                meta_offset,
                meta_size,
            }
        }

        fn words_capacity(&self) -> usize {
            self.meta_size / Self::WORD_SIZE
        }

        fn word_offset(&self, idx: usize) -> usize {
            self.meta_offset + idx * Self::WORD_SIZE
        }

        fn read_word(&self, idx: usize) -> u32 {
            self.flash.read_u32_le(self.word_offset(idx))
        }

        fn write_word(&self, idx: usize, value: u32) -> Result<(), FlashError> {
            let bytes = value.to_le_bytes();
            self.flash.program(self.word_offset(idx), &bytes)
        }

        /// Scan log: returns (a_count, b_count, next_free_index).
        pub fn scan(&self) -> (u32, u32, usize) {
            let mut a_count = 0u32;
            let mut b_count = 0u32;
            let mut idx = 0usize;
            let cap = self.words_capacity();

            while idx < cap {
                let w = self.read_word(idx);
                if w == Self::ERASED_WORD {
                    break;
                } else if w == Self::TOKEN_BANK_A {
                    a_count += 1;
                } else if w == Self::TOKEN_BANK_B {
                    b_count += 1;
                } else {
                    break;
                }
                idx += 1;
            }

            (a_count, b_count, idx)
        }

        fn compact(
            &self,
            mut a_count: u32,
            mut b_count: u32,
        ) -> Result<(), FlashError> {
            let block_index = self.meta_offset / self.flash.block_size;

            self.flash.block_erase(block_index)?;

            let mut idx = 0usize;

            while a_count > 0 {
                self.write_word(idx, Self::TOKEN_BANK_A)?;
                idx += 1;
                a_count -= 1;
            }

            while b_count > 0 {
                self.write_word(idx, Self::TOKEN_BANK_B)?;
                idx += 1;
                b_count -= 1;
            }

            Ok(())
        }

        pub fn record_boot(&self, bank: BootBank) -> Result<(), FlashError> {
            let (a_count, b_count, mut next_idx) = self.scan();
            let cap = self.words_capacity();

            if next_idx >= cap {
                self.compact(a_count, b_count)?;
                let (_a2, _b2, idx2) = self.scan();
                next_idx = idx2;
            }

            let token = match bank {
                BootBank::A => Self::TOKEN_BANK_A,
                BootBank::B => Self::TOKEN_BANK_B,
            };

            self.write_word(next_idx, token)
        }

        pub fn choose_bank(&self, max_trials: u32) -> BootBank {
            let (a_count, b_count, _idx) = self.scan();

            if b_count < max_trials {
                BootBank::B
            } else if a_count < max_trials {
                BootBank::A
            } else {
                BootBank::B
            }
        }
    }
}

// SPL1 entry + logic
use flash_intel::IntelFlash;
use bootmeta::BootMeta;

const FLASH_BASE: usize = 0x2000_0000; // TODO virt pflash base (later)
const FLASH_BLOCK_SIZE: usize = 128 * 1024; // TODO adjust if needed
const META_OFFSET: usize = FLASH_BLOCK_SIZE * 255; // last block of 32 MiB
const META_SIZE: usize = FLASH_BLOCK_SIZE;

const MAX_TRIALS: u32 = 4;

// Where QEMU would load OpenSBI fw_jump.bin TODO
const OPENSBI_BASE: usize = 0x8020_0000;

/// spl_main is entered from assembly stub `_start`.
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

    match meta.record_boot(bank) {
        Ok(()) => {
            slog!("recorded new boot trial for {:?}", bank);
        }
        Err(e) => {
            slog!("WARNING: failed to record boot trial: {:?}", e);
        }
    }

    slog!("spl1 ok, next load opensbi, bye");

    // For now, just loop so we keep QEMU alive and see the messages.
    loop {
        unsafe { core::arch::asm!("wfi") }
    }

    // TODO jump_to_opensbi(hartid, dtb_pa);
}

#[allow(dead_code)]
fn jump_to_opensbi(hartid: usize, dtb_pa: usize) -> ! {
    let entry_ptr = OPENSBI_BASE as *const ();
    let entry: extern "C" fn(usize, usize) -> ! = unsafe {
        core::mem::transmute(entry_ptr)
    };
    entry(hartid, dtb_pa)
}
