//! On wasm32, `std::time::Instant::now` panics (`unreachable` in the browser).
//! Tokio's clock calls that stub. Discover the current libstd symbol names and
//! override them with a `performance.now` / `Date.now` clock at link time.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let exports = out_dir.join("wasm_clock_exports.rs");
    if target != "wasm32-unknown-unknown" {
        std::fs::write(&exports, "// native build has no wasm clock override\n").ok();
        return;
    }

    println!("cargo:rustc-link-arg=--allow-multiple-definition");
    let rlib =
        libstd_rlib().expect("wasm32 libstd rlib (rustup target add wasm32-unknown-unknown)");
    println!("cargo:rerun-if-changed={}", rlib.display());
    let bytes = std::fs::read(&rlib).expect("read libstd");
    let instant_now = find_std_time_symbol(&bytes, "7Instant3now")
        .expect("std::time::Instant::now symbol in libstd");
    let system_now = find_std_time_symbol(&bytes, "10SystemTime3now")
        .expect("std::time::SystemTime::now symbol in libstd");
    let instant_elapsed = find_std_time_symbol(&bytes, "7Instant7elapsed");

    let mut src = String::new();
    src.push_str(&export_fn(
        &instant_now,
        "medousa_std_instant_now",
        "out: *mut u8",
        "crate::wasm_clock::write_instant_now(out);",
    ));
    src.push_str(&export_fn(
        &system_now,
        "medousa_std_system_time_now",
        "out: *mut u8",
        "crate::wasm_clock::write_system_time_now(out);",
    ));
    if let Some(elapsed) = instant_elapsed {
        src.push_str(&export_fn(
            &elapsed,
            "medousa_std_instant_elapsed",
            "out: *mut u8, this: *const u8",
            "crate::wasm_clock::write_instant_elapsed(out, this);",
        ));
    }
    std::fs::write(&exports, src).expect("write wasm clock exports");
}

fn export_fn(symbol: &str, name: &str, args: &str, body: &str) -> String {
    format!(
        "#[allow(dead_code, clippy::missing_safety_doc)]\n#[unsafe(export_name = \"{symbol}\")]\npub unsafe extern \"C\" fn {name}({args}) {{\n    {body}\n}}\n"
    )
}

fn libstd_rlib() -> Option<PathBuf> {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_string());
    let output = Command::new(rustc)
        .args(["--print", "sysroot"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sysroot = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    let dir = sysroot.join("lib/rustlib/wasm32-unknown-unknown/lib");
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("libstd-") && name.ends_with(".rlib") {
            return Some(entry.path());
        }
    }
    None
}

fn find_std_time_symbol(bytes: &[u8], suffix: &str) -> Option<String> {
    let needle = suffix.as_bytes();
    let mut from = 0;
    while let Some(rel) = find_slice(&bytes[from..], needle) {
        let at = from + rel;
        let after = at + needle.len();
        if after < bytes.len() && is_symbol_byte(bytes[after]) {
            from = at + 1;
            continue;
        }
        let mut begin = at;
        while begin > 0 && is_symbol_byte(bytes[begin - 1]) {
            begin -= 1;
        }
        if let Ok(symbol) = std::str::from_utf8(&bytes[begin..after])
            && symbol.starts_with("_R")
            && symbol.contains("3std4time")
            && !symbol.contains("unsupported")
        {
            return Some(symbol.to_string());
        }
        from = at + 1;
    }
    None
}

fn find_slice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn is_symbol_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
