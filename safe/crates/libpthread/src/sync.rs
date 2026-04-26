use core_runtime::futex::{self, FutexWakeTarget};
use std::sync::atomic::{AtomicI32, Ordering};

pub struct FutexMutex {
    state: AtomicI32,
}

impl FutexMutex {
    pub const fn new() -> Self {
        Self {
            state: AtomicI32::new(0),
        }
    }

    pub fn lock(&self) {
        loop {
            if self
                .state
                .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
                .is_ok()
            {
                return;
            }
            let _ = futex::wait(&self.state, 1, None);
        }
    }

    pub fn unlock(&self) {
        self.state.store(0, Ordering::Release);
        let _ = futex::wake(&self.state, FutexWakeTarget::One);
    }

    pub fn with_lock<R>(&self, f: impl FnOnce() -> R) -> R {
        self.lock();
        let result = f();
        self.unlock();
        result
    }
}

impl Default for FutexMutex {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FutexCondvar {
    generation: AtomicI32,
}

impl FutexCondvar {
    pub const fn new() -> Self {
        Self {
            generation: AtomicI32::new(0),
        }
    }

    pub fn wait(&self, mutex: &FutexMutex) {
        let current = self.generation.load(Ordering::Acquire);
        mutex.unlock();
        let _ = futex::wait(&self.generation, current, None);
        mutex.lock();
    }

    pub fn notify_one(&self) {
        self.generation.fetch_add(1, Ordering::Release);
        let _ = futex::wake(&self.generation, FutexWakeTarget::One);
    }

    pub fn notify_all(&self) {
        self.generation.fetch_add(1, Ordering::Release);
        let _ = futex::wake(&self.generation, FutexWakeTarget::All);
    }
}

impl Default for FutexCondvar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::FutexMutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn mutex_serializes_access() {
        let mutex = FutexMutex::new();
        let counter = AtomicUsize::new(0);
        mutex.with_lock(|| {
            counter.fetch_add(1, Ordering::Relaxed);
        });
        assert_eq!(counter.load(Ordering::Relaxed), 1);
    }
}
