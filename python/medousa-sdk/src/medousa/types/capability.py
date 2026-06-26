from __future__ import annotations

from medousa.types.daemon_api import MedousaModel


class CapabilityBindingSummary(MedousaModel):
    source: str
    reference: str
    available: bool
    effect_class: str | None = None
    invoke_via: str | None = None


class CapabilityListEntry(MedousaModel):
    id: str
    title: str
    binding_count: int
    description: str | None = None
    domain: str
    has_grapheme: bool
    has_mcp: bool
    bindings_summary: list[CapabilityBindingSummary] = []


class CapabilityListResponse(MedousaModel):
    capabilities: list[CapabilityListEntry]


class CapabilityResolveResponse(MedousaModel):
    capability: str
    title: str
    description: str | None = None
    implementations: dict = {}
    recommended: dict | None = None
    gateway_unreachable: bool | None = None
