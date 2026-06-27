from __future__ import annotations

from urllib.parse import quote


def encode_path_segment(value: str) -> str:
    return quote(value, safe="")


def encode_note_path(note_path: str) -> str:
    return "/".join(encode_path_segment(segment) for segment in note_path.split("/"))
