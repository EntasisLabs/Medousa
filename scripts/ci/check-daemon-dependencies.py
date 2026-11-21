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


def feature_map(tree):
    effective = {}
    for line in tree.splitlines():
        match = re.match(r"^(grapheme-[a-z-]+) v\S+ FEATURES=\[([^\]]*)\]", line)
        if match:
            effective[match[1]] = set(filter(None, match[2].split(",")))
    return effective


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    assert "tui" not in manifest["features"]["default"]
    bins = {entry["name"]: entry for entry in manifest["bin"]}
    assert bins["medousa_daemon"]["required-features"] == ["full-daemon"]
    assert bins["medousa"]["required-features"] == ["tui"]
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

    def cargo_tree(extra, package="medousa"):
        command = ["cargo", "tree", "--locked", "-p", package, "-e", "normal,build", "--prefix", "none"]
        if args.offline:
            command.append("--offline")
        return subprocess.check_output(command + extra, cwd=ROOT, text=True)

    def graph(extra):
        return packages(cargo_tree(extra))

    def grapheme_features(flags):
        tree = cargo_tree(flags + ["--format", "{p} FEATURES=[{f}]"])
        return feature_map(tree)

    slim = grapheme_features(["--no-default-features", "--features", "embedded-daemon"])
    assert slim["grapheme-sdk"] == {"host"}
    assert slim["grapheme-runtime"] == set()
    for label, flags in (
        ("default daemon", []),
        ("explicit daemon", ["--no-default-features", "--features", "full-daemon"]),
        ("release daemon + iroh", ["--no-default-features", "--features", "full-daemon,iroh-transport"]),
    ):
        names = graph(flags)
        reject_terminal(names)
        assert "medousa" in names and "stasis-rs" in names
        assert not {"grapheme-lsp", "grapheme-aot-container", "wasmer-wasix"} & names
        assert grapheme_features(flags) == slim, (label, grapheme_features(flags), slim)
        print(f"{label}: {len(names)} unique package names; no terminal deps; embedded Grapheme match")
    # Module signatures alone are not proof that execution implementations survived.
    # Check the effective feature closure for all currently supported heavy modules.
    capability_flags = ["--no-default-features", "--features", "grapheme-extended"]
    effective = grapheme_features(capability_flags)
    modules = {"data", "pdf", "image", "plot", "media"}
    for package, required in {
        "grapheme-sdk": modules | {"host", "stage-b", "wasix-runtime"},
        "grapheme-stdlib": modules | {"host", "http", "web", "net", "sql", "surreal", "email", "transforms"},
        "grapheme-runtime": modules | {"stage-b", "wasix-runtime"},
        "grapheme-compiler": modules | {"stage-b"},
        "grapheme-signatures": modules,
    }.items():
        assert required <= effective.get(package, set()), (package, required, effective.get(package))
    assert grapheme_features(["--no-default-features", "--features", "tui"]) == slim
    print("Extended Grapheme closure: implementations, signatures, AOT, WASIX and LSP retained")
    terminal = graph(["--no-default-features", "--features", "tui"])
    assert {"ratatui", "ratatui-image", "crossterm"} <= terminal
    for client in ("medousa-cli", "medousa-tui"):
        names = packages(cargo_tree(["--all-features"], client))
        forbidden = {"medousa", "medousa-engine", "medousa-runtime", "surrealdb", "stasis-rs", "grapheme-runtime", "grapheme-sdk", "medousa-forge"}
        assert not names & forbidden, (client, names & forbidden)
        assert ("ratatui" in names) == (client == "medousa-tui")
        print(f"{client}: {len(names)} packages, daemon runtime excluded")
    print("terminal positive control: rendering dependencies present")
    print("daemon-dependencies: PASS (host target, standalone package selection)")


if __name__ == "__main__":
    main()
