use libpthread::state::{current, PthreadState};

pub const THREAD_DB_HEADER_PATH: &str = "/usr/include/thread_db.h";
pub const CHECK_ABI_TEST_PATH: &str = "safe/tests/nptl_db/check-abi-libthread_db";
pub const DB_SYMBOLS_TEST_PATH: &str = "safe/tests/nptl_db/db-symbols";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThreadAgent {
    pub attached_pid: i32,
    pub main_thread: PthreadState,
}

pub fn attach_current_process() -> ThreadAgent {
    ThreadAgent {
        attached_pid: std::process::id() as i32,
        main_thread: current(),
    }
}
