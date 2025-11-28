use core::result::Result;
use crate::flash_intel::{FlashError, IntelFlash};
use crate::slog; // slog! macro

/// Which bank we booted from / are about to try.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootBank {
    A,
    B,
}

/// Simple append-only log of boot attempts, stored in NOR flash.
///
/// Layout in the metadata region:
///   - each entry is a 32-bit word
///   - 0xFFFF_FFFF = erased/unused
///   - 0x1111_1111 = "booted bank A"
///   - 0x0000_0000 = "booted bank B"
///
/// The log grows by appending words; compaction is theoretically
/// supported but depends on block_erase() being implemented.
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

    /// Scan the metadata area and count how many times each bank appears,
    /// and where the next free entry is.
    ///
    /// Returns: (a_count, b_count, next_free_index)
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
                // Unknown value, stop scanning to be conservative.
                break;
            }
            idx += 1;
        }

        (a_count, b_count, idx)
    }

    /// Compact the log by erasing the whole block and rewriting only the
    /// effective counts.
    ///
    /// For real NOR, let's use working block_erase(); in QEMU the
    /// current flash_intel::block_erase() is a stub and this will error.
    fn compact(
        &self,
        mut a_count: u32,
        mut b_count: u32,
    ) -> Result<(), FlashError> {
        let block_index = self.meta_offset / self.flash.block_size;

        slog!("compact: erasing block index {}", block_index);
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

    /// Record a boot attempt for the given bank.
    ///
    /// The runtime decision to *call* this (or not) is made in spl_main
    /// via should_record_boot(), so this function always assumes "writes allowed".
    pub fn record_boot(&self, bank: BootBank) -> Result<(), FlashError> {
        let (a_count, b_count, mut next_idx) = self.scan();
        let cap = self.words_capacity();

        slog!(
            "record_boot: start (bank={:?}, a_count={}, b_count={}, next_idx={}, cap={})",
            bank,
            a_count,
            b_count,
            next_idx,
            cap
        );

        if next_idx >= cap {
            slog!("record_boot: log full, compacting");
            self.compact(a_count, b_count)?;
            let (_a2, _b2, idx2) = self.scan();
            next_idx = idx2;
            slog!("record_boot: after compact scan: next_idx={}", next_idx);
        }

        let token = match bank {
            BootBank::A => Self::TOKEN_BANK_A,
            BootBank::B => Self::TOKEN_BANK_B,
        };

        slog!(
            "record_boot: writing token 0x{:08x} at word index {} (offset=0x{:x})",
            token,
            next_idx,
            self.word_offset(next_idx),
        );

        self.write_word(next_idx, token)
    }

    /// Pick which bank to boot next (A/B) based on how many trials each
    /// already has.
    pub fn choose_bank(&self, max_trials: u32) -> BootBank {
        let (a_count, b_count, _idx) = self.scan();

        if b_count < max_trials {
            BootBank::B
        } else if a_count < max_trials {
            BootBank::A
        } else {
            // Both reached max_trials, fall back to B by convention.
            BootBank::B
        }
    }
}
