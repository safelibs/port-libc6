use crate::errno::ErrnoGuard;
use std::ffi::c_void;

pub fn malloc(size: usize) -> *mut c_void {
    // SAFETY: The process allocator ABI requires forwarding exact size values
    // to libc's allocator entrypoints.
    unsafe { libc::malloc(size) }
}

pub fn calloc(count: usize, size: usize) -> *mut c_void {
    // SAFETY: The wrapper forwards count/size directly to libc's allocator and
    // leaves overflow handling to the allocator implementation.
    unsafe { libc::calloc(count, size) }
}

pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
    // SAFETY: Reallocating a pointer returned by the process allocator is the
    // expected C ABI contract for this boundary.
    unsafe { libc::realloc(ptr, size) }
}

pub fn free(ptr: *mut c_void) {
    let _guard = ErrnoGuard::new();
    // SAFETY: `ptr` must come from the process allocator. `free` should not
    // clobber thread-local errno, so the caller restores it with ErrnoGuard.
    unsafe { libc::free(ptr) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errno;

    #[test]
    fn free_preserves_errno() {
        let ptr = malloc(32);
        assert!(!ptr.is_null());
        errno::set(libc::ERANGE);
        free(ptr);
        assert_eq!(errno::get(), libc::ERANGE);
    }
}
