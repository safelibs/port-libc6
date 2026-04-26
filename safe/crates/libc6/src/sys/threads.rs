pub use core_runtime::thread::{current_descriptor, current_thread_id, ThreadDescriptor};
pub use core_runtime::tls::{
    mutate_state, pointer_guard, reseed_pointer_guard, with_state, ThreadLocalState,
};
