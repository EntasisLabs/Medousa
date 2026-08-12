#!/usr/bin/env bash
# Verify scripts/install.sh works both from a checkout and through stdin.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALLER="${ROOT}/scripts/install.sh"
COMMON_URL="file://${ROOT}/scripts/release/common.sh"

bash -n "${INSTALLER}"

direct_help="$(bash "${INSTALLER}" --help)"
grep -q "Install the Medousa engine package" <<<"${direct_help}"

streamed_help="$(
  MEDOUSA_INSTALL_COMMON_URL="${COMMON_URL}" \
    bash -s -- --help <"${INSTALLER}"
)"
grep -q "Install the Medousa engine package" <<<"${streamed_help}"

probe_state="$(mktemp -d)"
trap 'rm -rf "${probe_state}"' EXIT
set +e
streamed_probe="$(
  MEDOUSA_INSTALL_COMMON_URL="${COMMON_URL}" \
  MEDOUSA_INSTALL_DIR="${probe_state}/bin" \
  MEDOUSA_STATE_DIR="${probe_state}/state" \
    bash -s -- --verify-only <"${INSTALLER}" 2>&1
)"
probe_status=$?
set -e
[[ "${probe_status}" -ne 0 ]]
grep -q "no install record" <<<"${streamed_probe}"
if grep -q "unbound variable" <<<"${streamed_probe}"; then
  echo "streamed installer hit uninitialized state" >&2
  exit 1
fi

echo "ok: install.sh direct and streamed entrypoints"
