#!/usr/bin/env python3
"""Generate the TypeScript daemon contract used by Medousa surfaces."""

from __future__ import annotations

import json
import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCHEMA = ROOT / "sdk-contract" / "medousa-types.schema.json"
OUT = ROOT / "apps" / "medousa-home" / "src" / "lib" / "types" / "generated" / "daemon_api.ts"

# Stream + session types TypeScript surfaces rely on for contract parity.
# Nested $ref targets (MediaRef, ContextUsageReport, …) are resolved automatically.
EXPORTED_TYPES = [
    "InteractiveTurnStreamEvent",
    "TurnStreamEnvelopeV2",
    "InteractiveTurnResponse",
    "InteractiveTurnRequest",
    "SetSessionAgentModeRequest",
    "SessionAgentModeResponse",
    "AgentModeListResponse",
    "AgentModeTransitionPolicy",
    "AgentModeProposalListResponse",
    "AgentModeProposalResponse",
    "SessionCodeBindingResponse",
    "SessionTranscriptSearchResponse",
    "DeriveSessionRequest",
    "DeriveSessionResponse",
    "StartSessionCodeProjectRequest",
    "SessionCodeProjectResponse",
    "TurnTicketRecord",
]


def find_schema(name: str, schemas: dict) -> dict | None:
    if name in schemas and isinstance(schemas[name], dict):
        return schemas[name]
    for root in schemas.values():
        if not isinstance(root, dict):
            continue
        defs = root.get("definitions")
        if isinstance(defs, dict) and name in defs and isinstance(defs[name], dict):
            return defs[name]
    return None


def collect_refs(schema: dict, found: set[str]) -> None:
    if not isinstance(schema, dict):
        return
    if "$ref" in schema:
        found.add(schema["$ref"].split("/")[-1])
        return
    for key in ("anyOf", "oneOf", "allOf"):
        for part in schema.get(key) or []:
            collect_refs(part, found)
    if "items" in schema:
        collect_refs(schema["items"], found)
    for prop in (schema.get("properties") or {}).values():
        collect_refs(prop, found)
    if "additionalProperties" in schema and isinstance(schema["additionalProperties"], dict):
        collect_refs(schema["additionalProperties"], found)


def ts_type(schema: dict, defs: dict) -> str:
    if "$ref" in schema:
        ref = schema["$ref"].split("/")[-1]
        return ref

    if "allOf" in schema:
        parts = schema.get("allOf") or []
        if len(parts) == 1:
            return ts_type(parts[0], defs)
        mapped = [ts_type(part, defs) for part in parts]
        return " & ".join(mapped) if mapped else "unknown"

    t = schema.get("type")
    if isinstance(t, list):
        parts = [ts_type({**schema, "type": item}, defs) for item in t if item != "null"]
        if "null" in t:
            if not parts:
                return "null"
            inner = " | ".join(parts)
            return f"{inner} | null"
        return " | ".join(parts) if parts else "unknown"

    if t == "string":
        if schema.get("enum"):
            return " | ".join(json.dumps(value) for value in schema["enum"])
        if schema.get("format") == "date-time":
            return "string"
        return "string"
    if t == "integer" or t == "number":
        return "number"
    if t == "boolean":
        return "boolean"
    if t == "array":
        items = schema.get("items", {})
        if items is True or items == {}:
            return "unknown[]"
        inner = ts_type(items, defs)
        return f"{inner}[]"
    if t == "object":
        props = schema.get("properties") or {}
        if not props:
            return "Record<string, unknown>"
        required = set(schema.get("required", []))
        fields = []
        for prop, prop_schema in props.items():
            optional = "" if prop in required else "?"
            ts_name = prop if re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", prop) else json.dumps(prop)
            fields.append(f"{ts_name}{optional}: {ts_type(prop_schema, defs)}")
        return "{ " + "; ".join(fields) + " }"

    if "anyOf" in schema or "oneOf" in schema:
        parts = schema.get("anyOf") or schema.get("oneOf") or []
        mapped = []
        for part in parts:
            if part.get("type") == "null":
                mapped.append("null")
            else:
                mapped.append(ts_type(part, defs))
        # Deduplicate while preserving order
        seen: set[str] = set()
        uniq = []
        for part in mapped:
            if part not in seen:
                seen.add(part)
                uniq.append(part)
        return " | ".join(uniq) if uniq else "unknown"

    return "unknown"


def emit_interface(name: str, schema_root: dict, defs: dict) -> list[str]:
    schema = schema_root.get("schema", schema_root)
    props = schema.get("properties", {})
    required = set(schema.get("required", []))
    lines = [f"export interface {name} {{"]

    for prop, prop_schema in props.items():
        optional = "" if prop in required else "?"
        ts_name = prop
        if not re.match(r"^[A-Za-z_][A-Za-z0-9_]*$", prop):
            ts_name = json.dumps(prop)
        ty = ts_type(prop_schema, defs)
        lines.append(f"  {ts_name}{optional}: {ty};")

    lines.append("}")
    return lines


def emit_definition(name: str, schema_root: dict, defs: dict) -> list[str]:
    schema = schema_root.get("schema", schema_root)
    if schema.get("enum"):
        variants = " | ".join(json.dumps(value) for value in schema["enum"])
        return [f"export type {name} = {variants};"]
    if schema.get("oneOf") or schema.get("anyOf"):
        variants = schema.get("oneOf") or schema.get("anyOf")
        return [f"export type {name} = " + " | ".join(ts_type(item, defs) for item in variants) + ";"]
    if schema.get("type") != "object":
        return [f"export type {name} = {ts_type(schema, defs)};"]
    return emit_interface(name, schema_root, defs)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output",
        type=Path,
        default=OUT,
        help=f"output TypeScript file (default: {OUT})",
    )
    args = parser.parse_args()

    if not SCHEMA.exists():
        print(f"Missing {SCHEMA}; run: cargo run -p medousa-types-schema", file=sys.stderr)
        return 1

    schemas: dict = json.loads(SCHEMA.read_text())
    defs = schemas

    needed: list[str] = []
    seen: set[str] = set()

    def enqueue(name: str) -> None:
        if name in seen:
            return
        seen.add(name)
        schema = find_schema(name, schemas)
        if schema is None:
            print(f"warn: {name} missing from schema", file=sys.stderr)
            return
        refs: set[str] = set()
        collect_refs(schema, refs)
        for ref in sorted(refs):
            enqueue(ref)
        needed.append(name)

    for name in EXPORTED_TYPES:
        enqueue(name)

    output = args.output if args.output.is_absolute() else ROOT / args.output
    output.parent.mkdir(parents=True, exist_ok=True)
    body: list[str] = [
        "// DO NOT EDIT — generated by scripts/gen-ts-types.py",
        "// Source: sdk-contract/medousa-types.schema.json",
        "",
    ]

    for name in needed:
        schema = find_schema(name, schemas)
        if schema is None:
            continue
        body.extend(emit_definition(name, schema, defs))
        body.append("")

    output.write_text("\n".join(body).rstrip() + "\n")
    print(f"Generated {output} ({len(needed)} types)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
