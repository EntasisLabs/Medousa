#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <app-resources-directory>" >&2
  exit 2
fi

resources_dir="$1"
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../../.." && pwd)"
target_root="${CARGO_TARGET_DIR:-${MEDOUSA_CARGO_TARGET_DIR:-$(cd "${repo_root}/.." && pwd)/.cache/cargo-target}}"
package_resolved="${repo_root}/vendor/tauri-plugin-native-inference/ios/Package.resolved"

mlx_revision="$(awk '
  /"identity"[[:space:]]*:[[:space:]]*"mlx-swift"/ { in_mlx = 1 }
  in_mlx && /"revision"[[:space:]]*:/ {
    line = $0
    sub(/^.*"revision"[[:space:]]*:[[:space:]]*"/, "", line)
    sub(/".*$/, "", line)
    print line
    exit
  }
' "${package_resolved}")"

if [[ -z "${mlx_revision}" ]]; then
  echo "[mlx-metal] could not resolve the pinned mlx-swift revision" >&2
  exit 1
fi

mlx_checkout=""
while IFS= read -r candidate; do
  checkout="${candidate%/Source/Cmlx/mlx/mlx/backend/metal/kernels}"
  if [[ "$(git -C "${checkout}" rev-parse HEAD 2>/dev/null || true)" == "${mlx_revision}" ]]; then
    mlx_checkout="${checkout}"
    break
  fi
done < <(find "${target_root}" -type d \
  -path '*/checkouts/mlx-swift/Source/Cmlx/mlx/mlx/backend/metal/kernels' \
  -print 2>/dev/null)

if [[ -z "${mlx_checkout}" ]]; then
  echo "[mlx-metal] mlx-swift ${mlx_revision} was not found under ${target_root}" >&2
  exit 1
fi

mlx_source_root="${mlx_checkout}/Source/Cmlx/mlx"
kernels_dir="${mlx_source_root}/mlx/backend/metal/kernels"
sdk_version="$(xcrun --sdk iphoneos --show-sdk-version)"
deployment_target="${IPHONEOS_DEPLOYMENT_TARGET:-17.0}"
cache_dir="${target_root}/native-inference-metal/${mlx_revision}-iphoneos${sdk_version}-min${deployment_target}"
cached_library="${cache_dir}/mlx.metallib"

if [[ ! -f "${cached_library}" ]]; then
  mkdir -p "${cache_dir}"
  build_dir="$(mktemp -d "${cache_dir}/build.XXXXXX")"
  trap 'rm -rf "${build_dir}"' EXIT
  mkdir -p "${build_dir}/module-cache"

  air_files=()
  # NAX kernels require Metal 4 and the macOS 26.2 SDK. MLX's CMake build
  # excludes them on today's iOS toolchain as well, so mirror that selection
  # instead of failing the entire mobile build on unsupported M++ symbols.
  while IFS= read -r source; do
    name="$(basename "${source}" .metal)"
    air="${build_dir}/${name}.air"
    xcrun --sdk iphoneos metal \
      -c "${source}" \
      -I "${mlx_source_root}" \
      -Wall \
      -Wextra \
      -fno-fast-math \
      -Wno-c++17-extensions \
      -Wno-c++20-extensions \
      -std=metal3.2 \
      "-fmodules-cache-path=${build_dir}/module-cache" \
      "-mios-version-min=${deployment_target}" \
      -o "${air}"
    air_files+=("${air}")
  done < <(find "${kernels_dir}" -type f -name '*.metal' ! -name '*nax*.metal' -print | LC_ALL=C sort)

  if [[ ${#air_files[@]} -eq 0 ]]; then
    echo "[mlx-metal] no Metal kernels found in ${kernels_dir}" >&2
    exit 1
  fi

  xcrun --sdk iphoneos metallib "${air_files[@]}" -o "${build_dir}/mlx.metallib"
  mv "${build_dir}/mlx.metallib" "${cached_library}"
fi

mkdir -p "${resources_dir}"
cp "${cached_library}" "${resources_dir}/mlx.metallib"
echo "[mlx-metal] installed mlx.metallib in ${resources_dir}"
