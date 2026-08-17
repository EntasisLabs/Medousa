from __future__ import annotations

from urllib.parse import quote


def encode_path_segment(value: str) -> str:
    return quote(value, safe="")


def encode_note_path(note_path: str) -> str:
    return "/".join(encode_path_segment(segment) for segment in note_path.split("/"))


def expand_path(template: str, params: dict[str, str]) -> str:
    if "?" in template:
        raise ValueError("query text must not be embedded in a path template")
    path = template
    for name, value in params.items():
        splat = "{*" + name + "}"
        needle = "{" + name + "}"
        encoded = encode_path_segment(value)
        if splat in path:
            path = path.replace(splat, encoded)
        elif needle in path:
            path = path.replace(needle, encoded)
        else:
            raise ValueError(f"path template missing parameter {name}")
    if "{" in path:
        raise ValueError("path template has unbound parameters")
    return path
