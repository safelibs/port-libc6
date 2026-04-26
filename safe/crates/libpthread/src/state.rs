use core_runtime::{thread, tls};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PthreadState {
    pub thread_id: u64,
    pub kernel_tid: Option<libc::pid_t>,
    pub pointer_guard: u64,
    pub name: Option<String>,
}

pub fn current() -> PthreadState {
    let descriptor = thread::current_descriptor();
    PthreadState {
        thread_id: descriptor.thread_id,
        kernel_tid: descriptor.kernel_tid,
        pointer_guard: tls::pointer_guard(),
        name: descriptor.name,
    }
}
