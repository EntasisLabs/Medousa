#!/usr/bin/env python3
"""Verify current correction sources and immutable old-candidate evidence.

Does not compile/run Rust or promote old results to current qualification.
"""
import hashlib
import json
from pathlib import Path
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    sources = json.loads((HERE / "source-fingerprints.json").read_text())
    for path, expected in sources["files"].items():
        assert digest((ROOT / path).read_bytes()) == expected, f"changed correction source: {path}"
    archive = json.loads((HERE / "archive-fingerprints.json").read_text())
    candidate = archive["candidate"]
    for path, expected in archive["files"].items():
        assert digest((ROOT / path).read_bytes()) == expected, f"changed historical archive: {path}"
        original = subprocess.check_output(["git", "show", f"{candidate}:{path}"], cwd=ROOT)
        assert digest(original) == expected, f"archive identity mismatch: {path}"
    historical = json.loads((HERE.parent / "source-fingerprints.json").read_text())
    for path, expected in historical.items():
        # Historical hashes describe checkout bytes, including .ps1 CRLF. Git
        # blobs are LF; require unchanged attributes and apply their checkout EOL.
        original_attributes = subprocess.check_output(
            ["git", "show", f"{candidate}:.gitattributes"], cwd=ROOT)
        assert (ROOT / ".gitattributes").read_bytes() == original_attributes
        original = subprocess.check_output(
            ["git", "cat-file", "--filters", f"{candidate}:{path}"], cwd=ROOT)
        assert digest(original) == expected, f"old candidate checkout source mismatch: {path}"
    unrun = json.loads((HERE / "unrun-rust-tests.json").read_text())
    assert len(unrun["tests"]) == 3
    assert unrun["status"] == "uncompiled_and_unrun"
    print(f"PASS: {len(sources['files'])} current source fingerprints; "
          f"{len(archive['files'])} immutable old-candidate files; "
          f"{len(historical)} old-source fingerprints verified against {candidate}")
    print("Three new Rust constructor tests remain uncompiled/unrun. No Rust/Docker qualification implied.")


if __name__ == "__main__":
    main()
