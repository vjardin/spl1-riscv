use core::result::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashError {
    ProgramError,
    EraseError,
}

/// Very small Intel CFI NOR (pflash_cfi01) driver, 8-bit commands,
/// simplified for QEMU:
///   - No status polling
///   - No real erase (we rely on pre-erased image for the meta block)
pub struct IntelFlash {
    pub base: usize,
    pub block_size: usize,
}

impl IntelFlash {
    const CMD_PROGRAM: u8 = 0x40;

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

    /// Program a single byte at `offset`.
    /// Enforces NOR semantics: only 1→0 transitions allowed.
    pub fn program_byte(&self, offset: usize, value: u8) -> Result<(), FlashError> {
        let current = self.read_u8(offset);

        // Only allow 1→0 transitions; cannot set bits back to 1.
        if (value | current) != current {
            return Err(FlashError::ProgramError);
        }

        // Intel "program" sequence: cmd at address, then data.
        self.write_cmd8(offset, Self::CMD_PROGRAM);
        self.write_data8(offset, value);

        // In real hardware we would poll SR here; for QEMU's pflash we
        // assume the write completes "instantly".
        Ok(())
    }

    /// Program arbitrary data at `flash_offset`.
    pub fn program(&self, flash_offset: usize, data: &[u8]) -> Result<(), FlashError> {
        for (i, b) in data.iter().enumerate() {
            let dst_off = flash_offset + i;
            self.program_byte(dst_off, *b)?;
        }
        Ok(())
    }

    /// Stubbed erase: not used in normal path (we pre-erase the meta block),
    /// but kept for API compatibility.
    pub fn block_erase(&self, _block_index: usize) -> Result<(), FlashError> {
        Err(FlashError::EraseError)
    }
}
