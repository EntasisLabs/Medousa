use std::io::{self, BufRead, BufReader, Write};
use std::process::ExitCode;

use medousa_computer_bridge::{
    COMPUTER_DRIVER_PROTOCOL_VERSION, ComputerDriverRequest, ComputerDriverRequestEnvelope,
    ComputerDriverResponseEnvelope, ComputerDriverResponseResult,
    MAX_COMPUTER_DRIVER_MESSAGE_BYTES,
};

mod platform;

fn main() -> ExitCode {
    let driver = platform::NativeComputerDriver::new();
    match std::env::args().nth(1).as_deref().unwrap_or("serve") {
        "serve" => match serve(&driver) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("medousa-computer: {error}");
                ExitCode::FAILURE
            }
        },
        "preflight" => match serde_json::to_writer_pretty(io::stdout(), &driver.preflight()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("medousa-computer: serialize preflight: {error}");
                ExitCode::FAILURE
            }
        },
        command => {
            eprintln!("medousa-computer: unknown command '{command}' (use serve or preflight)");
            ExitCode::FAILURE
        }
    }
}

fn serve(driver: &platform::NativeComputerDriver) -> Result<(), String> {
    let stdin = io::stdin();
    let mut reader = BufReader::new(stdin.lock());
    let stdout = io::stdout();
    let mut writer = stdout.lock();

    loop {
        match read_bounded_line(&mut reader)? {
            BoundedLine::Eof => return Ok(()),
            BoundedLine::TooLong => write_response(
                &mut writer,
                &ComputerDriverResponseEnvelope::error(
                    "invalid",
                    "message_too_large",
                    "computer driver request exceeded its framing limit",
                    false,
                ),
            )?,
            BoundedLine::Line(line) => {
                let response = handle_line(driver, &line);
                write_response(&mut writer, &response)?;
            }
        }
    }
}

fn handle_line(
    driver: &platform::NativeComputerDriver,
    line: &[u8],
) -> ComputerDriverResponseEnvelope {
    let request: ComputerDriverRequestEnvelope = match serde_json::from_slice(line) {
        Ok(request) => request,
        Err(error) => {
            return ComputerDriverResponseEnvelope::error(
                "invalid",
                "invalid_request",
                format!("invalid computer driver request: {error}"),
                false,
            );
        }
    };
    if request.protocol_version != COMPUTER_DRIVER_PROTOCOL_VERSION {
        return ComputerDriverResponseEnvelope::error(
            request.request_id,
            "unsupported_protocol",
            format!(
                "computer driver protocol {} is unsupported",
                request.protocol_version
            ),
            false,
        );
    }
    if !valid_request_id(&request.request_id) {
        return ComputerDriverResponseEnvelope::error(
            "invalid",
            "invalid_request_id",
            "computer driver request id is invalid",
            false,
        );
    }

    match request.request {
        ComputerDriverRequest::Preflight => ComputerDriverResponseEnvelope::success(
            request.request_id,
            ComputerDriverResponseResult::Preflight {
                report: driver.preflight(),
            },
        ),
        ComputerDriverRequest::Observe { request: observe } => {
            let request_id = request.request_id;
            match driver.observe(observe) {
                Ok(observation) => ComputerDriverResponseEnvelope::success(
                    request_id,
                    ComputerDriverResponseResult::Observation { observation },
                ),
                Err(error) => ComputerDriverResponseEnvelope::error(
                    request_id,
                    error.code,
                    error.message,
                    error.retryable,
                ),
            }
        }
    }
}

fn valid_request_id(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.len() <= 256 && !trimmed.chars().any(char::is_control)
}

fn write_response(
    writer: &mut impl Write,
    response: &ComputerDriverResponseEnvelope,
) -> Result<(), String> {
    let encoded = serde_json::to_vec(response)
        .map_err(|error| format!("serialize computer driver response: {error}"))?;
    if encoded.len() > MAX_COMPUTER_DRIVER_MESSAGE_BYTES {
        return Err("computer driver response exceeded its framing limit".to_string());
    }
    writer
        .write_all(&encoded)
        .and_then(|()| writer.write_all(b"\n"))
        .and_then(|()| writer.flush())
        .map_err(|error| format!("write computer driver response: {error}"))
}

enum BoundedLine {
    Eof,
    Line(Vec<u8>),
    TooLong,
}

fn read_bounded_line(reader: &mut impl BufRead) -> Result<BoundedLine, String> {
    let mut line = Vec::new();
    let mut too_long = false;
    loop {
        let buffer = reader
            .fill_buf()
            .map_err(|error| format!("read computer driver request: {error}"))?;
        if buffer.is_empty() {
            return if line.is_empty() && !too_long {
                Ok(BoundedLine::Eof)
            } else if too_long {
                Ok(BoundedLine::TooLong)
            } else {
                Ok(BoundedLine::Line(line))
            };
        }
        let consumed = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |index| index + 1);
        if !too_long {
            let content_len = consumed - usize::from(buffer[consumed - 1] == b'\n');
            if line.len().saturating_add(content_len) > MAX_COMPUTER_DRIVER_MESSAGE_BYTES {
                too_long = true;
                line.clear();
            } else {
                line.extend_from_slice(&buffer[..content_len]);
            }
        }
        let ended = buffer[consumed - 1] == b'\n';
        reader.consume(consumed);
        if ended {
            return if too_long {
                Ok(BoundedLine::TooLong)
            } else {
                Ok(BoundedLine::Line(line))
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_keeps_protocol_lines_separate() {
        let input = b"one\ntwo\n";
        let mut reader = BufReader::new(input.as_slice());
        assert!(matches!(
            read_bounded_line(&mut reader).expect("first line"),
            BoundedLine::Line(line) if line == b"one"
        ));
        assert!(matches!(
            read_bounded_line(&mut reader).expect("second line"),
            BoundedLine::Line(line) if line == b"two"
        ));
        assert!(matches!(
            read_bounded_line(&mut reader).expect("eof"),
            BoundedLine::Eof
        ));
    }
}
