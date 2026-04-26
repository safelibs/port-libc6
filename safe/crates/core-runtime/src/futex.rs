use crate::syscall;
use libc::{c_long, timespec};
use std::sync::atomic::AtomicI32;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FutexWakeTarget {
    One,
    All,
}

pub fn wait(word: &AtomicI32, expected: i32, timeout: Option<Duration>) -> Result<(), i32> {
    let timeout_storage = timeout.map(duration_to_timespec);
    let timeout_ptr = timeout_storage
        .as_ref()
        .map(|value| value as *const timespec as c_long)
        .unwrap_or(0);
    syscall::syscall6(
        libc::SYS_futex as c_long,
        [
            word.as_ptr() as c_long,
            (libc::FUTEX_WAIT | libc::FUTEX_PRIVATE_FLAG) as c_long,
            expected as c_long,
            timeout_ptr,
            0,
            0,
        ],
    )?;
    Ok(())
}

pub fn wake(word: &AtomicI32, target: FutexWakeTarget) -> Result<usize, i32> {
    let count = match target {
        FutexWakeTarget::One => 1,
        FutexWakeTarget::All => i32::MAX,
    };
    syscall::syscall3(
        libc::SYS_futex as c_long,
        [
            word.as_ptr() as c_long,
            (libc::FUTEX_WAKE | libc::FUTEX_PRIVATE_FLAG) as c_long,
            count as c_long,
        ],
    )
    .map(|value| value as usize)
}

fn duration_to_timespec(duration: Duration) -> timespec {
    timespec {
        tv_sec: duration.as_secs() as libc::time_t,
        tv_nsec: duration.subsec_nanos() as libc::c_long,
    }
}
