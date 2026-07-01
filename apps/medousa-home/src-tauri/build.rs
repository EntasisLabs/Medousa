fn main() {
    embed_macos_dev_info_plist();
    if is_ios_build_target() {
        compile_ios_live_activity();
        println!("cargo:rustc-link-lib=framework=WebKit");
    }
    tauri_build::build();
}

fn is_ios_build_target() -> bool {
    std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios")
}

fn compile_ios_live_activity() {
    use std::path::Path;
    use std::process::Command;

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let swift_root = manifest_dir.join("ios-live-activity");
    let sources = [
        swift_root.join("Shared/MedousaWorkAttributes.swift"),
        swift_root.join("App/MedousaLiveActivityManager.swift"),
        swift_root.join("App/MedousaLiveActivityBridge.swift"),
    ];

    for source in &sources {
        if !source.exists() {
            eprintln!(
                "cargo:warning=Live Activity Swift source missing: {}",
                source.display()
            );
            return;
        }
        println!("cargo:rerun-if-changed={}", source.display());
    }

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let lib_path = Path::new(&out_dir).join("libMedousaLiveActivity.a");

    let target = std::env::var("TARGET").unwrap_or_default();
    let sdk = if target.contains("ios") {
        if target.contains("sim") {
            "iphonesimulator"
        } else {
            "iphoneos"
        }
    } else {
        eprintln!("cargo:warning=Skipping Live Activity Swift build for non-iOS target");
        return;
    };

    let sdk_path = match Command::new("xcrun")
        .args(["--sdk", sdk, "--show-sdk-path"])
        .output()
    {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => {
            eprintln!("cargo:warning=Could not resolve iOS SDK — Live Activity native bridge skipped");
            return;
        }
    };

    let min_version = "16.1";
    let swift_target = if sdk == "iphonesimulator" {
        format!("arm64-apple-ios{min_version}-simulator")
    } else {
        format!("arm64-apple-ios{min_version}")
    };

    let mut args = vec![
        "swiftc".to_string(),
        "-emit-library".to_string(),
        "-static".to_string(),
        "-O".to_string(),
        "-sdk".to_string(),
        sdk_path,
        "-target".to_string(),
        swift_target,
        "-o".to_string(),
        lib_path.display().to_string(),
    ];
    for source in &sources {
        args.push(source.display().to_string());
    }

    let status = Command::new("xcrun").args(&args).status();
    match status {
        Ok(s) if s.success() => {
            println!("cargo:rustc-cfg=live_activity_native");
            println!("cargo:rustc-link-search=native={out_dir}");
            println!("cargo:rustc-link-lib=static=MedousaLiveActivity");
            println!("cargo:rustc-link-lib=framework=ActivityKit");
            println!("cargo:rustc-link-lib=framework=WidgetKit");
            println!("cargo:rustc-link-lib=framework=SwiftUI");
        }
        Ok(s) => {
            eprintln!(
                "cargo:warning=Live Activity Swift compile failed (exit {}); bridge will be unavailable",
                s
            );
        }
        Err(err) => {
            eprintln!("cargo:warning=Could not run xcrun swiftc: {err}");
        }
    }
}

/// `tauri dev` runs `target/debug/medousa-home`, not `Medousa.app`. Embed privacy
/// strings into the binary so macOS TCC can read them. Target the binary link only
/// (this crate also builds staticlib/cdylib for iOS).
#[cfg(target_os = "macos")]
fn embed_macos_dev_info_plist() {
    use std::path::Path;

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let plist_path = manifest_dir.join("Info.plist");
    if !plist_path.exists() {
        return;
    }

    let canonical = plist_path
        .canonicalize()
        .unwrap_or_else(|err| panic!("failed to resolve {}: {err}", plist_path.display()));

    println!(
        "cargo:rustc-link-arg-bin=medousa-home=-Wl,-sectcreate,__TEXT,__info_plist,{}",
        canonical.display()
    );
    println!("cargo:rerun-if-changed={}", plist_path.display());
}

#[cfg(not(target_os = "macos"))]
fn embed_macos_dev_info_plist() {}
