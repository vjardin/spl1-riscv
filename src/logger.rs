use core::fmt::{self, Write};

// UART logging (NS16550)
const UART0_BASE: *mut u8 = 0x1000_0000 as *mut u8; // QEMU virt UART

#[inline(always)]
pub fn uart_putc(b: u8) {
    unsafe {
        core::ptr::write_volatile(UART0_BASE, b);
    }
}

pub fn uart_puts(s: &str) {
    for &b in s.as_bytes() {
        uart_putc(b);
    }
}

pub struct UartWriter;

impl Write for UartWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        uart_puts(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! slog {
    ($($arg:tt)*) => {{
        let mut w = $crate::logger::UartWriter;
        let _ = core::fmt::write(&mut w, format_args!("[{}:{}] ", file!(), line!()));
        let _ = core::fmt::write(&mut w, format_args!($($arg)*));
        $crate::logger::uart_puts("\n");
    }};
}
