use core_runtime::thread;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetxidRequest {
    pub sequence: u64,
    pub thread_id: u64,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
}

pub fn begin(uid: Option<u32>, gid: Option<u32>) -> SetxidRequest {
    SetxidRequest {
        sequence: NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        thread_id: thread::current_thread_id(),
        uid,
        gid,
    }
}

pub fn is_owner_thread(request: &SetxidRequest) -> bool {
    request.thread_id == thread::current_thread_id()
}
