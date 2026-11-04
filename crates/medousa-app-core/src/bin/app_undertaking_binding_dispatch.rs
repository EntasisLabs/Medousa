//! Stdio JSON dispatcher for the undertaking binding application core.

use std::io::{self, BufRead, Write};

use medousa_app_core::{UndertakingBindingCommand, UndertakingBindingCore};

fn main() {
    let mut core = UndertakingBindingCore::new("");
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(value) => value,
            Err(error) => {
                eprintln!("stdin error: {error}");
                std::process::exit(1);
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        let command: UndertakingBindingCommand = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                let rejected = serde_json::json!({
                    "type": "rejected",
                    "reason": format!("invalid command json: {error}"),
                });
                writeln!(stdout, "{rejected}").ok();
                stdout.flush().ok();
                continue;
            }
        };
        match core.dispatch(command) {
            Ok(update) => {
                let encoded = serde_json::to_string(&update).expect("serialize update");
                writeln!(stdout, "{encoded}").ok();
            }
            Err(error) => {
                let rejected = serde_json::json!({
                    "type": "rejected",
                    "reason": error.to_string(),
                });
                writeln!(stdout, "{rejected}").ok();
            }
        }
        stdout.flush().ok();
    }
}
