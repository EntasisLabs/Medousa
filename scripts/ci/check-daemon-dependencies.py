#!/usr/bin/env python3
"""Guard actual daemon profiles, not a workspace-unified metadata graph.

Run from any directory. --offline uses only the local Cargo cache. This inspects
normal/build dependencies; dev-dependencies intentionally do not define a shipped
binary. CI separately checks both daemon and terminal targets.
"""
import argparse
import pathlib
import re
import subprocess
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
FORBIDDEN = re.compile(r"^(ratatui(?:[-_].*)?|crossterm(?:[-_].*)?)$")


def packages(tree):
    return {line.split()[0] for line in tree.splitlines()
            if re.match(r"^[A-Za-z0-9_-]+ v[0-9]", line)}


def reject_terminal(names):
    forbidden = sorted(name for name in names if FORBIDDEN.fullmatch(name))
    if forbidden:
        raise AssertionError(f"daemon pulls terminal-only dependencies: {forbidden}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    assert "tui" not in manifest["features"]["default"]
    bins = {entry["name"]: entry for entry in manifest["bin"]}
    assert bins["medousa_daemon"]["required-features"] == ["full-daemon"]
    for name in ("medousa", "medousa_tui"):
        assert bins[name]["required-features"] == ["tui"]
    # A shared consumer must never import terminal-owned contracts/assembly again.
    for path in (ROOT / "src").rglob("*.rs"):
        relative = path.relative_to(ROOT / "src")
        if relative.parts[0] in {"bin", "tui"} or relative.as_posix() == "runtime/tui_platform.rs":
            continue
        assert "crate::tui::" not in path.read_text(), f"reverse runtime ownership: {relative}"
    # Ensure the deny rule itself cannot accidentally accept the known baseline.
    assert packages("ratatui v0.30.0\ncrossterm v0.28.1\nimage v0.25.9 (*)") == {"ratatui", "crossterm", "image"}
    for name in ("ratatui", "ratatui-core", "ratatui-image", "crossterm", "crossterm_winapi"):
        try:
            reject_terminal({name})
        except AssertionError:
            pass
        else:
            raise AssertionError(f"deny rule accepted {name}")

    def cargo_tree(extra):
        command = ["cargo", "tree", "--locked", "-p", "medousa", "-e", "normal,build", "--prefix", "none"]
        if args.offline:
            command.append("--offline")
        return subprocess.check_output(command + extra, cwd=ROOT, text=True)

    def graph(extra):
        return packages(cargo_tree(extra))

    for label, flags in (
        ("default daemon", []),
        ("explicit daemon", ["--no-default-features", "--features", "full-daemon"]),
        ("release daemon + iroh", ["--no-default-features", "--features", "full-daemon,iroh-transport"]),
    ):
        names = graph(flags)
        # Retained shared capability: Grapheme data -> Polars fmt -> comfy-table tty.
        # Only this exact immediate parent may bring Crossterm into a daemon build.
        # Adding a direct dependency (even the same version) must fail the guard.
        inverse = cargo_tree(flags + ["--invert", "crossterm", "--depth", "1"])
        assert packages(inverse) == {"crossterm", "comfy-table"}, inverse
        assert "crossterm v0.29.0" in inverse, inverse
        assert "crossterm v0.28." not in cargo_tree(flags)
        reject_terminal(names - {"crossterm"})
        assert "medousa" in names and "stasis-rs" in names
        print(f"{label}: {len(names)} unique package names, no TUI-only deps; shared Crossterm only via comfy-table")
    # Module signatures alone are not proof that execution implementations survived.
    # Check the effective feature closure for all currently supported heavy modules.
    capability_flags = ["--no-default-features", "--features", "full-daemon"]
    feature_tree = cargo_tree(capability_flags + ["--format", "{p} FEATURES=[{f}]"])
    effective = {}
    for line in feature_tree.splitlines():
        match = re.match(r"^(grapheme-[a-z-]+) v\S+ FEATURES=\[([^\]]*)\]", line)
        if match:
            effective[match[1]] = set(match[2].split(","))
    modules = {"data", "pdf", "image", "plot", "media"}
    for package, required in {
        "grapheme-sdk": modules | {"host", "stage-b", "wasix-runtime"},
        "grapheme-stdlib": modules | {"host", "http", "web", "net", "sql", "surreal", "email", "transforms"},
        "grapheme-runtime": modules | {"stage-b", "wasix-runtime"},
        "grapheme-compiler": modules | {"stage-b"},
        "grapheme-signatures": modules,
    }.items():
        assert required <= effective.get(package, set()), (package, required, effective.get(package))
    print("Grapheme capability closure: implementations, signatures, AOT and WASIX retained")
    terminal = graph(["--no-default-features", "--features", "tui"])
    assert {"ratatui", "ratatui-image", "crossterm"} <= terminal
    print("terminal positive control: rendering dependencies present")
    print("daemon-dependencies: PASS (host target, standalone package selection)")


if __name__ == "__main__":
    main()
