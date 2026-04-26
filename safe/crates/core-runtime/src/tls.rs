use crate::entropy;
use crate::thread;
use std::cell::RefCell;
use std::time::{SystemTime, UNIX_EPOCH};

thread_local! {
    static TLS_STATE: RefCell<ThreadLocalState> = RefCell::new(ThreadLocalState::new());
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadLocalState {
    pub runtime_thread_id: u64,
    pub pointer_guard: u64,
}

impl ThreadLocalState {
    fn new() -> Self {
        Self {
            runtime_thread_id: thread::current_thread_id(),
            pointer_guard: fresh_pointer_guard(),
        }
    }
}

pub fn with_state<R>(f: impl FnOnce(&ThreadLocalState) -> R) -> R {
    TLS_STATE.with(|state| f(&state.borrow()))
}

pub fn mutate_state<R>(f: impl FnOnce(&mut ThreadLocalState) -> R) -> R {
    TLS_STATE.with(|state| f(&mut state.borrow_mut()))
}

pub fn pointer_guard() -> u64 {
    with_state(|state| state.pointer_guard)
}

pub fn reseed_pointer_guard() -> u64 {
    mutate_state(|state| {
        state.pointer_guard = fresh_pointer_guard();
        state.pointer_guard
    })
}

fn fresh_pointer_guard() -> u64 {
    entropy::random_u64().unwrap_or_else(|_| fallback_pointer_guard())
}

fn fallback_pointer_guard() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    nanos ^ (std::process::id() as u64) ^ 0x9e37_79b9_7f4a_7c15
}
