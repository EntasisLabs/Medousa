fn main() {
    println!("cargo:rerun-if-changed=ios/Sources/NativeInferencePlugin.swift");
    println!("cargo:rerun-if-changed=ios/Package.swift");

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
