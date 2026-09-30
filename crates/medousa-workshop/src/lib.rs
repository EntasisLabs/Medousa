//! Workshop guest for `wasmer run`.
//!
//! Portal supervises this process. The guest reads the sandbox environment,
//! probes a Medousa daemon over cleartext HTTP when `MEDOUSA_CONNECT_URL` is
//! an origin, and stays in the foreground until that supervisor kills it.
//!
//! It is not the browser Personal workshop (`wasm32-unknown-unknown`) and it
//! is not `medousa_daemon`. Iroh, pairing completion, vault, and turns stay on
//! the host daemon or Home. See `docs/cookbook/wasmer-workshop.md`.

mod contract;
mod http;
mod probe;

pub use probe::Execution;

/// Read the Portal environment and build the one-line result.
///
/// `reside` is true when the process should sleep until it is killed. Contract
/// errors and `MEDOUSA_WORKSHOP_ONCE` leave `reside` false.
pub fn execute_from(get: impl Fn(&str) -> Option<String>) -> Execution {
    match contract::parse_launch(&get) {
        Ok(launch) => probe::execute(&launch),
        Err(error) => Execution {
            exit_code: 2,
            line: error.to_line(),
            reside: false,
        },
    }
}

pub fn execute_from_env() -> Execution {
    execute_from(|key| std::env::var(key).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_kind_exits_before_residing() {
        let execution = execute_from(|key| (key == "MEDOUSA_KIND").then(|| "browser".to_string()));
        assert_eq!(execution.exit_code, 2);
        assert!(!execution.reside);
        let value: serde_json::Value = serde_json::from_str(&execution.line).unwrap();
        assert_eq!(value["event"], "workshop.error");
        assert_eq!(value["error"], "invalid_kind");
    }
}
