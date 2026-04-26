use crate::syscall;
use std::cell::Cell;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_THREAD_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static THREAD_ID: Cell<u64> = const { Cell::new(0) };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadDescriptor {
    pub thread_id: u64,
    pub kernel_tid: Option<libc::pid_t>,
    pub name: Option<String>,
}

pub fn current_thread_id() -> u64 {
    THREAD_ID.with(|slot| match slot.get() {
        0 => {
            let next = NEXT_THREAD_ID.fetch_add(1, Ordering::Relaxed);
            slot.set(next);
            next
        }
        existing => existing,
    })
}

pub fn current_descriptor() -> ThreadDescriptor {
    ThreadDescriptor {
        thread_id: current_thread_id(),
        kernel_tid: syscall::gettid().ok(),
        name: std::thread::current().name().map(str::to_string),
    }
}
