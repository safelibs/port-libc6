use std::cell::Cell;

thread_local! {
    static ERRNO: Cell<i32> = const { Cell::new(0) };
}

pub fn get() -> i32 {
    ERRNO.with(Cell::get)
}

pub fn set(value: i32) {
    ERRNO.with(|slot| slot.set(value));
}

pub fn clear() {
    set(0);
}

pub fn last_os_error_code() -> i32 {
    std::io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

pub fn capture_last_os_error() -> i32 {
    let code = last_os_error_code();
    set(code);
    code
}

pub fn with_location<R>(f: impl FnOnce(*mut i32) -> R) -> R {
    ERRNO.with(|slot| f(slot.as_ptr()))
}

#[no_mangle]
pub extern "C" fn __errno_location() -> *mut i32 {
    with_location(|ptr| ptr)
}

#[derive(Debug)]
pub struct ErrnoGuard {
    saved: i32,
}

impl ErrnoGuard {
    pub fn new() -> Self {
        Self { saved: get() }
    }
}

impl Default for ErrnoGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ErrnoGuard {
    fn drop(&mut self) {
        set(self.saved);
    }
}
