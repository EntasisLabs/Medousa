/** Forge / Undertakings HTTP client (daemon `/v1/forge` + `/v1/world`). */

import { getDaemonUrl } from "$lib/daemon";

export type ActionAffordance = {
  allowed: boolean;
  reason?: string | null;
};

export type AllowedActions = {
  provision: ActionAffordance;
  start_agent: ActionAffordance;
  open_terminal: ActionAffordance;
  begin_attempt: ActionAffordance;
  seal: ActionAffordance;
  review: ActionAffordance;
  apply: ActionAffordance;
  discard: ActionAffordance;
};

export type HumanPhase =
  | "prepare"
  | "work"
  | "review"
  | "complete"
  | "needs_attention";

export type ForgeWorkItem = {
  id: string;
  title: string;
  brief: string;
  state: string;
  owner: string;
  created_at?: string;
  updated_at?: string;
  environment?: {
    worktree: string;
    baseline_oid: string;
    generation: number;
  } | null;
  attempts?: Array<{
    id: string;
    seq: number;
    state: string;
    executor?: { kind?: string; detail?: Record<string, unknown> } | null;
    evidence_id?: string | null;
    lease?: {
      lease_id: string;
      generation: number;
    } | null;
  }>;
  active_attempt?: string | null;
  review_decisions?: Array<{ id: string; strategy: string }>;
  disposition?: string | null;
  target?: { Git?: { repo_path: string; base_ref: string } };
};

export type ItemProjection = ForgeWorkItem & {
  human_phase: HumanPhase | string;
  allowed_actions: AllowedActions;
};

export type ReviewProjection = {
  work_id: string;
  title: string;
  state: string;
  human_phase: string;
  allowed_actions: AllowedActions;
  baseline_oid?: string | null;
  sealed_head_oid?: string | null;
  evidence_id?: string | null;
  evidence_digest?: string | null;
  attempt_id?: string | null;
  attempt_seq?: number | null;
  changed_files: Array<{
    path: string;
    status: string;
    old_path?: string | null;
    is_binary: boolean;
    byte_size?: number | null;
  }>;
  truncated: boolean;
  base_advanced: boolean;
  policy?: PolicyReport | null;
  command_log_lines: number;
  patch_byte_size: number;
  decision?: { id?: string; strategy?: string } | null;
  disposition?: string | null;
  worktree?: string | null;
  active_lease_id?: string | null;
  active_lease_generation?: number | null;
  world?: WorldBindingStatus | null;
};

export type PolicyReport = {
  violations: Array<{ id: string; path: string; rule: string; detail: string }>;
  capture_risks: Array<
    | { kind: "oversize_file"; path: string; bytes: number; limit: number }
    | { kind: "oversize_total"; bytes: number; limit: number }
    | { kind: "secret_pattern"; path: string; pattern: string }
  >;
  symlinks: string[];
  submodules: string[];
  nested_repos: string[];
};

export type WorldBindingStatus = {
  work_id: string;
  baseline?: SnapshotSlot | null;
  sealed?: SnapshotSlot | null;
  diagnostics?: string[];
  capabilities?: Record<string, unknown>;
  last_index?: unknown;
};

export type SnapshotSlot = {
  state: string;
  world?: string | null;
  version?: string | null;
  error?: string | null;
};

export type WorldFile = {
  id: string;
  label: string;
  kind: string;
  path: string;
  language?: string | null;
};

export type WorldEntity = WorldFile & {
  line_start?: number | null;
  line_end?: number | null;
};

export type WorldFilesResult = { ok: boolean; snapshot?: unknown; files: WorldFile[] };
export type WorldFindResult = { ok: boolean; snapshot?: unknown; entities: WorldEntity[] };
export type WorldImpactResult = {
  ok: boolean;
  target?: WorldEntity | null;
  direct_dependents?: number;
  transitive_dependents?: number;
  nodes: Array<WorldEntity & { depth?: number }>;
  message?: string | null;
};
export type WorldAvecResult = {
  ok: boolean;
  code_avec?: {
    scoreable_entities: number;
    fully_scored_entities: number;
    gaps: Array<{ id?: string; label?: string; path?: string }>;
  };
};

export type WorldSnapshotRef = Pick<SnapshotSlot, "world" | "version">;

export type EvidencePage = {
  evidence_id: string;
  offset: number;
  limit: number;
  total_lines: number;
  truncated: boolean;
  lines: string[];
};

export type BeginAttemptResponse = {
  item: ItemProjection;
  lease: { lease_id: string; generation: number };
};

async function forgeUrl(path: string): Promise<string> {
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  return `${base}${path.startsWith("/") ? path : `/${path}`}`;
}

export async function forgeStreamUrl(): Promise<string> {
  return forgeUrl("/v1/forge/stream");
}

async function forgeFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const url = await forgeUrl(path);
  const res = await fetch(url, {
    ...init,
    headers: {
      Accept: "application/json",
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
  });
  if (!res.ok) {
    let detail = res.statusText;
    try {
      const body = (await res.json()) as { error?: string };
      if (body.error) detail = body.error;
    } catch {
      /* ignore */
    }
    const err = new Error(detail) as Error & { status?: number };
    err.status = res.status;
    throw err;
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

export async function listUndertakings(): Promise<ItemProjection[]> {
  return forgeFetch("/v1/forge/items");
}

export async function getUndertaking(workId: string): Promise<ItemProjection> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}`);
}

export async function createUndertaking(input: {
  title: string;
  brief: string;
  repo_path: string;
  base_ref?: string;
}): Promise<ItemProjection> {
  return forgeFetch("/v1/forge/items", {
    method: "POST",
    body: JSON.stringify({
      title: input.title,
      brief: input.brief,
      repo_path: input.repo_path,
      base_ref: input.base_ref ?? "main",
    }),
  });
}

export async function provisionUndertaking(workId: string): Promise<ItemProjection> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/provision`, {
    method: "POST",
    body: "{}",
  });
}

export async function beginHumanAttempt(workId: string): Promise<BeginAttemptResponse> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/attempts`, {
    method: "POST",
    body: JSON.stringify({ executor: { kind: "human", detail: {} } }),
  });
}

export async function sealLease(
  leaseId: string,
  generation: number,
): Promise<ItemProjection> {
  return forgeFetch(`/v1/forge/leases/${encodeURIComponent(leaseId)}/complete`, {
    method: "POST",
    body: JSON.stringify({ generation }),
  });
}

export async function heartbeatLease(leaseId: string, generation: number): Promise<void> {
  return forgeFetch(`/v1/forge/leases/${encodeURIComponent(leaseId)}/heartbeat`, {
    method: "POST",
    body: JSON.stringify({ generation }),
  });
}

export async function getReview(workId: string): Promise<ReviewProjection> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/review`);
}

export async function getEvidencePatch(
  evidenceId: string,
  opts?: { work_id?: string; offset?: number; limit?: number },
): Promise<EvidencePage> {
  const q = new URLSearchParams();
  if (opts?.work_id) q.set("work_id", opts.work_id);
  if (opts?.offset != null) q.set("offset", String(opts.offset));
  if (opts?.limit != null) q.set("limit", String(opts.limit));
  const qs = q.toString();
  return forgeFetch(
    `/v1/forge/evidence/${encodeURIComponent(evidenceId)}/patch${qs ? `?${qs}` : ""}`,
  );
}

export async function getEvidenceCommands(
  evidenceId: string,
  opts?: { work_id?: string; offset?: number; limit?: number },
): Promise<EvidencePage> {
  const q = new URLSearchParams();
  if (opts?.work_id) q.set("work_id", opts.work_id);
  if (opts?.offset != null) q.set("offset", String(opts.offset));
  if (opts?.limit != null) q.set("limit", String(opts.limit));
  const qs = q.toString();
  return forgeFetch(
    `/v1/forge/evidence/${encodeURIComponent(evidenceId)}/commands${qs ? `?${qs}` : ""}`,
  );
}

export async function recordReviewIntent(
  workId: string,
  intent: {
    evidence_id: string;
    evidence_digest: string;
    strategy?: string;
    rationale?: string;
    acknowledged_violations?: string[];
  },
): Promise<ItemProjection> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/decisions`, {
    method: "POST",
    body: JSON.stringify({
      evidence_id: intent.evidence_id,
      evidence_digest: intent.evidence_digest,
      strategy: intent.strategy ?? "preserve_branch",
      rationale: intent.rationale ?? null,
      acknowledged_violations: intent.acknowledged_violations ?? [],
    }),
  });
}

export async function applyDecision(
  workId: string,
  decisionId: string,
): Promise<ItemProjection> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/apply`, {
    method: "POST",
    body: JSON.stringify({ decision_id: decisionId }),
  });
}

export async function discardUndertaking(workId: string): Promise<ItemProjection> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/discard`, {
    method: "POST",
    body: "{}",
  });
}

export async function getWorldBinding(workId: string): Promise<WorldBindingStatus> {
  return forgeFetch(`/v1/world/bindings/${encodeURIComponent(workId)}`);
}

function worldQuery(workId: string, snapshot?: WorldSnapshotRef | null): URLSearchParams {
  if (snapshot?.world && snapshot.version) {
    return new URLSearchParams({ world: snapshot.world, version: snapshot.version });
  }
  return new URLSearchParams({ work_id: workId });
}

export async function getWorldCodeAvec(
  workId: string,
  snapshot?: WorldSnapshotRef | null,
): Promise<WorldAvecResult> {
  return forgeFetch(`/v1/world/code_avec?${worldQuery(workId, snapshot)}`);
}

export async function getWorldFiles(
  workId: string,
  path?: string,
  snapshot?: WorldSnapshotRef | null,
): Promise<WorldFilesResult> {
  const q = worldQuery(workId, snapshot);
  if (path) q.set("path", path);
  return forgeFetch(`/v1/world/files?${q}`);
}

export async function getWorldFind(
  workId: string,
  opts?: {
    kind?: string;
    name_contains?: string;
    path?: string;
    snapshot?: WorldSnapshotRef | null;
  },
): Promise<WorldFindResult> {
  const q = worldQuery(workId, opts?.snapshot);
  if (opts?.kind) q.set("kind", opts.kind);
  if (opts?.name_contains) q.set("name_contains", opts.name_contains);
  if (opts?.path) q.set("path", opts.path);
  return forgeFetch(`/v1/world/find?${q}`);
}

export async function getWorldImpact(
  workId: string,
  entityId: string,
  snapshot?: WorldSnapshotRef | null,
): Promise<WorldImpactResult> {
  const q = worldQuery(workId, snapshot);
  q.set("entity_id", entityId);
  return forgeFetch(`/v1/world/impact?${q}`);
}

export async function queueWorldIndex(
  workId: string,
  kind: "baseline" | "sealed" = "sealed",
): Promise<unknown> {
  return forgeFetch("/v1/world/index", {
    method: "POST",
    body: JSON.stringify({ work_id: workId, kind }),
  });
}

export function humanPhaseLabel(phase: string): string {
  switch (phase) {
    case "prepare":
      return "Prepare";
    case "work":
      return "Working";
    case "review":
      return "Review";
    case "complete":
      return "Complete";
    case "needs_attention":
      return "Needs attention";
    default:
      return phase;
  }
}
