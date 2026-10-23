#!/usr/bin/env bash
# Reject damaged Cargo outputs before they overwrite a working packaged sidecar.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "${ROOT}/scripts/release/common.sh"
fixture_dir="$(mktemp -d)"
trap 'rm -rf "${fixture_dir}"' EXIT
printf '\317\372\355\376' >"${fixture_dir}/mach-o"
printf '\312\376\272\276' >"${fixture_dir}/universal"
printf '\177ELF' >"${fixture_dir}/elf"
printf 'MZ\220\000' >"${fixture_dir}/pe"
printf 'damaged output' >"${fixture_dir}/broken"
medousa_validate_native_binary "${fixture_dir}/mach-o" aarch64-apple-darwin
medousa_validate_native_binary "${fixture_dir}/universal" x86_64-apple-darwin
medousa_validate_native_binary "${fixture_dir}/elf" x86_64-unknown-linux-gnu
medousa_validate_native_binary "${fixture_dir}/pe" x86_64-pc-windows-msvc
for file in broken elf missing; do
  if medousa_validate_native_binary "${fixture_dir}/${file}" aarch64-apple-darwin 2>"${fixture_dir}/error"; then
    echo "unexpectedly accepted ${file} as a macOS executable" >&2
    exit 1
  fi
  grep -q 'invalid native executable' "${fixture_dir}/error"
done
# A rejected prebuilt must not touch the existing repository sidecar.
target=aarch64-apple-darwin
sidecar="${ROOT}/apps/medousa-home/src-tauri/binaries/medousa_daemon-${target}"
before=missing
[[ ! -f "${sidecar}" ]] || before="$(medousa_sha256_file "${sidecar}")"
if CARGO_BUILD_TARGET="${target}" MEDOUSA_SIDECAR_DAEMON="${fixture_dir}/broken" \
  bash "${ROOT}/apps/medousa-home/scripts/prepare-engine-sidecar.sh" >"${fixture_dir}/error" 2>&1; then
  echo "sidecar preparation accepted a corrupt prebuilt" >&2
  exit 1
fi
grep -q 'invalid native executable' "${fixture_dir}/error"
after=missing
[[ ! -f "${sidecar}" ]] || after="$(medousa_sha256_file "${sidecar}")"
[[ "${before}" == "${after}" ]]
echo "check-native-binary: OK"
