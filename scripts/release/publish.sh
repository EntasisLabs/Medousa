#!/usr/bin/env bash
# Create a git tag and GitHub Release for Medousa.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=common.sh
source "${SCRIPT_DIR}/common.sh"

TAG=""
LOCAL_ONLY=0
CI_MODE=0
DRY_RUN=0
SKIP_TAG_PUSH=0

usage() {
  cat <<'EOF'
Usage: scripts/release/publish.sh [options] [vX.Y.Z]

Options:
  --local-only    Build + package native target and upload to GitHub Release
  --ci            Create/push tag only; GitHub Actions builds the matrix (recommended)
  --dry-run       Print actions without mutating git or GitHub
  --skip-push     Create annotated tag locally but do not push (with --ci)
  -h, --help      Show this help

If tag is omitted, version is read from Cargo.toml (as vX.Y.Z).

Production flow:
  ./scripts/release/publish.sh --ci v0.1.0

Smoke flow (single platform):
  ./scripts/release/publish.sh --local-only v0.1.0
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --local-only)
      LOCAL_ONLY=1
      shift
      ;;
    --ci)
      CI_MODE=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --skip-push)
      SKIP_TAG_PUSH=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    v* | [0-9]*)
      TAG="$1"
      shift
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "${LOCAL_ONLY}" -eq 1 && "${CI_MODE}" -eq 1 ]]; then
  echo "error: use either --local-only or --ci, not both" >&2
  exit 1
fi

if [[ "${LOCAL_ONLY}" -eq 0 && "${CI_MODE}" -eq 0 ]]; then
  CI_MODE=1
fi

medousa_require_cmd git
ROOT="$(medousa_repo_root)"
cd "${ROOT}"

medousa_assert_versions_match
VERSION="$(medousa_version)"

if [[ -z "${TAG}" ]]; then
  TAG="$(medousa_tag_for_version "${VERSION}")"
fi

TAG_VERSION="$(medousa_version_from_tag "${TAG}")"
if [[ "${TAG_VERSION}" != "${VERSION}" ]]; then
  echo "error: tag ${TAG} (${TAG_VERSION}) != Cargo.toml (${VERSION})" >&2
  exit 1
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "error: working tree is not clean — commit or stash before publishing" >&2
  git status --short >&2
  exit 1
fi

run() {
  if [[ "${DRY_RUN}" -eq 1 ]]; then
    echo "[dry-run] $*"
  else
    "$@"
  fi
}

medousa_log "publish ${TAG} (mode: $([[ ${CI_MODE} -eq 1 ]] && echo ci || echo local-only))"

if [[ "${CI_MODE}" -eq 1 ]]; then
  if git rev-parse "${TAG}" >/dev/null 2>&1; then
    medousa_log "tag ${TAG} already exists locally"
  else
    run git tag -a "${TAG}" -m "Release ${TAG}"
  fi

  if [[ "${SKIP_TAG_PUSH}" -eq 0 ]]; then
    run git push origin "${TAG}"
    medousa_log "pushed ${TAG} — GitHub Actions will build and publish release assets"
    medousa_log "track: https://github.com/${MEDOUSA_GITHUB_REPO}/actions"
  else
    medousa_log "skipped tag push (--skip-push)"
  fi
  exit 0
fi

# --local-only: build native, package, gh release
medousa_require_cmd gh

TARGET="$(medousa_host_target)"
"${SCRIPT_DIR}/build.sh" --target "${TARGET}"
"${SCRIPT_DIR}/package.sh" --target "${TARGET}"

DIST="${ROOT}/dist"
ARCHIVE_NAME="$(medousa_asset_archive_name "${VERSION}" "${TARGET}")"
ARCHIVE_PATH="${DIST}/${ARCHIVE_NAME}"
CHECKSUMS_FILE="${DIST}/SHA256SUMS"

if [[ ! -f "${ARCHIVE_PATH}" ]]; then
  echo "error: missing archive ${ARCHIVE_PATH}" >&2
  exit 1
fi

if git rev-parse "${TAG}" >/dev/null 2>&1; then
  medousa_log "tag ${TAG} already exists"
else
  run git tag -a "${TAG}" -m "Release ${TAG}"
  run git push origin "${TAG}"
fi

if gh release view "${TAG}" --repo "${MEDOUSA_GITHUB_REPO}" >/dev/null 2>&1; then
  medousa_log "uploading assets to existing release ${TAG}"
  run gh release upload "${TAG}" "${ARCHIVE_PATH}" "${CHECKSUMS_FILE}" --repo "${MEDOUSA_GITHUB_REPO}" --clobber
else
  run gh release create "${TAG}" \
    "${ARCHIVE_PATH}" \
    "${CHECKSUMS_FILE}" \
    --repo "${MEDOUSA_GITHUB_REPO}" \
    --title "Medousa ${TAG}" \
    --notes "Local smoke release (${TARGET}). Full matrix builds via CI on tag push."
fi

medousa_log "release ${TAG} published (${ARCHIVE_NAME})"
