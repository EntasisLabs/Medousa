use serde::Serialize;
use std::io::{self, Write};

const DEFAULT_BYTES: usize = 10_000;
const MAX_BYTES: usize = 1_000_000;
const DEFAULT_FRAGMENT_BYTES: usize = 256;
const MAX_FRAGMENT_BYTES: usize = 65_536;

#[derive(Serialize)]
pub struct P02Config {
    bytes: usize,
    fragment_bytes: usize,
}

fn bounded_env(name: &str, fallback: usize, maximum: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|raw| raw.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .map(|value| value.min(maximum))
        .unwrap_or(fallback)
}

#[tauri::command]
fn p02_benchmark_config() -> P02Config {
    P02Config {
        bytes: bounded_env("MEDOUSA_P02_BYTES", DEFAULT_BYTES, MAX_BYTES),
        fragment_bytes: bounded_env(
            "MEDOUSA_P02_FRAGMENT_BYTES",
            DEFAULT_FRAGMENT_BYTES,
            MAX_FRAGMENT_BYTES,
        ),
    }
}

#[tauri::command]
fn p02_benchmark_complete(app: tauri::AppHandle, result: serde_json::Value) {
    println!("MEDOUSA_P02_RESULT={result}");
    let _ = io::stdout().flush();
    app.exit(0);
}

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            p02_benchmark_config,
            p02_benchmark_complete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running packaged P02 benchmark");
}

#[cfg(test)]
mod tests {
    use super::bounded_env;

    #[test]
    fn bounded_env_falls_back_for_missing_values() {
        assert_eq!(bounded_env("MEDOUSA_P02_TEST_MISSING", 256, 1024), 256);
    }
}
