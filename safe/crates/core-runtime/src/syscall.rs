use crate::errno;
use libc::{c_long, pid_t};

pub type SyscallResult<T> = Result<T, i32>;

pub fn syscall0(number: c_long) -> SyscallResult<c_long> {
    syscall6(number, [0, 0, 0, 0, 0, 0])
}

pub fn syscall3(number: c_long, args: [c_long; 3]) -> SyscallResult<c_long> {
    syscall6(number, [args[0], args[1], args[2], 0, 0, 0])
}

pub fn syscall6(number: c_long, args: [c_long; 6]) -> SyscallResult<c_long> {
    // SAFETY: `libc::syscall` is the narrow FFI boundary for low-level Linux
    // entrypoints. Callers provide already-integerized kernel arguments.
    let rc = unsafe { libc::syscall(number, args[0], args[1], args[2], args[3], args[4], args[5]) };
    if rc == -1 {
        Err(errno::capture_last_os_error())
    } else {
        Ok(rc)
    }
}

pub fn gettid() -> SyscallResult<pid_t> {
    syscall0(libc::SYS_gettid as c_long).map(|value| value as pid_t)
}

pub fn getpid() -> pid_t {
    std::process::id() as pid_t
}
