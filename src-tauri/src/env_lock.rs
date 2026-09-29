//! Shared process-global environment mutex for tests. All tests that mutate
//! environment variables must take this single lock, otherwise tests in
//! different modules race on the same process-wide state.

#[cfg(test)]
pub fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(())).lock().unwrap()
}

#[cfg(test)]
macro_rules! set_var {
    ($k:expr, $v:expr) => {
        unsafe { std::env::set_var($k, $v) }
    };
}

#[cfg(test)]
macro_rules! remove_var {
    ($k:expr) => {
        unsafe { std::env::remove_var($k) }
    };
}

#[cfg(test)]
pub(crate) use {remove_var, set_var};
