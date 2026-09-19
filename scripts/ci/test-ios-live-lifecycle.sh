#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
test_dir="$(mktemp -d "${TMPDIR:-/tmp}/medousa-live-tests.XXXXXX")"
trap 'rm -f "$test_dir/lifecycle-tests"; rmdir "$test_dir"' EXIT

xcrun swiftc -o "$test_dir/lifecycle-tests" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/App/MedousaLiveSocketLifecycle.swift" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/Tests/MedousaLiveSocketLifecycleTests.swift"
"$test_dir/lifecycle-tests"

# Type-check the native socket/audio coordinator against the iOS SDK. These
# classes are intentionally not executable in the macOS lifecycle harness.
simulator_sdk="$(xcrun --sdk iphonesimulator --show-sdk-path)"
xcrun swiftc -typecheck \
  -sdk "$simulator_sdk" \
  -target arm64-apple-ios17.0-simulator \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/Shared/MedousaWorkAttributes.swift" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/Shared/MedousaWidgetSnapshot.swift" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/App/MedousaWidgetSnapshotStore.swift" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/App/MedousaLiveActivityManager.swift" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/App/MedousaLiveSocketLifecycle.swift" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/App/MedousaLiveSocketTransport.swift" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/App/MedousaLiveNativeAudioEngine.swift" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/App/MedousaLiveVoiceSessionManager.swift"
echo "Native Live audio type-check passed"
