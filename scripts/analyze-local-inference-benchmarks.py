#!/usr/bin/env python3
"""Fail-closed MIR-0 lifecycle report from content-free benchmark manifests."""

from __future__ import annotations

import argparse
import json
import statistics
import sys
from pathlib import Path
from typing import Any

LIFECYCLE_SOAK_RUNS = 100
MAX_PREDICTION_ERROR_PERCENT = 15.0
MIN_RECLAIM_PERCENT = 95.0
MAX_RSS_TREND_MB_PER_CYCLE = 1.0


def phase_sample(manifest: dict[str, Any], phase: str) -> dict[str, Any] | None:
    return next((sample for sample in manifest.get("samples", []) if sample.get("phase") == phase), None)


def group_key(manifest: dict[str, Any]) -> tuple[Any, ...]:
    recipe = manifest.get("recipe", {})
    engine = manifest.get("engine", {})
    return (
        recipe.get("modelId"),
        recipe.get("artifactMode"),
        engine.get("artifactDigest"),
        engine.get("binaryDigest"),
        engine.get("recipeRevision"),
        recipe.get("maxSeqLen"),
        recipe.get("maxBatchSize"),
    )


def linear_slope(values: list[float]) -> float | None:
    if len(values) < 2:
        return None
    center = (len(values) - 1) / 2
    denominator = sum((index - center) ** 2 for index in range(len(values)))
    if denominator == 0:
        return 0.0
    mean = statistics.fmean(values)
    return sum((index - center) * (value - mean) for index, value in enumerate(values)) / denominator


def analyze_group(manifests: list[dict[str, Any]]) -> dict[str, Any]:
    ordered = sorted(manifests, key=lambda manifest: manifest.get("startedAt", ""))
    completed = [
        manifest
        for manifest in ordered
        if manifest.get("result", {}).get("outcome") == "completed"
        and not manifest.get("result", {}).get("error")
    ]
    observed_peaks: list[float] = []
    prediction_errors: list[float] = []
    reclaim_percentages: list[float] = []
    swap_growth: list[float] = []
    settled_rss: list[float] = []
    identities_complete = True

    for manifest in completed:
        engine = manifest.get("engine", {})
        identities_complete &= (
            all(
                isinstance(engine.get(field), str) and engine[field].startswith("sha256:")
                for field in ("artifactDigest", "binaryDigest")
            )
            and isinstance(engine.get("recipeRevision"), str)
            and engine["recipeRevision"].startswith("mir-recipe-v1:")
        )
        before = phase_sample(manifest, "beforeLoad")
        loaded = phase_sample(manifest, "afterLoad")
        streamed = phase_sample(manifest, "afterStream")
        settled = phase_sample(manifest, "reclaimed10s")
        if before and loaded and streamed:
            baseline = float(before.get("processRssMb", 0))
            peak = max(float(loaded.get("processRssMb", 0)), float(streamed.get("processRssMb", 0))) - baseline
            if peak > 0:
                observed_peaks.append(peak)
                predicted = manifest.get("admission", {}).get("estimatedPeakMb")
                if isinstance(predicted, (int, float)):
                    prediction_errors.append(abs(float(predicted) - peak) / peak * 100)
                reclaimed = manifest.get("result", {}).get("rssReclaimedMb10s")
                if isinstance(reclaimed, (int, float)):
                    reclaim_percentages.append(float(reclaimed) / peak * 100)
        if before:
            initial_swap = float(before.get("hostUsedSwapMb", 0))
            swap_growth.append(
                max(
                    (float(sample.get("hostUsedSwapMb", initial_swap)) - initial_swap for sample in manifest.get("samples", [])),
                    default=0.0,
                )
            )
        if settled:
            settled_rss.append(float(settled.get("processRssMb", 0)))

    trend = linear_slope(settled_rss)
    gates: dict[str, bool | None] = {
        "identity": identities_complete and len(completed) == len(ordered),
        "completed": len(completed) == len(ordered),
        "prediction": max(prediction_errors, default=float("inf")) <= MAX_PREDICTION_ERROR_PERCENT
        if prediction_errors
        else None,
        "swap": max(swap_growth, default=float("inf")) <= 0 if swap_growth else None,
        "reclaim": min(reclaim_percentages, default=float("-inf")) >= MIN_RECLAIM_PERCENT
        if reclaim_percentages
        else None,
        "soak": len(completed) >= LIFECYCLE_SOAK_RUNS,
        "trend": trend <= MAX_RSS_TREND_MB_PER_CYCLE if trend is not None else None,
    }
    status = "pass" if all(value is True for value in gates.values()) else "fail" if any(value is False for value in gates.values()) else "unknown"
    return {
        "key": group_key(ordered[0]),
        "runs": len(ordered),
        "completedRuns": len(completed),
        "status": status,
        "gates": gates,
        "maxObservedPeakMb": max(observed_peaks, default=None),
        "maxPredictionErrorPercent": max(prediction_errors, default=None),
        "minReclaimPercent": min(reclaim_percentages, default=None),
        "maxSwapGrowthMb": max(swap_growth, default=None),
        "settledRssTrendMbPerCycle": trend,
    }


def render(groups: list[dict[str, Any]]) -> str:
    lines = [
        "# Local inference MIR-0 evidence report",
        "",
        "| Model | Artifact | Context | Batch | Runs | Result | Peak MiB | Prediction error | Reclaimed | Swap growth | RSS trend |",
        "|---|---:|---:|---:|---:|---|---:|---:|---:|---:|---:|",
    ]
    for group in groups:
        model, artifact, _, _, _, context, batch = group["key"]
        value = lambda key, suffix="": "unknown" if group[key] is None else f"{group[key]:.1f}{suffix}"
        lines.append(
            f"| {model} | {artifact} | {context} | {batch} | {group['completedRuns']}/{group['runs']} | "
            f"{group['status'].upper()} | {value('maxObservedPeakMb')} | {value('maxPredictionErrorPercent', '%')} | "
            f"{value('minReclaimPercent', '%')} | {value('maxSwapGrowthMb')} | {value('settledRssTrendMbPerCycle')} |"
        )
    lines.extend(["", "## Ranked release findings", ""])
    findings: list[tuple[int, str]] = []
    for group in groups:
        model, artifact, _, _, _, context, batch = group["key"]
        label = f"{model}/{artifact} context={context} batch={batch}"
        for gate, passed in group["gates"].items():
            if passed is False:
                priority = {"completed": 100, "identity": 95, "swap": 90, "reclaim": 80, "prediction": 70, "trend": 60, "soak": 50}[gate]
                findings.append((priority, f"{label}: `{gate}` gate failed"))
            elif passed is None:
                findings.append((40, f"{label}: `{gate}` evidence is unavailable"))
    if findings:
        lines.extend(f"{index}. {finding}" for index, (_, finding) in enumerate(sorted(findings, reverse=True), 1))
    else:
        lines.append("All measured groups passed every MIR-0 safety gate.")
    lines.extend([
        "",
        "A group passes only with verified identities, completed runs, ≤15% peak error, no new swap, ≥95% reclaim, 100 cycles, and ≤1 MiB/cycle settled-RSS slope.",
    ])
    return "\n".join(lines) + "\n"


def load_manifests(directory: Path) -> list[dict[str, Any]]:
    manifests = []
    for path in sorted(directory.rglob("*.json")):
        with path.open(encoding="utf-8") as handle:
            manifest = json.load(handle)
        if not isinstance(manifest, dict) or "recipe" not in manifest or "result" not in manifest:
            raise ValueError(f"{path} is not a local benchmark manifest")
        manifests.append(manifest)
    if not manifests:
        raise ValueError(f"no JSON benchmark manifests found in {directory}")
    return manifests


def self_test() -> None:
    manifests = []
    for iteration in range(100):
        manifests.append({
            "startedAt": f"2026-01-01T00:00:{iteration:02d}Z",
            "engine": {"artifactDigest": "sha256:a", "binaryDigest": "sha256:b", "recipeRevision": "mir-recipe-v1:c"},
            "recipe": {"modelId": "test", "artifactMode": "prequantizedUqff", "maxSeqLen": 4096, "maxBatchSize": 1},
            "admission": {"estimatedPeakMb": 1000},
            "samples": [
                {"phase": "beforeLoad", "processRssMb": 100, "hostUsedSwapMb": 0},
                {"phase": "afterLoad", "processRssMb": 1100, "hostUsedSwapMb": 0},
                {"phase": "afterStream", "processRssMb": 1090, "hostUsedSwapMb": 0},
                {"phase": "reclaimed10s", "processRssMb": 100, "hostUsedSwapMb": 0},
            ],
            "result": {"outcome": "completed", "error": None, "rssReclaimedMb10s": 1000},
        })
    report = analyze_group(manifests)
    assert report["status"] == "pass", report
    print("self-test passed")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", nargs="?", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return 0
    if args.directory is None:
        parser.error("directory is required unless --self-test is used")
    manifests = load_manifests(args.directory)
    grouped: dict[tuple[Any, ...], list[dict[str, Any]]] = {}
    for manifest in manifests:
        grouped.setdefault(group_key(manifest), []).append(manifest)
    report = render([analyze_group(group) for group in grouped.values()])
    if args.output:
        with args.output.open("x", encoding="utf-8") as handle:
            handle.write(report)
    else:
        sys.stdout.write(report)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"error: {error}", file=sys.stderr)
        raise SystemExit(2)
