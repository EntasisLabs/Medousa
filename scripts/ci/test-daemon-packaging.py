#!/usr/bin/env python3
"""Execute real Docker/release packaging shell invocations with a fake Cargo.

No Rust compilation or Docker execution. The fake enforces required binary features
and deliberately overwrites daemon output during a tui build, proving staging keeps
standalone artifacts. This complements (does not replace) the real Cargo graph guard.
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import textwrap
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
BINS = {entry["name"]: entry.get("required-features", [])
        for entry in tomllib.loads((ROOT / "Cargo.toml").read_text())["bin"]}
FAKE_CARGO = '''#!{python}
import json, os, pathlib, sys
args = sys.argv[1:]
assert args[0] == "build", args
assert "--no-default-features" in args, args
features = set()
for i, arg in enumerate(args):
    if arg == "--features":
        features.update(args[i + 1].split(","))
bins = [args[i + 1] for i, a in enumerate(args) if a == "--bin"]
required = json.loads(os.environ["BINARY_REQUIREMENTS"])
for name in bins:
    assert set(required[name]) <= features, (name, features)
assert not ("medousa_daemon" in bins and "tui" in features), "unified daemon build"
root = pathlib.Path(os.environ["CARGO_TARGET_DIR"])
target = args[args.index("--target") + 1] if "--target" in args else ""
release = root / target / "release"
release.mkdir(parents=True, exist_ok=True)
ext = ".exe" if "windows" in target else ""
for name in bins:
    (release / (name + ext)).write_text("terminal" if "tui" in features else "standalone")
if "tui" in features:
    # Adversarially model a launcher rebuilding its daemon dependency.
    (release / ("medousa_daemon" + ext)).write_text("terminal-rebuilt-daemon")
with open(os.environ["INVOCATIONS"], "a") as log:
    log.write(json.dumps(args) + "\\n")
'''


def docker_commands(docker):
    logical = docker.replace("\\\n", " ")
    return [line[4:] for line in logical.splitlines() if line.startswith("RUN cargo ")]


def workflow_run(workflow, name):
    # Limit extraction to the exact literal run block; unsupported syntax fails closed.
    match = re.search(r"^      - name: " + re.escape(name) + r"\n        run: \|\n((?:          .*\n|\n)+)", workflow, re.M)
    assert match, name
    return textwrap.dedent(match[1])


def execute(commands, target=None):
    with tempfile.TemporaryDirectory(prefix="medousa-packaging-") as temporary:
        root = Path(temporary)
        bindir = root / "bin"
        bindir.mkdir()
        cargo = bindir / "cargo"
        cargo.write_text(FAKE_CARGO.format(python=sys.executable))
        cargo.chmod(0o755)
        env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ["PATH"],
                   CARGO_TARGET_DIR=str(root / "target"),
                   BINARY_REQUIREMENTS=json.dumps(BINS), INVOCATIONS=str(root / "calls.jsonl"))
        for command in commands:
            command = command.replace("/build", str(root))
            if target:
                command = command.replace("${{ matrix.target }}", target)
            subprocess.run(["bash", "-e", "-c", command], cwd=root, env=env,
                           check=True, capture_output=True, text=True)
        return {str(path.relative_to(root)): path.read_text()
                for directory in (root / "packaged", root / "dist", root / "target")
                if directory.exists() for path in directory.rglob("*") if path.is_file()}, [
                    json.loads(line) for line in (root / "calls.jsonl").read_text().splitlines()]


class PackagingInvocations(unittest.TestCase):
    def test_docker_invocations_preserve_all_packaged_binaries(self):
        docker = (ROOT / "Dockerfile").read_text()
        self.assertIn("FROM rust:1.100-bookworm AS builder", docker)
        self.assertIn("ENV CARGO_TARGET_DIR=/build/target", docker)
        commands = docker_commands(docker)
        self.assertEqual(len(commands), 2)
        files, calls = execute(commands)
        self.assertEqual(len(calls), 2)
        self.assertEqual(files["packaged/medousa_daemon"], "standalone")
        self.assertEqual(files["packaged/medousa_cli"], "standalone")
        self.assertEqual(files["packaged/medousa"], "terminal")
        self.assertEqual(files["target/release/medousa_daemon"], "terminal-rebuilt-daemon")
        copies = re.findall(r"^COPY --from=builder (\S+) (\S+)$", docker, re.M)
        self.assertEqual(len(copies), 3)
        for source, destination in copies:
            relative = source.removeprefix("/build/")
            self.assertIn(relative, files)
            self.assertEqual(Path(destination).name, Path(source).name)

    def test_release_invocations_preserve_standalone_for_each_suffix(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        commands = [workflow_run(workflow, name) for name in (
            "Build daemon separately from terminal launcher", "Stage daemon + launcher binaries")]
        for target in ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc", "aarch64-apple-darwin"):
            with self.subTest(target=target):
                files, calls = execute(commands, target)
                ext = ".exe" if "windows" in target else ""
                self.assertEqual(len(calls), 2)
                self.assertEqual(files[f"dist/daemon/medousa_daemon-{target}{ext}"], "standalone")
                self.assertEqual(files[f"dist/daemon/medousa-{target}{ext}"], "terminal")
                self.assertEqual(files[f"target/{target}/release/medousa_daemon{ext}"], "terminal-rebuilt-daemon")

    def test_bash_release_caller_stages_before_terminal_build(self):
        source = (ROOT / "scripts/release/build.sh").read_text()
        section = source[source.index("stage_bin() {"):source.index("build_adapter_manifest() {")]
        # Keep actual release function/build control flow; only tool/path/native
        # validators are fixture bindings. No native binary qualification claimed.
        setup = '''
NEED_ENGINE=1
TARGET=x86_64-unknown-linux-gnu
CARGO_BUILD_ARGS=(--release --no-default-features --features full-daemon --target "$TARGET")
BIN_DIR=/build/dist/bin
mkdir -p "$BIN_DIR"
medousa_log() { :; }
medousa_binary_filename() { echo "$1"; }
medousa_validate_native_binary() { test -f "$1"; }
medousa_find_release_binary() { echo "$CARGO_TARGET_DIR/$2/release/$1"; }
'''
        for prebuilt in (False, True):
            with self.subTest(prebuilt=prebuilt):
                initialize = ""
                if prebuilt:
                    initialize = ('cargo build --no-default-features --features full-daemon '
                                  '--bin medousa_daemon --target "$TARGET"\n'
                                  'MEDOUSA_PREBUILT_DAEMON="$CARGO_TARGET_DIR/$TARGET/release/medousa_daemon"\n')
                files, _ = execute([setup + initialize + section])
                self.assertEqual(files["dist/bin/medousa_daemon"], "standalone")
                self.assertEqual(files["target/x86_64-unknown-linux-gnu/release/medousa_daemon"],
                                 "terminal-rebuilt-daemon")
        late = source[source.index('medousa_log "staging release binaries'):]
        self.assertNotIn("stage_bin medousa_daemon", late)

    def test_platform_lifetime_runner_isolation_and_zero_match_guard(self):
        with tempfile.TemporaryDirectory(prefix="medousa-lifetime-runner-") as temporary:
            root = Path(temporary)
            cargo = root / "cargo"
            calls = root / "calls.jsonl"
            cargo.write_text(f'#!{sys.executable}\n' + '''
import json, os, sys
with open(os.environ["CALLS"], "a") as log:
    log.write(json.dumps(sys.argv[1:]) + "\\n")
print("test result: ok. " + os.environ["PASS_COUNT"] + " passed; 0 failed; 0 ignored; 0 measured; 0 filtered out")
''')
            cargo.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"],
                       CALLS=str(calls), PASS_COUNT="1")
            subprocess.run(["bash", str(ROOT / "scripts/ci/test-platform-lifetime.sh")],
                           cwd=ROOT, env=env, check=True, capture_output=True, text=True)
            invocations = [json.loads(line) for line in calls.read_text().splitlines()]
            self.assertEqual(len(invocations), 3)
            source = (ROOT / "src/runtime/platform/lifetime_tests.rs").read_text()
            for args in invocations:
                self.assertEqual(args[0], "test")
                self.assertIn("--ignored", args)
                self.assertIn("--exact", args)
                self.assertIn("--test-threads=1", args)
                selector = next(arg for arg in args if arg.startswith("runtime::platform::"))
                self.assertIn("async fn " + selector.split("::")[-1] + "()", source)
            env["PASS_COUNT"] = "0"
            refused = subprocess.run(["bash", str(ROOT / "scripts/ci/test-platform-lifetime.sh")],
                                     cwd=ROOT, env=env, capture_output=True, text=True)
            self.assertNotEqual(refused.returncode, 0, "zero matching Rust tests must fail closed")

    def test_powershell_preservation_order_static_only(self):
        # No pwsh on this host: explicitly static, not an executed PS qualification.
        source = (ROOT / "scripts/release/build.ps1").read_text()
        standalone = source.index("Invoke-MedousaCargo @cargoBuildArgs --bin medousa_daemon")
        preserve = source.index("Copy-Item -Force $daemonSrc")
        terminal = source.index("Invoke-MedousaCargo @cargoBuildArgs --features tui")
        self.assertLess(standalone, preserve)
        self.assertLess(preserve, terminal)
        self.assertIn('$bin -eq "medousa_daemon") { continue }', source)

    def test_inherited_missing_tui_invocation_is_rejected(self):
        with self.assertRaises(subprocess.CalledProcessError):
            execute(["cargo build --release -p medousa --no-default-features --features full-daemon "
                     "--bin medousa_daemon --bin medousa --bin medousa_cli"])

    def test_combined_tui_daemon_invocation_is_rejected(self):
        with self.assertRaises(subprocess.CalledProcessError):
            execute(["cargo build --release -p medousa --no-default-features --features tui,full-daemon "
                     "--bin medousa_daemon --bin medousa"])

    def test_late_daemon_copy_would_package_contaminated_binary(self):
        commands = docker_commands((ROOT / "Dockerfile").read_text())
        files, _ = execute(commands + ["cp /build/target/release/medousa_daemon /build/packaged/"])
        self.assertNotEqual(files["packaged/medousa_daemon"], "standalone")

    def test_mobile_documented_launcher_commands_opt_in(self):
        document = (ROOT / "apps/medousa-home/MOBILE-DEV.md").read_text()
        for line in document.splitlines():
            if re.search(r"cargo (?:install|run).*--bin medousa(?: |$)", line):
                self.assertIn("--features tui", line)
        self.assertIn("--features tui", (ROOT / "scripts/mobile-dev-daemon.sh").read_text())


if __name__ == "__main__":
    unittest.main(verbosity=2)
