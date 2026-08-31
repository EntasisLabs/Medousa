//! Process-wide environment lock for tests (TEST-001).
//!
//! Unit tests must not mutate `std::env` in parallel without this guard.
//! Prefer [`crate::paths::scoped_test_data_dir`] when only the data root changes.
//!
//! `MEDOUSA_TEST_HERMETIC=1` (set by `scripts/ci/test-hermetic.sh`) refuses the
//! host keyring and panics if unit tests initialize the live ChatGPT OAuth broker.

use std::ffi::{OsStr, OsString};
use std::sync::{Mutex, MutexGuard};

static ENV_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn hermetic() -> bool {
    std::env::var_os("MEDOUSA_TEST_HERMETIC").is_some()
}

/// Skip OS keyring in the hermetic lib suite (treat as unconfigured).
#[allow(dead_code)]
pub(crate) fn refuse_host_keyring() -> Result<(), keyring::Error> {
    if hermetic() {
        return Err(keyring::Error::NoEntry);
    }
    Ok(())
}

pub(crate) fn panic_if_hermetic_host(what: &str) {
    if hermetic() {
        panic!("TEST-001 hermetic suite forbids host {what}; inject fakes or #[ignore] the test");
    }
}

pub(crate) struct EnvLock(#[allow(dead_code)] MutexGuard<'static, ()>);

pub(crate) fn lock() -> EnvLock {
    EnvLock(
        ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    )
}

pub(crate) struct EnvVarGuard {
    key: String,
    previous: Option<OsString>,
    _lock: EnvLock,
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        // SAFETY: held the suite env lock for the lifetime of this guard.
        unsafe {
            match &self.previous {
                Some(value) => std::env::set_var(&self.key, value),
                None => std::env::remove_var(&self.key),
            }
        }
    }
}

pub(crate) fn set_var(key: impl Into<String>, value: impl AsRef<OsStr>) -> EnvVarGuard {
    let key = key.into();
    let lock = lock();
    let previous = std::env::var_os(&key);
    unsafe { std::env::set_var(&key, value) };
    EnvVarGuard {
        key,
        previous,
        _lock: lock,
    }
}
