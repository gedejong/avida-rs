//! Process-wide stdout/stderr emulation of the C++ `std::cout` / `std::cerr`.
//!
//! The reference writes to a fully buffered `stdout` (when it is a pipe or a
//! file) and flushes at every `std::endl` and at `exit()`. A crash (the
//! C++ dereferences NULL in a few paths) loses whatever was written since the
//! last `endl`. [`write`], [`endl`], [`flush`] and [`crash`] reproduce that.

use std::cell::RefCell;
use std::io::Write;

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// `std::cout << bytes` (no flush).
pub fn write(b: &[u8]) {
    OUT.with(|o| o.borrow_mut().extend_from_slice(b));
}

/// `std::cout << s`.
pub fn write_str(s: &str) {
    write(s.as_bytes());
}

/// `std::endl`: newline plus flush.
pub fn endl() {
    write(b"\n");
    flush();
}

/// Write the buffered stdout bytes out.
pub fn flush() {
    OUT.with(|o| {
        let mut o = o.borrow_mut();
        let so = std::io::stdout();
        let mut l = so.lock();
        let _ = l.write_all(&o);
        let _ = l.flush();
        o.clear();
    });
}

/// `std::cerr << bytes` (unbuffered).
pub fn err_write(b: &[u8]) {
    let se = std::io::stderr();
    let mut l = se.lock();
    let _ = l.write_all(b);
    let _ = l.flush();
}

/// `exit(code)`: flushes stdout like the C library does.
pub fn exit(code: i32) -> ! {
    flush();
    std::process::exit(code)
}

/// QUIRK: emulates a SIGSEGV of the C++ (NULL dereference). Unflushed stdout
/// is lost; the exit status is the shell view of a SIGSEGV death (128 + 11).
pub fn crash() -> ! {
    OUT.with(|o| o.borrow_mut().clear());
    std::process::exit(139)
}
