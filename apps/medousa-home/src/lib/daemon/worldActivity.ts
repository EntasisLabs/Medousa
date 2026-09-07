import type {
  WorldEvidenceRecord,
  WorldEvidenceResponse,
  WorldTimelineCompensation,
  WorldTimelineEvent,
  WorldTimelineResponse,
} from "$lib/types/generated/daemon_api";
import { daemonUnary } from "./contractClient";

export interface WorldActivityItem {
  event: WorldTimelineEvent;
  evidence?: WorldEvidenceRecord;
}

export interface WorldActivityPage {
  items: WorldActivityItem[];
  hasEarlier: boolean;
  evidenceAvailable: boolean;
  evidenceHasEarlier: boolean;
}

export function mergeWorldActivity(
  timeline: WorldTimelineResponse,
  evidence: WorldEvidenceResponse | null,
): WorldActivityPage {
  const evidenceBySequence = new Map(
    (evidence?.evidence ?? []).map((record) => [record.ledger_sequence, record]),
  );
  return {
    items: [...timeline.events]
      .reverse()
      .map((event) => ({ event, evidence: evidenceBySequence.get(event.sequence) })),
    hasEarlier: timeline.has_more,
    evidenceAvailable: evidence !== null,
    evidenceHasEarlier: evidence?.has_more ?? false,
  };
}

export async function loadLatestWorldActivity(
  executionRuntimeId?: string | null,
  requestedLimit = 80,
): Promise<WorldActivityPage> {
  const limit = Math.max(1, Math.min(200, Math.round(requestedLimit)));
  const query = { tail: "true", limit: String(limit) };
  const [timelineResult, evidenceResult] = await Promise.allSettled([
    daemonUnary<WorldTimelineResponse>(
      "worlds.timeline.get",
      {},
      undefined,
      executionRuntimeId,
      query,
    ),
    daemonUnary<WorldEvidenceResponse>(
      "worlds.evidence.get",
      {},
      undefined,
      executionRuntimeId,
      query,
    ),
  ]);
  if (timelineResult.status === "rejected") throw timelineResult.reason;
  return mergeWorldActivity(
    timelineResult.value,
    evidenceResult.status === "fulfilled" ? evidenceResult.value : null,
  );
}

export function worldActorLabel(event: WorldTimelineEvent): string {
  switch (event.principal_kind) {
    case "human":
      return "You";
    case "agent":
      return "Medousa";
    case "bot":
      return "Bot";
    case "worker":
      return "Worker";
    case "peer":
      return "Paired workshop";
    case "system":
      return "System";
    default:
      return "World runtime";
  }
}

export function worldEventLabel(event: WorldTimelineEvent): string {
  switch (event.event_type) {
    case "world_created":
      return "Created world";
    case "capability_granted":
      return "Granted access";
    case "capability_revoked":
      return "Revoked access";
    case "control_acquired":
      return "Took control";
    case "control_released":
      return "Returned control";
    case "action_admitted":
      return "Admitted action";
    case "action_committed":
      return "Completed action";
    case "action_failed":
      return "Action failed";
    case "action_interrupted":
      return "Action interrupted";
    case "external_mutation_observed":
      return "Observed outside change";
    default:
      return event.event_type.replaceAll("_", " ");
  }
}

export type WorldActivityTone = "neutral" | "success" | "warning" | "danger";

export function worldActivityTone(event: WorldTimelineEvent): WorldActivityTone {
  if (event.event_type === "action_failed" || event.event_type === "action_interrupted") {
    return "danger";
  }
  if (event.status === "indeterminate" || event.status === "needs_reconciliation") {
    return "warning";
  }
  if (event.status === "confirmed") return "success";
  return "neutral";
}

export function worldActivityWhen(timestampMs: number, nowMs = Date.now()): string {
  if (!Number.isFinite(timestampMs) || timestampMs <= 0) return "";
  const elapsedMinutes = Math.max(0, Math.floor((nowMs - timestampMs) / 60_000));
  if (elapsedMinutes < 1) return "now";
  if (elapsedMinutes < 60) return `${elapsedMinutes}m`;
  const elapsedHours = Math.floor(elapsedMinutes / 60);
  if (elapsedHours < 24) return `${elapsedHours}h`;
  const elapsedDays = Math.floor(elapsedHours / 24);
  if (elapsedDays < 14) return `${elapsedDays}d`;
  return new Date(timestampMs).toLocaleDateString([], { month: "short", day: "numeric" });
}

export function worldActivityDetailLabel(value?: string | null): string {
  if (!value) return "";
  return value
    .split("_")
    .filter(Boolean)
    .map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(" ");
}

export function worldCompensationLabel(compensation: WorldTimelineCompensation): string {
  switch (compensation.strategy) {
    case "not_applicable":
      return "No compensation needed";
    case "reconcile_then_domain_action":
      return "Reconcile, then admit a new domain action";
    case "operator_directed":
      return "Operator-directed new action";
    case "unavailable":
      return "No compensation available";
    default:
      return worldActivityDetailLabel(compensation.strategy);
  }
}
