//! Stdio JSON dispatcher for the application chat core (tests / non-Tauri harnesses).
//!
//! Protocol: one `ChatCommand` JSON object per stdin line → one `ChatUpdate` JSON
//! object per stdout line. Process-local `ChatCore` state persists across lines.

use std::io::{self, BufRead, Write};

use medousa_app_core::{ChatCommand, ChatCore};

fn main() {
    let mut core = ChatCore::new();
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
        let command: ChatCommand = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(error) => {
                let rejected = serde_json::json!({
                    "type": "rejected",
                    "session_id": "",
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
                    "session_id": "",
                    "reason": error.to_string(),
                });
                writeln!(stdout, "{rejected}").ok();
            }
        }
        stdout.flush().ok();
    }
}
