from __future__ import annotations

from pydantic import ConfigDict, Field
from pydantic.alias_generators import to_camel

from medousa.types.daemon_api import MedousaModel


class CamelMedousaModel(MedousaModel):
    model_config = ConfigDict(
        extra="ignore",
        populate_by_name=True,
        alias_generator=to_camel,
    )


class LocalEngineStatus(CamelMedousaModel):
    feature_enabled: bool
    loaded: bool
    base_url: str
    bind: str | None = None
    model_repo: str | None = None
    model_alias: str | None = None
    inference_backend: str | None = None
    message: str


class LocalHardwareResponse(CamelMedousaModel):
    profile: dict
    engine_available: bool
    compiled_backends: list[str]
    message: str


class LocalCatalogResponse(CamelMedousaModel):
    tier: str
    tier_label: str
    family_default: str
    recommended_model_id: str
    models: list[dict]


class LocalModelsResponse(CamelMedousaModel):
    installed: list[dict] = Field(default_factory=list)
    active_downloads: list[dict] = Field(default_factory=list)


class LocalModelDownloadResponse(CamelMedousaModel):
    job: dict


class ModelDownloadProgress(CamelMedousaModel):
    job_id: str
    model_id: str
    phase: str
    bytes_done: int
    bytes_total: int
    percent: float
    current_file: str | None = None
    message: str
    error: str | None = None
