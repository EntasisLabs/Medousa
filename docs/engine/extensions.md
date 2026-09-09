# Extensions

**Audience:** integrator

Short reference for secondary engine subsystems. Full routes in [http-api.md](http-api.md).

---

## Grapheme (scripts workbench)

WASM/script modules for automations and custom tools.

| Routes | `/v1/grapheme/*` |
| Env | `GRAPHEME_*`, `MEDOUSA_GRAPHEME_*` in [configuration-reference.md](../configuration-reference.md) |
| Plan | [scripts-workbench-plan.md](../../architecture/scripts-workbench-plan.md) |

Runtime-backed Grapheme calls, including `cognition_shell_run` and
`grapheme.invoke`, submit a durable job to the shared default queue. They wait
up to 60 seconds for that job's terminal attempt receipt. Processing another
queued job, or observing a terminal state before its receipt is stored, does
not count as an execution failure.

Results include `job_id`, `completed`, and `succeeded`. If the receipt is still
pending, `completed` is `false` and `succeeded` is `null`; diagnostics include
`result_path` pointing to `GET /v1/jobs/{job_id}/result`. Inspect the existing
job before resubmitting: ending the wait does not cancel execution. Scheduling
preflight likewise returns `validated: null` while pending and does not approve
the source for scheduling. A successful runtime receipt alone does not establish
shell command success; check the shell result's exit code and diagnostics.

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
