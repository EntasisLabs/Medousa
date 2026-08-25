#!/usr/bin/env bash
# Install an Xcode-built simulator bundle and prove the Phase 0 Keychain and
# Rust-startup diagnostics in the app's own sandbox.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
device="${1:-booted}"
app_path="${2:-${repo_root}/apps/medousa-home/src-tauri/gen/apple/build/arm64-sim/Medousa.app}"

if [[ "${app_path}" != /* ]]; then
  app_path="${repo_root}/${app_path}"
fi

if [[ ! -d "${app_path}" || ! -f "${app_path}/Info.plist" ]]; then
  echo "smoke-ios-phase0: app bundle not found: ${app_path}" >&2
  exit 1
fi

bundle_id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "${app_path}/Info.plist")"
executable_name="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "${app_path}/Info.plist")"
executable_path="${app_path}/${executable_name}"
if [[ ! -f "${executable_path}" ]]; then
  echo "smoke-ios-phase0: executable not found: ${executable_path}" >&2
  exit 1
fi

cleanup() {
  xcrun simctl terminate "${device}" "${bundle_id}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

xcrun simctl install "${device}" "${app_path}"
container_path="$(xcrun simctl get_app_container "${device}" "${bundle_id}" data)"
receipt_path="${container_path}/Library/Caches/${bundle_id}/ios-phase0-probe.json"
launch_started_epoch="$(date +%s)"

xcrun simctl launch --terminate-running-process \
  "${device}" \
  "${bundle_id}" \
  --medousa-ios-phase0-probe

fresh_receipt=false
for _attempt in {1..100}; do
  if [[ -f "${receipt_path}" ]]; then
    receipt_modified_epoch="$(stat -f '%m' "${receipt_path}")"
    if (( receipt_modified_epoch >= launch_started_epoch )); then
      fresh_receipt=true
      break
    fi
  fi
  sleep 0.1
done

if [[ "${fresh_receipt}" != true ]]; then
  echo "smoke-ios-phase0: fresh probe receipt not found: ${receipt_path}" >&2
  exit 1
fi

keychain_roundtrip="$(plutil -extract keychain_roundtrip raw -o - "${receipt_path}")"
rust_startup_ms="$(plutil -extract rust_startup_ms raw -o - "${receipt_path}")"
setup_ms="$(plutil -extract setup_ms raw -o - "${receipt_path}")"
if [[ "${keychain_roundtrip}" != "ok" ]]; then
  echo "smoke-ios-phase0: Keychain round trip failed" >&2
  exit 1
fi

read -r bundle_kib _ < <(du -sk "${app_path}")
executable_bytes="$(stat -f '%z' "${executable_path}")"

echo "smoke-ios-phase0: keychain_roundtrip=${keychain_roundtrip}"
echo "smoke-ios-phase0: rust_startup_ms=${rust_startup_ms}"
echo "smoke-ios-phase0: setup_ms=${setup_ms}"
echo "smoke-ios-phase0: bundle_kib=${bundle_kib}"
echo "smoke-ios-phase0: executable_bytes=${executable_bytes}"
