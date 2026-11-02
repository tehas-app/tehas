//! Printing to stdout that ends quietly when the reader has gone.
//!
//! Rust ignores SIGPIPE, so a write to a closed pipe fails with EPIPE, and
//! `println!` panics on it: `tehas status | head -1` printed a panic and ended
//! 101. `outln!` and `out!` write the same text and exit 141 on EPIPE, the
//! status a shell gives a command SIGPIPE killed, with nothing on stderr. Any
//! other failed write still panics, as `println!` does.

use std::fmt;
use std::io::{self, ErrorKind, Write};

/// The exit status when stdout's reader has gone: 128 + SIGPIPE.
pub const CLOSED: i32 = 141;

#[doc(hidden)]
pub fn write(args: fmt::Arguments) {
    if let Err(e) = io::stdout().lock().write_fmt(args) {
        if e.kind() == ErrorKind::BrokenPipe {
            std::process::exit(CLOSED);
        }
        panic!("failed printing to stdout: {e}");
    }
}

/// `println!` that exits 141 when stdout's reader has gone.
#[macro_export]
macro_rules! outln {
    () => {
        $crate::out::write(format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::out::write(format_args!("{}\n", format_args!($($arg)*)))
    };
}

/// `print!` that exits 141 when stdout's reader has gone.
#[macro_export]
macro_rules! out {
    ($($arg:tt)*) => {
        $crate::out::write(format_args!($($arg)*))
    };
}
