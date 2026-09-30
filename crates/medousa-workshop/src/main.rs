use std::io::{self, Write};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

fn main() -> ExitCode {
    let execution = medousa_workshop::execute_from_env();
    println!("{}", execution.line);
    let _ = io::stdout().flush();
    if execution.reside {
        // Portal kills the wasmer process on release. Sleep instead of
        // polling a terminal; there is no interactive prompt.
        loop {
            thread::sleep(Duration::from_secs(30));
        }
    }
    ExitCode::from(execution.exit_code)
}
