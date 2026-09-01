fn main() {
    println!("cargo:rerun-if-changed=ios/Sources/NativeInferencePlugin.swift");
    println!("cargo:rerun-if-changed=ios/Package.swift");

    // Tauri bundles the Swift package products into this crate's rlib, but
    // SwiftPM's linker settings do not travel with those object files. Keep
    // the native archive self-contained by forwarding the frameworks used by
    // MLX's Cmlx target (including Accelerate's NEWLAPACK entry points).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("ios") {
        // `tauri-utils` otherwise gives direct Cargo builds an iOS 13 target,
        // overriding this package's declared iOS 17 floor. Xcode already
        // supplies this variable, so retain its project-selected value.
        if std::env::var_os("IPHONEOS_DEPLOYMENT_TARGET").is_none() {
            std::env::set_var("IPHONEOS_DEPLOYMENT_TARGET", "17.0");
        }
        for framework in ["Foundation", "Metal", "Accelerate"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
        println!("cargo:rustc-link-lib=c++");
    }

    tauri_plugin::Builder::new(&[
        "hardware",
        "catalog",
        "models",
        "start_download",
        "download_status",
        "load_model",
        "status",
        "unload",
        "remove_model",
        "generate",
        "cancel",
    ])
    .ios_path("ios")
    .build();
}
