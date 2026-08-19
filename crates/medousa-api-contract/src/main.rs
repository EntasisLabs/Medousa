use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use medousa_api_contract::{ContractRegistry, generate_artifacts};

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("generate") {
        eprintln!("usage: medousa-api-contract generate --input <ir.json> --out-dir <dir>");
        return ExitCode::from(2);
    }
    let mut input = None;
    let mut out_dir = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--input" => {
                input = args.get(index + 1).cloned();
                index += 2;
            }
            "--out-dir" => {
                out_dir = args.get(index + 1).cloned();
                index += 2;
            }
            other => {
                eprintln!("unknown argument {other}");
                return ExitCode::from(2);
            }
        }
    }
    let Some(input) = input else {
        eprintln!("missing --input");
        return ExitCode::from(2);
    };
    let Some(out_dir) = out_dir else {
        eprintln!("missing --out-dir");
        return ExitCode::from(2);
    };
    let payload = match fs::read_to_string(&input) {
        Ok(payload) => payload,
        Err(error) => {
            eprintln!("read {input}: {error}");
            return ExitCode::from(1);
        }
    };
    let operations: Vec<medousa_api_contract::OperationSpec> = match serde_json::from_str(&payload)
    {
        Ok(operations) => operations,
        Err(error) => {
            eprintln!("parse IR: {error}");
            return ExitCode::from(1);
        }
    };
    let mut registry = ContractRegistry::new();
    for spec in operations {
        if let Err(error) = registry.register(spec) {
            eprintln!("{error}");
            return ExitCode::from(1);
        }
    }
    let artifacts = generate_artifacts(&registry);
    let out = PathBuf::from(out_dir);
    if let Err(error) = fs::create_dir_all(&out) {
        eprintln!("{error}");
        return ExitCode::from(1);
    }
    let writes = [
        ("openapi.json", artifacts.openapi_json.as_bytes()),
        (
            "route-inventory.json",
            artifacts.route_inventory_json.as_bytes(),
        ),
        ("ops.rs", artifacts.rust_ops.as_bytes()),
        ("ops.py", artifacts.python_ops.as_bytes()),
        ("ops.ts", artifacts.typescript_ops.as_bytes()),
        ("daemon_operation.rs", artifacts.tauri_enum.as_bytes()),
    ];
    for (name, bytes) in writes {
        if let Err(error) = fs::write(out.join(name), bytes) {
            eprintln!("write {name}: {error}");
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}
