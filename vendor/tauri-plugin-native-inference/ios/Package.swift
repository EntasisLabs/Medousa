// swift-tools-version: 6.1

import PackageDescription

let package = Package(
  name: "tauri-plugin-native-inference",
  platforms: [
    .macOS(.v14),
    .iOS(.v17),
  ],
  products: [
    .library(
      name: "tauri-plugin-native-inference",
      type: .static,
      targets: ["tauri-plugin-native-inference"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api"),
    .package(url: "https://github.com/ml-explore/mlx-swift-lm", exact: "3.31.4"),
    .package(url: "https://github.com/huggingface/swift-huggingface", exact: "0.9.0"),
    .package(url: "https://github.com/huggingface/swift-transformers", exact: "1.3.3"),
  ],
  targets: [
    .target(
      name: "tauri-plugin-native-inference",
      dependencies: [
        .byName(name: "Tauri"),
        .product(name: "MLXLLM", package: "mlx-swift-lm"),
        .product(name: "MLXVLM", package: "mlx-swift-lm"),
        .product(name: "MLXLMCommon", package: "mlx-swift-lm"),
        .product(name: "HuggingFace", package: "swift-huggingface"),
        .product(name: "Tokenizers", package: "swift-transformers"),
      ],
      path: "Sources")
  ],
  swiftLanguageModes: [.v5]
)
