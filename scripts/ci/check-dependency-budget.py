#!/usr/bin/env python3
"""P09: unique Cargo name/version pairs and duplicate-version names."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUDGET_PATH = ROOT / "scripts" / "ci" / "dependency-budget.json"
BANNED_ROOT = ("teloxide", "serenity", "slack-morphism")
TREE_LINE = re.compile(r"^(\S+)\s+v(\S+)")


def parse_tree(text: str) -> tuple[set[tuple[str, str]], dict[str, set[str]]]:
    pairs: set[tuple[str, str]] = set()
    versions: dict[str, set[str]] = defaultdict(set)
    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            continue
        match = TREE_LINE.match(line)
        if not match:
            continue
        name, version = match.group(1), match.group(2)
        version = version.split(" ", 1)[0]
        pairs.add((name, version))
        versions[name].add(version)
    return pairs, versions


def cargo_tree(args: list[str]) -> str:
    result = subprocess.run(
        ["cargo", "tree", *args, "-e", "normal", "--prefix", "none", "--no-dedupe"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return result.stdout


def profile_tree_args(profile: dict) -> list[str]:
    kind = profile.get("kind", "package")
    if kind == "package":
        args = ["-p", profile["package"]]
        features = profile.get("features") or []
        if features:
            args.extend(["--features", ",".join(features)])
        return args
    if kind == "packages":
        args: list[str] = []
        for package in profile["packages"]:
            args.extend(["-p", package])
        return args
    if kind == "manifest":
        return ["--manifest-path", profile["manifestPath"]]
    raise SystemExit(f"unknown profile kind: {kind}")


def banned_root_hits() -> list[str]:
    manifest = (ROOT / "Cargo.toml").read_text()
    in_deps = False
    hits = []
    for line in manifest.splitlines():
        stripped = line.strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            in_deps = stripped in {"[dependencies]", "[dev-dependencies]"}
            continue
        if not in_deps or stripped.startswith("#") or not stripped:
            continue
        name = stripped.split("=", 1)[0].strip().strip('"')
        if name in BANNED_ROOT:
            hits.append(name)
    return hits


def measure_profile(profile: dict) -> dict:
    pairs, versions = parse_tree(cargo_tree(profile_tree_args(profile)))
    duplicates = sorted(name for name, vers in versions.items() if len(vers) > 1)
    names = {name for name, _ in pairs}
    return {
        "uniqueNameVersionPairs": len(pairs),
        "duplicateVersionNames": len(duplicates),
        "duplicateNames": duplicates,
        "bannedInTree": sorted(name for name in BANNED_ROOT if name in names),
    }


def load_budget() -> dict:
    return json.loads(BUDGET_PATH.read_text())


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()

    banned = banned_root_hits()
    if banned:
        print(
            "dependency-budget: banned root dependencies still listed: "
            + ", ".join(banned),
            file=sys.stderr,
        )
        return 1

    budget = load_budget()
    failed = False
    default_measured: dict | None = None

    for name, profile in budget["profiles"].items():
        measured = measure_profile(profile)
        if name == "medousa-default":
            default_measured = measured
        print(
            f"dependency-budget[{name}]: unique={measured['uniqueNameVersionPairs']} "
            f"ceiling={profile['uniqueNameVersionPairs']}"
        )
        print(
            f"dependency-budget[{name}]: duplicate-names="
            f"{measured['duplicateVersionNames']} "
            f"ceiling={profile['duplicateVersionNames']}"
        )
        if measured["bannedInTree"] and profile.get("forbidAdapterFrameworks", name == "medousa-default"):
            print(
                "dependency-budget: adapter frameworks in compile graph: "
                + ", ".join(measured["bannedInTree"]),
                file=sys.stderr,
            )
            failed = True
        if args.write:
            profile["uniqueNameVersionPairs"] = measured["uniqueNameVersionPairs"]
            profile["duplicateVersionNames"] = measured["duplicateVersionNames"]
            continue
        if measured["uniqueNameVersionPairs"] > int(profile["uniqueNameVersionPairs"]):
            print(
                f"dependency-budget[{name}]: unique name/version pairs grew; "
                "update scripts/ci/dependency-budget.json with justification",
                file=sys.stderr,
            )
            failed = True
        if measured["duplicateVersionNames"] > int(profile["duplicateVersionNames"]):
            print(
                f"dependency-budget[{name}]: duplicate-version names grew; "
                "update the duplicate ledger + budget file with owner/expiry",
                file=sys.stderr,
            )
            failed = True

    if args.write:
        if default_measured is not None:
            budget["duplicateNames"] = default_measured["duplicateNames"]
        BUDGET_PATH.write_text(json.dumps(budget, indent=2) + "\n")
        print("dependency-budget: wrote profile ceilings")
        return 0

    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
