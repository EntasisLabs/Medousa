fn main() {
    embed_macos_dev_info_plist();
    #[cfg(target_os = "ios")]
    println!("cargo:rustc-link-lib=framework=WebKit");
    tauri_build::build();
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
