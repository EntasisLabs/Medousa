# Extensions

**Audience:** integrator

Short reference for secondary engine subsystems. Full routes in [http-api.md](http-api.md).

---

## Grapheme (scripts workbench)

WASM/script modules for automations and custom tools.

| Routes | `/v1/grapheme/*` |
| Env | `GRAPHEME_*`, `MEDOUSA_GRAPHEME_*` in [configuration-reference.md](../configuration-reference.md) |
| Plan | [scripts-workbench-plan.md](../../architecture/scripts-workbench-plan.md) |

---

## Locus (semantic memory)

| Routes | `/v1/locus/nodes`, `/v1/locus/tags` |
| Env | `LOCUS_*` in configuration reference |
| ADR | [adr-002-user-profiles.md](../architecture/decisions/adr-002-user-profiles.md) |

---

## Workflows & tool history

| Routes | `/v1/workflows/*`, `/v1/tool-history/slices` |
| Use | Replay tool slices, schedule workflows |

---

## Manuscripts (specialties)

| Routes | `/v1/manuscripts`, `/v1/manuscripts/{id}` |
| Cookbook | [skills-and-specialties.md](../cookbook/skills-and-specialties.md) |

---

## Media & STT

| Routes | `POST /v1/media/upload`, `GET /v1/media/{id}`, `POST /v1/stt/transcribe` |
| Plan | [media-and-attachments-plan.md](../../architecture/media-and-attachments-plan.md) |

---

## Model catalog

| Routes | `/v1/models/catalog`, `/v1/models/capabilities`, `/v1/models/catalog/refresh` |
| Plan | [inference-profiles-and-model-catalog-plan.md](../../architecture/inference-profiles-and-model-catalog-plan.md) |
