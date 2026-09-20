// swift-tools-version:5.3

import PackageDescription

let package = Package(
    name: "tauri-plugin-ios-webview-insets",
    platforms: [.iOS(.v13)],
    products: [
        .library(
            name: "tauri-plugin-ios-webview-insets",
            type: .static,
            targets: ["tauri-plugin-ios-webview-insets"]),
    ],
    dependencies: [
        .package(name: "Tauri", path: "../.tauri/tauri-api"),
    ],
    targets: [
        .target(
            name: "tauri-plugin-ios-webview-insets",
            dependencies: [.byName(name: "Tauri")],
            path: "Sources"),
    ]
)
