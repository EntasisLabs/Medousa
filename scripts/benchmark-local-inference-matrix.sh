#!/usr/bin/env bash
set -euo pipefail

execute=false
models=()
contexts="1024,2048,4096"
batches="1"
iterations=3
prompt_tokens=64
output_tokens=64
output_dir=""
features=""

usage() {
  sed -n '2,34p' "$0"
  exit "${1:-0}"
}

# Content-free, cold-lifecycle matrix/soak runner for medousa_local_bench.
#
# Usage:
#   scripts/benchmark-local-inference-matrix.sh \
#     --model-id <installed-id> [--model-id <installed-id>] \
#     --output-dir <directory> [options] [--execute]
#
# Options:
#   --contexts <csv>       Context caps (default: 1024,2048,4096)
#   --batches <csv>        Batch caps (default: 1)
#   --iterations <count>   Cold load/stream/unload cycles per cell (default: 3)
#   --prompt-tokens <n>    Synthetic prompt words (default: 64)
#   --output-tokens <n>    Maximum generated tokens (default: 64)
#   --features <cargo>     Cargo feature set; defaults to Metal on macOS and CPU elsewhere
#   --execute              Perform model loads; without this flag, only print the matrix
#   -h, --help             Show this help
#
# Each run writes a create-new JSON manifest. Prompts and generated content are
# never retained. Use separate model IDs to compare UQFF and in-situ artifacts.

while [[ $# -gt 0 ]]; do
  case "$1" in
    --model-id) models+=("${2:?missing model id}"); shift 2 ;;
    --contexts) contexts="${2:?missing contexts}"; shift 2 ;;
    --batches) batches="${2:?missing batches}"; shift 2 ;;
    --iterations) iterations="${2:?missing iterations}"; shift 2 ;;
    --prompt-tokens) prompt_tokens="${2:?missing prompt token count}"; shift 2 ;;
    --output-tokens) output_tokens="${2:?missing output token count}"; shift 2 ;;
    --output-dir) output_dir="${2:?missing output directory}"; shift 2 ;;
    --features) features="${2:?missing Cargo features}"; shift 2 ;;
    --execute) execute=true; shift ;;
    -h|--help) usage 0 ;;
    *) echo "unknown argument: $1" >&2; usage 2 ;;
  esac
done

[[ ${#models[@]} -gt 0 ]] || { echo "at least one --model-id is required" >&2; exit 2; }
[[ -n "$output_dir" ]] || { echo "--output-dir is required" >&2; exit 2; }
[[ "$iterations" =~ ^[1-9][0-9]*$ ]] || { echo "--iterations must be positive" >&2; exit 2; }

if [[ -z "$features" ]]; then
  if [[ "$(uname -s)" == "Darwin" ]]; then
    features="embedded-inference-metal"
  else
    features="embedded-inference"
  fi
fi

IFS=',' read -r -a context_values <<< "$contexts"
IFS=',' read -r -a batch_values <<< "$batches"
total=$((${#models[@]} * ${#context_values[@]} * ${#batch_values[@]} * iterations))
echo "local inference matrix: $total cold lifecycle runs"
echo "features: $features"
echo "output: $output_dir"

if [[ "$execute" != true ]]; then
  echo "dry run only; add --execute to load installed models"
  exit 0
fi

mkdir -p "$output_dir"
failures=0
run=0
for model in "${models[@]}"; do
  safe_model="${model//\//_}"
  safe_model="${safe_model//:/_}"
  for context in "${context_values[@]}"; do
    for batch in "${batch_values[@]}"; do
      for ((iteration = 1; iteration <= iterations; iteration++)); do
        run=$((run + 1))
        manifest="$output_dir/${safe_model}-c${context}-b${batch}-i${iteration}.json"
        echo "[$run/$total] $model context=$context batch=$batch iteration=$iteration"
        if ! cargo run --quiet -p medousa-local-inference \
          --bin medousa_local_bench --features "$features" -- \
          --model-id "$model" --context "$context" --batch "$batch" \
          --prompt-tokens "$prompt_tokens" --output-tokens "$output_tokens" \
          --output "$manifest"; then
          failures=$((failures + 1))
        fi
      done
    done
  done
done

echo "matrix complete: $((total - failures)) passed, $failures failed"
[[ $failures -eq 0 ]]
