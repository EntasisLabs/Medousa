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
  synthesis: {
    outcome: string;
    status: "ready" | "review" | "needs_attention" | string;
    status_summary: string;
    risk: "low" | "attention" | "high" | string;
    risk_summary: string;
    verification?: {
      label: string;
      command: string[];
      success: boolean;
      exit_code?: number | null;
      duration_ms?: number | null;
    } | null;
    unresolved_issues: string[];
    recommended_next_action: string;
  };
  attribution: Array<{
    id: string;
    kind: "human" | "agent" | "terminal" | "verification" | string;
    label: string;
    state: string;
    started_at: string;
    ended_at?: string | null;
    files: string[];
  }>;
  timeline: Array<{
    id: string;
    at: string;
    kind: string;
    label: string;
    detail?: string | null;
    actor_kind: string;
    actor_label: string;
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

export type ReviewFileDiff = {
  work_id: string;
  path: string;
  status: string;
  old_path?: string | null;
  baseline_oid: string;
  reviewed_oid: string;
  binary: boolean;
  baseline: ReviewFileVersion;
  reviewed: ReviewFileVersion;
  hunks: ReviewDiffHunk[];
  changed_lines: Array<{ line: number; kind: "added" | "deleted" | string }>;
  truncated: boolean;
};

export type ReviewFileVersion = {
  exists: boolean;
  binary: boolean;
  byte_size: number;
  digest?: string | null;
  content?: string | null;
};

export type ReviewDiffHunk = {
  old_start: number;
  old_count: number;
  new_start: number;
  new_count: number;
  lines: Array<{
    kind: "context" | "addition" | "deletion" | string;
    old_line?: number | null;
    new_line?: number | null;
    content: string;
  }>;
};

export type RestoreReviewFileResponse = {
  item: ItemProjection;
  lease: {
    lease_id: string;
    generation: number;
  };
  path: string;
  action: string;
  preserved_revision: string;
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

export type ForgeSourceFile = {
  work_id: string;
  path: string;
  content: string;
  digest: string;
  byte_size: number;
};

export type ForgeSourceTree = {
  work_id: string;
  files: Array<{ path: string; byte_size: number; status?: string | null }>;
  truncated: boolean;
};

export type ForgeSourceTreeFile = ForgeSourceTree["files"][number];

export type ForgeSourceSearch = {
  work_id: string;
  hits: Array<{ path: string; line: number; preview: string }>;
  truncated: boolean;
};

export type ForgeCodeWorkspaceState = {
  tabs: Array<{
    path: string;
    draft?: string | null;
    source_digest: string;
    line?: number | null;
  }>;
  active_path?: string | null;
  secondary_path?: string | null;
  updated_at?: string | null;
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

export async function getUndertakingSource(
  workId: string,
  path: string,
): Promise<ForgeSourceFile> {
  const query = new URLSearchParams({ path });
  return forgeFetch(
    `/v1/forge/items/${encodeURIComponent(workId)}/source?${query}`,
  );
}

export async function getUndertakingSourceTree(
  workId: string,
): Promise<ForgeSourceTree> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/tree`);
}

export async function searchUndertakingSource(
  workId: string,
  query: string,
): Promise<ForgeSourceSearch> {
  const params = new URLSearchParams({ query });
  return forgeFetch(
    `/v1/forge/items/${encodeURIComponent(workId)}/search?${params}`,
  );
}

export async function getCodeWorkspaceState(
  workId: string,
): Promise<ForgeCodeWorkspaceState> {
  return forgeFetch(
    `/v1/forge/items/${encodeURIComponent(workId)}/workspace-state`,
  );
}

export async function saveCodeWorkspaceState(
  workId: string,
  state: ForgeCodeWorkspaceState,
  lease?: { lease_id: string; generation: number } | null,
): Promise<ForgeCodeWorkspaceState> {
  return forgeFetch(
    `/v1/forge/items/${encodeURIComponent(workId)}/workspace-state`,
    {
      method: "PUT",
      body: JSON.stringify({
        ...state,
        lease_id: lease?.lease_id ?? null,
        generation: lease?.generation ?? null,
      }),
    },
  );
}

export async function saveUndertakingSource(
  workId: string,
  input: {
    path: string;
    content: string;
    lease_id: string;
    generation: number;
    expected_digest: string;
  },
): Promise<ForgeSourceFile> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/source`, {
    method: "PUT",
    body: JSON.stringify(input),
  });
}

export async function createUndertakingSource(
  workId: string,
  input: {
    path: string;
    content?: string;
    lease_id: string;
    generation: number;
  },
): Promise<ForgeSourceFile> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/source`, {
    method: "POST",
    body: JSON.stringify({ content: "", ...input }),
  });
}

export async function renameUndertakingSource(
  workId: string,
  input: {
    path: string;
    destination: string;
    lease_id: string;
    generation: number;
    expected_digest: string;
  },
): Promise<ForgeSourceFile> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/source`, {
    method: "PATCH",
    body: JSON.stringify(input),
  });
}

export async function deleteUndertakingSource(
  workId: string,
  input: {
    path: string;
    lease_id: string;
    generation: number;
    expected_digest: string;
  },
): Promise<{ work_id: string; path: string; deleted: boolean }> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/source`, {
    method: "DELETE",
    body: JSON.stringify(input),
  });
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

export type RepositoryInspection = {
  path: string;
  display_name: string;
  current_branch?: string | null;
  suggested_base_ref: string;
  dirty: boolean;
  changed_files: number;
  remotes: string[];
  existing_projects: Array<{
    id: string;
    title: string;
    state: string;
    human_phase: string;
  }>;
  state_explanation: string;
  trust_explanation: string;
};

export type RepositoryCatalogEntry = RepositoryInspection & {
  pinned: boolean;
  last_used_at: string;
  available: boolean;
};

export type RepositoryBrowseEntry = {
  name: string;
  path: string;
  repository: boolean;
};

export type RepositoryBrowseResponse = {
  path: string;
  parent?: string | null;
  repository: boolean;
  places: RepositoryBrowseEntry[];
  entries: RepositoryBrowseEntry[];
  truncated: boolean;
};

export type ProjectTask = {
  id: string;
  label: string;
  kind: "verify" | "test" | "build" | "run" | string;
  argv: string[];
};

export type ProjectTaskResult = {
  task: ProjectTask;
  success: boolean;
  exit_code?: number | null;
  stdout: string;
  stderr: string;
  truncated: boolean;
  duration_ms: number;
};

export async function getProjectTasks(workId: string): Promise<ProjectTask[]> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/tasks`);
}

export async function runProjectTask(
  workId: string,
  taskId: string,
  lease: { lease_id: string; generation: number },
): Promise<ProjectTaskResult> {
  return forgeFetch(
    `/v1/forge/items/${encodeURIComponent(workId)}/tasks/${encodeURIComponent(taskId)}/run`,
    { method: "POST", body: JSON.stringify(lease) },
  );
}

export async function inspectForgeRepository(path: string): Promise<RepositoryInspection> {
  return forgeFetch("/v1/forge/repositories/inspect", {
    method: "POST",
    body: JSON.stringify({ path }),
  });
}

export async function listForgeRepositories(): Promise<RepositoryCatalogEntry[]> {
  return forgeFetch("/v1/forge/repositories");
}

export async function setForgeRepositoryPinned(
  path: string,
  pinned: boolean,
): Promise<RepositoryCatalogEntry[]> {
  return forgeFetch("/v1/forge/repositories", {
    method: "PUT",
    body: JSON.stringify({ path, pinned }),
  });
}

export async function browseForgeRepositories(
  path?: string | null,
): Promise<RepositoryBrowseResponse> {
  const query = path ? `?path=${encodeURIComponent(path)}` : "";
  return forgeFetch(`/v1/forge/repositories/browse${query}`);
}

export async function startUndertaking(input: {
  title: string;
  brief: string;
  repo_path: string;
  base_ref?: string;
}): Promise<ItemProjection> {
  return forgeFetch("/v1/forge/items/start", {
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

export async function prepareExecutorHandoff(input: {
  work_id: string;
  lease_id: string;
  generation: number;
  to_executor: "codex" | "cursor" | "human";
}): Promise<ItemProjection> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(input.work_id)}/handoff`, {
    method: "POST",
    body: JSON.stringify({
      lease_id: input.lease_id,
      generation: input.generation,
      to_executor: input.to_executor,
    }),
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

export async function getReviewFile(workId: string, path: string): Promise<ReviewFileDiff> {
  const query = new URLSearchParams({ path });
  return forgeFetch(
    `/v1/forge/items/${encodeURIComponent(workId)}/review/file?${query.toString()}`,
  );
}

export async function restoreReviewFile(
  workId: string,
  input: { path: string; expected_reviewed_oid: string },
): Promise<RestoreReviewFileResponse> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/review/file`, {
    method: "POST",
    body: JSON.stringify(input),
  });
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

export async function getWorldAtLocation(
  workId: string,
  path: string,
  line: number,
  snapshot?: WorldSnapshotRef | null,
): Promise<{ ok: boolean; entity?: WorldEntity | null }> {
  const q = worldQuery(workId, snapshot);
  q.set("path", path);
  q.set("line", String(Math.max(1, Math.floor(line))));
  return forgeFetch(`/v1/world/at_location?${q}`);
}

export async function exportUndertakingBundle(
  workId: string,
  destination: string,
): Promise<{ destination: string }> {
  return forgeFetch(`/v1/forge/items/${encodeURIComponent(workId)}/export`, {
    method: "POST",
    body: JSON.stringify({ destination }),
  });
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
      return "Ready to set up";
    case "work":
      return "In progress";
    case "review":
      return "Ready to review";
    case "complete":
      return "Finished";
    case "needs_attention":
      return "Needs attention";
    default:
      return "In progress";
  }
}

/** User-facing orientation for Forge phases. Internal state names stay behind details. */
export function humanPhaseGuidance(phase: string): string {
  switch (phase) {
    case "prepare":
      return "Medousa will make a safe place for this change.";
    case "work":
      return "Your files, tools, and agents stay together here.";
    case "review":
      return "See what changed, then decide what to keep.";
    case "complete":
      return "This work is preserved and ready whenever you need it.";
    case "needs_attention":
      return "Medousa kept your work safe and needs your decision.";
    default:
      return "Your work stays together while you decide what happens next.";
  }
}

export function humanExecutorLabel(executor: string | null | undefined): string | null {
  if (!executor) return null;
  if (executor === "human") return "You";
  if (executor === "codex") return "Codex";
  if (executor === "cursor") return "Cursor";
  return "Agent";
}

/** Keep daemon diagnostics useful without making users learn Forge's machinery. */
export function humanizeForgeMessage(message: string): string {
  return message
    .replace(/\bgoverned workspaces\b/gi, "projects")
    .replace(/\bgoverned workspace\b/gi, "project")
    .replace(/\bworktrees\b/gi, "working copies")
    .replace(/\bworktree\b/gi, "working copy")
    .replace(/\bactive leases\b/gi, "active editing sessions")
    .replace(/\bactive lease\b/gi, "active editing session")
    .replace(/\bleases\b/gi, "editing sessions")
    .replace(/\blease\b/gi, "editing session")
    .replace(/\bsource files\b/gi, "files")
    .replace(/\bsource file\b/gi, "file")
    .replace(/\bsource changes\b/gi, "file changes")
    .replace(/\bundertakings\b/gi, "projects")
    .replace(/\bundertaking\b/gi, "project")
    .replace(/\bcheckpoints\b/gi, "sets of changes")
    .replace(/\bcheckpoint\b/gi, "set of changes")
    .replace(/\bsealed\b/gi, "ready for review");
}
