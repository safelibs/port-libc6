use crate::errno;
use std::ffi::c_void;
use std::num::NonZeroUsize;
use std::ptr::NonNull;

pub fn mmap_anonymous(length: NonZeroUsize, prot: i32, flags: i32) -> Result<NonNull<c_void>, i32> {
    // SAFETY: This is the narrow FFI boundary for Linux page mapping. The
    // wrapper fixes the fd/offset pair required for anonymous mappings.
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            length.get(),
            prot,
            flags | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        Err(errno::capture_last_os_error())
    } else {
        Ok(NonNull::new(ptr).expect("mmap succeeded with a null pointer"))
    }
}

pub fn mprotect(ptr: NonNull<c_void>, length: usize, prot: i32) -> Result<(), i32> {
    // SAFETY: The caller supplies a range previously obtained from a mapping
    // API. The wrapper forwards the request and captures errno on failure.
    let rc = unsafe { libc::mprotect(ptr.as_ptr(), length, prot) };
    if rc == -1 {
        Err(errno::capture_last_os_error())
    } else {
        Ok(())
    }
}

pub fn munmap(ptr: NonNull<c_void>, length: usize) -> Result<(), i32> {
    // SAFETY: The caller supplies the exact mapping base and length to tear
    // down. The wrapper only forwards to libc and records errno on failure.
    let rc = unsafe { libc::munmap(ptr.as_ptr(), length) };
    if rc == -1 {
        Err(errno::capture_last_os_error())
    } else {
        Ok(())
    }
}
