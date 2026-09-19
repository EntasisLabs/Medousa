#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
test_dir="$(mktemp -d "${TMPDIR:-/tmp}/medousa-live-tests.XXXXXX")"
trap 'rm -f "$test_dir/lifecycle-tests"; rmdir "$test_dir"' EXIT

xcrun swiftc -o "$test_dir/lifecycle-tests" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/App/MedousaLiveSocketLifecycle.swift" \
  "$repo_root/apps/medousa-home/src-tauri/ios-live-activity/Tests/MedousaLiveSocketLifecycleTests.swift"
"$test_dir/lifecycle-tests"
