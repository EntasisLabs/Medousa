#!/usr/bin/env python3
"""Generate Pydantic models from sdk-contract/medousa-types.schema.json."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCHEMA = ROOT / "sdk-contract" / "medousa-types.schema.json"
OUT_DIR = ROOT / "python" / "medousa-sdk" / "src" / "medousa" / "types" / "_generated"
OUT_FILE = OUT_DIR / "models.py"
UNSIGNED_INTEGER_FORMATS = {"uint", "uint8", "uint16", "uint32", "uint64"}


def _rewrite_openapi_schema(value):
    """Translate schemars-local refs and formats into OpenAPI equivalents."""
    if isinstance(value, list):
        return [_rewrite_openapi_schema(item) for item in value]
    if not isinstance(value, dict):
        return value

    rewritten = {
        key: _rewrite_openapi_schema(item)
        for key, item in value.items()
        if key not in {"$schema", "definitions"}
    }
    ref = rewritten.get("$ref")
    if isinstance(ref, str) and ref.startswith("#/definitions/"):
        rewritten["$ref"] = ref.replace(
            "#/definitions/", "#/components/schemas/", 1
        )
    schema_type = rewritten.get("type")
    is_integer = schema_type == "integer" or (
        isinstance(schema_type, list) and "integer" in schema_type
    )
    if is_integer and rewritten.get("format") in UNSIGNED_INTEGER_FORMATS:
        # OpenAPI does not define Rust's unsigned integer formats. Schemars
        # already emits minimum=0, which preserves the useful constraint.
        rewritten.pop("format")
    return rewritten


def _schema_without_title(value: dict) -> dict:
    normalized = _rewrite_openapi_schema(value)
    normalized.pop("title", None)
    return normalized


def _build_openapi(schemas: dict) -> dict:
    """Flatten independent schemars roots into one resolvable OpenAPI schema."""
    components: dict[str, dict] = {}

    for root_name, root in schemas.items():
        schema = root.get("schema", root)
        for name, definition in schema.get("definitions", {}).items():
            rewritten = _rewrite_openapi_schema(definition)
            existing = components.get(name)
            if existing is not None and existing != rewritten:
                raise ValueError(
                    f"conflicting schema definition {name!r} while processing {root_name!r}"
                )
            components[name] = rewritten

    for name, root in schemas.items():
        schema = root.get("schema", root)
        rewritten = _rewrite_openapi_schema(schema)
        existing = components.get(name)
        if existing is not None and _schema_without_title(existing) != _schema_without_title(
            rewritten
        ):
            raise ValueError(f"root schema {name!r} conflicts with its nested definition")
        components[name] = rewritten

    return {
        "openapi": "3.0.0",
        "info": {"title": "medousa-types", "version": "1.0.0"},
        "paths": {},
        "components": {"schemas": components},
    }


def main() -> int:
    if not SCHEMA.exists():
        print(f"Run: cargo run -p medousa-types-schema\nMissing {SCHEMA}", file=sys.stderr)
        return 1

    schemas: dict = json.loads(SCHEMA.read_text())
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    openapi = _build_openapi(schemas)

    with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as tmp:
        json.dump(openapi, tmp)
        tmp_path = tmp.name

    try:
        subprocess.run(
            [
                sys.executable,
                "-m",
                "datamodel_code_generator",
                "--input",
                tmp_path,
                "--input-file-type",
                "openapi",
                "--output",
                str(OUT_FILE),
                "--output-model-type",
                "pydantic_v2.BaseModel",
                "--use-standard-collections",
                "--use-union-operator",
                "--field-constraints",
                "--use-default",
                "--target-python-version",
                "3.10",
                "--disable-timestamp",
                "--strict-refs",
                "--formatters",
                "builtin",
            ],
            check=True,
        )
        _normalize_generated()
    except (subprocess.CalledProcessError, FileNotFoundError) as error:
        print(f"datamodel-code-generator failed: {error}", file=sys.stderr)
        return 1
    finally:
        Path(tmp_path).unlink(missing_ok=True)

    init = OUT_DIR / "__init__.py"
    init.write_text(
        '"""Auto-generated medousa-types mirrors."""\n'
        "from medousa.types._generated.models import *  # noqa: F403\n"
    )
    print(f"Generated {OUT_FILE} ({len(schemas)} types)")
    return 0


def _normalize_generated() -> None:
    """Rewrite codegen output into a deterministic, ruff-clean module."""
    raw = OUT_FILE.read_text()
    class_lines: list[str] = []
    needs_any = "Any" in raw
    needs_aware = "AwareDatetime" in raw
    needs_enum = "(Enum):" in raw
    needs_root = "RootModel" in raw
    needs_field = "Field(" in raw

    in_class = False
    for line in raw.splitlines():
        if line.startswith("class "):
            in_class = True
            line = line.replace("(BaseModel)", "(MedousaModel)")
        if not in_class:
            continue
        class_lines.append(line)

    if not class_lines:
        raise RuntimeError("datamodel-code-generator produced no model classes")

    pydantic_imports = ["BaseModel", "ConfigDict"]
    if needs_field:
        pydantic_imports.append("Field")
    if needs_aware:
        pydantic_imports.append("AwareDatetime")
    if needs_root:
        pydantic_imports.append("RootModel")
    # Stable alphabetical order for ruff/isort.
    pydantic_imports = sorted(set(pydantic_imports))

    lines = [
        "# DO NOT EDIT — generated by scripts/gen-python-types.py",
        "# Source: sdk-contract/medousa-types.schema.json (cargo run -p medousa-types-schema)",
        "",
        "from __future__ import annotations",
        "",
    ]
    standard_imports = []
    if needs_enum:
        standard_imports.append("from enum import Enum")
    if needs_any:
        standard_imports.append("from typing import Any")
    if standard_imports:
        lines.extend(standard_imports)
        lines.append("")
    lines.append(f"from pydantic import {', '.join(pydantic_imports)}")
    lines.extend(
        [
            "",
            "",
            "class MedousaModel(BaseModel):",
            '    model_config = ConfigDict(extra="ignore", populate_by_name=True)',
            "",
            "",
        ]
    )
    lines.extend(class_lines)
    OUT_FILE.write_text("\n".join(lines).rstrip() + "\n")
if __name__ == "__main__":
    raise SystemExit(main())
