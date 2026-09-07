import type {
  WorldEvidenceRecord,
  WorldEvidenceResponse,
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
