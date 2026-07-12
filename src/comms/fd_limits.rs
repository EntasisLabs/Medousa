//! File-descriptor limit hardening.
//!
//! The daemon multiplexes many sockets (HTTP clients, SSE streams, DB, Iroh,
//! grapheme). Under a reconnect storm the default soft `RLIMIT_NOFILE` (often
//! 256 on macOS) is exhausted, which is the FD-pressure failure class behind
//! the observed wasix panics and persist drops. Raise the soft limit toward the
//! hard limit at startup.

/// Default target soft limit we try to reach (capped by the hard limit).
pub const DEFAULT_TARGET_NOFILE: u64 = 16_384;

/// Outcome of attempting to raise the soft FD limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NofileLimits {
    pub soft: u64,
    pub hard: u64,
}

/// Raise the soft `RLIMIT_NOFILE` toward `min(target, hard)`. Returns the
/// resulting `(soft, hard)` limits. No-op (returns current limits) if the soft
/// limit is already at least `target`.
#[cfg(unix)]
pub fn raise_nofile_limit(target: u64) -> std::io::Result<NofileLimits> {
    // SAFETY: getrlimit/setrlimit with a valid resource id and initialized
    // rlimit struct; we read errno via std::io::Error::last_os_error.
    unsafe {
        let mut limit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) != 0 {
            return Err(std::io::Error::last_os_error());
        }

        let hard = limit.rlim_max;
        // `RLIM_INFINITY` means "no hard cap"; honor the requested target then.
        let hard_cap = if hard == libc::RLIM_INFINITY {
            target as libc::rlim_t
        } else {
            hard
        };
        let desired = (target as libc::rlim_t).min(hard_cap);

        if limit.rlim_cur >= desired {
            return Ok(NofileLimits {
                soft: limit.rlim_cur,
                hard: rlim_to_u64(hard),
            });
        }

        limit.rlim_cur = desired;
        if libc::setrlimit(libc::RLIMIT_NOFILE, &limit) != 0 {
            return Err(std::io::Error::last_os_error());
        }

        let mut applied = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut applied) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(NofileLimits {
            soft: applied.rlim_cur,
            hard: rlim_to_u64(applied.rlim_max),
        })
    }
}

#[cfg(unix)]
fn rlim_to_u64(value: libc::rlim_t) -> u64 {
    if value == libc::RLIM_INFINITY {
        u64::MAX
    } else {
        value
    }
}

/// Non-unix fallback: nothing to raise.
#[cfg(not(unix))]
pub fn raise_nofile_limit(_target: u64) -> std::io::Result<NofileLimits> {
    Ok(NofileLimits {
        soft: 0,
        hard: 0,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn raising_does_not_lower_soft_limit() {
        let before = raise_nofile_limit(DEFAULT_TARGET_NOFILE).expect("raise");
        // Asking for a tiny target must never reduce the soft limit.
        let after = raise_nofile_limit(1).expect("no-op");
        assert!(after.soft >= before.soft.min(1).max(before.soft));
        assert!(after.soft >= 1);
    }

    #[test]
    fn never_exceeds_hard_limit() {
        let limits = raise_nofile_limit(u64::MAX / 2).expect("raise to hard");
        assert!(limits.soft <= limits.hard);
    }
}
