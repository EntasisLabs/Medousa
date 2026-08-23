#!/usr/bin/env bash
# Compile and link the native iOS daemon dependency candidates. The
# Grapheme probe is intentionally built separately so Stasis's current host
# feature cannot mask an accidental loss of the lean SDK profile.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

targets=(aarch64-apple-ios aarch64-apple-ios-sim)
if [[ -n "${MEDOUSA_IOS_TARGETS:-}" ]]; then
  read -r -a targets <<<"${MEDOUSA_IOS_TARGETS}"
fi
profile="${MEDOUSA_IOS_PROFILE:-ios-probe}"

installed_targets="$(rustup target list --installed)"
for target in "${targets[@]}"; do
  if ! grep -qx "${target}" <<<"${installed_targets}"; then
    echo "check-ios-embedded-deps: missing Rust target ${target}" >&2
    echo "install it with: rustup target add ${target}" >&2
    exit 1
  fi

  echo "check-ios-embedded-deps: medousa-engine -> ${target}"
  cargo check --locked -p medousa-engine --target "${target}" --profile "${profile}"

  echo "check-ios-embedded-deps: production foreground runtime -> ${target}"
  cargo check --locked -p medousa-runtime --target "${target}" --profile "${profile}"

  echo "check-ios-embedded-deps: Stasis + Locus native -> ${target}"
  cargo build --locked -p medousa-ios-dependency-probe \
    --target "${target}" \
    --profile "${profile}" \
    --no-default-features \
    --features stasis-native

  echo "check-ios-embedded-deps: lean Grapheme -> ${target}"
  cargo build --locked -p medousa-ios-dependency-probe \
    --target "${target}" \
    --profile "${profile}" \
    --no-default-features \
    --features grapheme-portable

  echo "check-ios-embedded-deps: Apple Keychain linkage -> ${target}"
  cargo build --locked -p medousa-ios-dependency-probe \
    --target "${target}" \
    --profile "${profile}" \
    --no-default-features \
    --features keychain
done

echo "check-ios-embedded-deps: OK"
