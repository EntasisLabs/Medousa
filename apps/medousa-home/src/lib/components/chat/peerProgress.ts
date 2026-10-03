import type { PeerProposalReviewRecord } from "$lib/types/generated/daemon_api";

export function peerRuntimeLabel(runtime: string): string {
  return ({ medousa: "Medousa Coder", codex: "Codex", cursor: "Cursor", hermes: "Hermes" } as Record<string, string>)[runtime] ?? runtime;
}

const stateLabels = {
  accepted: "Starting agent",
  running: "Agent is working",
  blocked: "Waiting to continue",
  awaiting_receipt: "Verifying the final result",
  unobserved: "Execution status unavailable",
} as const;
const outcomeLabels = {
  completed: "Work completed",
  failed: "Work failed",
  cancelled: "Work cancelled",
  interrupted: "Work interrupted",
} as const;

export function peerProgressPresentation(row: PeerProposalReviewRecord, now: number) {
  if (row.receipt) return {
    headline: outcomeLabels[row.receipt.outcome],
    activity: null, lastActivity: null, lastActivityStatus: null,
    lastUpdate: null, notice: null,
  };
  const progress = row.progress;
  const lastActivity = progress?.last_activity ?? null;
  const lastActivityStatus = progress?.last_activity_status ?? null;
  const seen = progress?.last_activity_at ? Date.parse(progress.last_activity_at) : NaN;
  const checked = progress ? Date.parse(progress.observed_at) : NaN;
  const age = Math.max(0, Math.floor((now - seen) / 1000));
  const lastUpdate = Number.isFinite(seen)
    ? age < 60 ? "just now" : age < 3600 ? `${Math.floor(age / 60)}m ago` : `${Math.floor(age / 3600)}h ago`
    : null;
  let notice: string | null = null;
  if (row.binding && !progress) notice = "This workshop has not reported live progress yet.";
  else if (Number.isFinite(checked) && now - checked > 45_000) notice = "Progress updates are delayed. Showing the last known activity.";
  else if (progress?.state === "unobserved") notice = "The runtime cannot currently observe this execution. A verified result has not arrived.";
  else if (progress?.state === "running" && Number.isFinite(seen) && age >= 300) notice = "No recent activity update. The assignment has not reported a final outcome.";
  return {
    headline: progress ? stateLabels[progress.state] : "Medousa is tracking this work",
    activity: progress?.current_activity ?? (progress?.state === "running" ? "Working through the next step…" : null),
    lastActivity,
    lastActivityStatus,
    lastUpdate,
    notice,
  };
}
