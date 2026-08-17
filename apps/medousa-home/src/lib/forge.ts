/** Forge / Undertakings HTTP client for daemon forge and world operations. */

import { invoke } from "@tauri-apps/api/core";
import { getDaemonUrl, operationPath, type OperationId } from "$lib/daemon";
import { streamPathWithSince } from "$lib/stream/reconnect";
import { isTauri } from "$lib/window";

export type ActionAffordance = {
  allowed: boolean;
  reason?: string | null;
};

export type AllowedActions = {
  provision: ActionAffordance;
  start_agent: ActionAffordance;
  open_terminal: ActionAffordance;
  begin_attempt: ActionAffordance;
  /** Present once the workshop advertises reopen-without-agent. */
  continue_editing?: ActionAffordance;
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
  /** Human-readable worktree/branch identity derived from title. */
  slug?: string;
  state: string;
  owner: string;
  created_at?: string;
  updated_at?: string;
  environment?: {
    worktree: string;
    branch: string;
    baseline_oid: string;
    generation: number;
  } | null;
  attempts?: Array<{
    id: string;
    seq: number;
    state: string;
    executor?: { kind?: string; detail?: Record<string, unknown> } | null;
    environment?: {
      worktree: string;
      branch: string;
      baseline_oid: string;
      generation: number;
    } | null;
    evidence_id?: string | null;
    lease?: {
      lease_id: string;
      generation: number;
    } | null;
  }>;
  active_attempt?: string | null;
  active_attempts?: string[];
  review_decisions?: Array<{ id: string; strategy: string }>;
  disposition?: string | null;
  /** Internally tagged: `{ kind: "git", repo_path, base_ref, base_oid }`. */
  target?: GitWorkTarget | null;
};

export type GitWorkTarget = {
  kind: "git";
  repo_path: string;
  base_ref: string;
  base_oid?: string;
};

export function gitTargetRepoPath(
  target: GitWorkTarget | null | undefined,
): string | null {
  const path = target?.kind === "git" ? target.repo_path?.trim() : "";
  return path || null;
}

export function gitTargetBaseRef(
  target: GitWorkTarget | null | undefined,
): string | null {
  const ref = target?.kind === "git" ? target.base_ref?.trim() : "";
  return ref || null;
}

export type ItemProjection = ForgeWorkItem & {
  human_phase: HumanPhase | string;
  allowed_actions: AllowedActions;
  /** Missing on older daemons; environment presence is the compatibility fallback. */
  workspace_present?: boolean;
};

export type ReviewProjection = {
  work_id: string;
  title: string;
  state: string;
  human_phase: string;
  allowed_actions: AllowedActions;
  candidates: Array<{
    attempt_id: string;
    attempt_seq: number;
    executor: string;
    evidence_id: string;
    evidence_digest: string;
    baseline_oid: string;
    sealed_head_oid: string;
    branch: string;
    worktree: string;
    changed_file_count: number;
    sealed_at: string;
    decision_id?: string | null;
  }>;
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
    lines_added?: number;
    lines_removed?: number;
    intents?: string[];
    primary_intent?: string | null;
    symbol_count?: number;
    scopes?: ReviewSymbolScope[];
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
    issues?: Array<{
      id: string;
      message: string;
      severity: "high" | "attention" | "info" | string;
      blocks_approval: boolean;
    }>;
    blocks_approval?: boolean;
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
    count?: number;
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
  comments?: ReviewComment[];
  unresolved_comment_count?: number;
  revision_brief?: string | null;
  changed_since_previous?: string[];
  policy?: PolicyReport | null;
  command_log_lines: number;
  patch_byte_size: number;
  decision?: {
    id?: string;
    strategy?: string;
    rationale?: string | null;
  } | null;
  disposition?: string | null;
  worktree?: string | null;
  active_lease_id?: string | null;
  active_lease_generation?: number | null;
  world?: WorldBindingStatus | null;
};

export type ReviewSymbolScope = {
  id: string;
  label: string;
  kind: string;
  line_start: number;
  line_end: number;
  entity_id?: string | null;
  lines_added: number;
  lines_removed: number;
  intents?: string[];
};

export type ReviewFileChange = ReviewProjection["changed_files"][number];

export type ReviewComment = {
  id: string;
  thread_id: string;
  parent_id?: string | null;
  evidence_id: string;
  attempt_id: string;
  path: string;
  side: "new" | "old" | string;
  start_line: number;
  end_line: number;
  anchor_digest: string;
  anchor_text?: string | null;
  body: string;
  actor_kind: string;
  actor_id: string;
  created_at: string;
  resolved_at?: string | null;
  resolved_by_kind?: string | null;
  resolved_by_id?: string | null;
};

export type ReviewFileDiff = {
  work_id: string;
  attempt_id: string;
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
  lease: {
    lease_id: string;
    generation: number;
    work_id: string;
    attempt_id: string;
  };
  attempt_id: string;
  worktree: string;
  branch: string;
};

export type ForgeSourceFile = {
  work_id: string;
  path: string;
  content: string;
  digest: string;
  byte_size: number;
  /** `utf-8`, `utf-8-lossy`, or `binary` when the workshop reports it. */
  encoding?: string | null;
  /** True when content is a bounded preview (large/binary/lossy), not editable. */
  preview?: boolean;
  /** True when a text preview was truncated to the editor byte limit. */
  truncated?: boolean;
};

export type ForgeSourceWorkspacePrecondition =
  | { kind: "existing"; path: string; expected_digest: string }
  | { kind: "missing"; path: string };

export type ForgeSourceWorkspaceOperation =
  | { kind: "write"; path: string; content: string }
  | { kind: "create"; path: string; content: string }
  | { kind: "rename"; path: string; destination: string }
  | { kind: "delete"; path: string };

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
  next_cursor?: string | null;
};

export type ForgeSourceSearchOptions = {
  query: string;
  mode?: "literal" | "regex";
  caseSensitive?: boolean;
  wholeWord?: boolean;
  include?: string;
  exclude?: string;
  includeIgnored?: boolean;
  scope?: "all" | "changed";
  limit?: number;
  cursor?: string | null;
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
  /** Contextual Code regions (Problems/Terminal/Tests/Search/Changes); additive. */
  layout?: {
    context_panel?: "problems" | "outline" | "references" | "language" | null;
    terminal?: boolean;
    tests?: boolean;
    search?: boolean;
    changes?: boolean;
  } | null;
  updated_at?: string | null;
};

async function forgeUrl(path: string): Promise<string> {
  const base = (await getDaemonUrl()).replace(/\/$/, "");
  return `${base}${path.startsWith("/") ? path : `/${path}`}`;
}

export async function forgeStreamUrl(): Promise<string> {
  return forgeUrl(operationPath("forge.stream.get"));
}

export type ForgeProjectEventKind =
  | "created"
  | "changed"
  | "renamed"
  | "deleted"
  | "git_status"
  | "snapshot";

/** Path-aware project event from GET …/project-events (SSE `project`). */
export type ForgeProjectEvent = {
  seq: number;
  work_id: string;
  kind: ForgeProjectEventKind;
  path?: string | null;
  old_path?: string | null;
  digest?: string | null;
  updated_at: string;
};

/** Resumable source/Git event stream for one undertaking (`?since=` replay). */
export async function forgeProjectEventsUrl(
  workId: string,
  since = 0,
): Promise<string> {
  const path = streamPathWithSince(
    operationPath("forge.items.by_work_id.project_events.get", { work_id: workId }),
    since,
  );
  return forgeUrl(path);
}

async function forgeFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  if (isTauri()) {
    let body: unknown = null;
    if (typeof init?.body === "string" && init.body.length > 0) {
      body = JSON.parse(init.body);
    } else if (init?.body != null) {
      throw new Error("Forge request body must be JSON");
    }
    try {
      return await invoke<T>("forge_request", {
        method: init?.method ?? "GET",
        path,
        body,
      });
    } catch (cause) {
      const raw = cause instanceof Error ? cause.message : String(cause);
      const err = new Error(raw) as Error & { status?: number; path?: string };
      const status = raw.match(/HTTP\s+(\d{3})/i)?.[1];
      if (status) err.status = Number(status);
      err.path = path;
      throw err;
    }
  }

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
    const err = new Error(detail) as Error & { status?: number; path?: string };
    err.status = res.status;
    err.path = path;
    throw err;
  }
  if (res.status === 204) return undefined as T;
  return (await res.json()) as T;
}

/** True when the workshop binary is older than the Home UI's Forge surface. */
export function isMissingForgeRoute(err: unknown): boolean {
  const status = (err as { status?: number } | null)?.status;
  if (status === 404 || status === 405) return true;
  const message = err instanceof Error ? err.message : String(err ?? "");
  return /HTTP\s+404\b/i.test(message) || /HTTP\s+405\b/i.test(message);
}

function folderNameFromPath(path: string): string {
  const parts = path.split(/[/\\]/).filter(Boolean);
  return parts[parts.length - 1] || path;
}

/** Local fallback when repository inspect is missing on older daemons. */
export function synthesizeRepositoryInspection(path: string): RepositoryInspection {
  const trimmed = path.trim();
  return {
    path: trimmed,
    display_name: folderNameFromPath(trimmed),
    current_branch: null,
    suggested_base_ref: "main",
    has_commits: true,
    dirty: false,
    changed_files: 0,
    remotes: [],
    existing_projects: [],
    state_explanation:
      "This workshop cannot inspect Git yet. Medousa will still start the project from this folder.",
    trust_explanation:
      "Update medousa_daemon to get branch, dirty-state, and duplicate-project checks.",
  };
}

export async function listUndertakings(): Promise<ItemProjection[]> {
  const items: ItemProjection[] = [];
  const seenCursors = new Set<string>();
  let cursor: string | null = null;
  do {
    const query = new URLSearchParams({ limit: "256" });
    if (cursor) query.set("cursor", cursor);
    const payload = await forgeFetch<
      | ItemProjection[]
      | {
          items: ItemProjection[];
          next_cursor?: string | null;
          truncated?: boolean;
        }
    >(operationPath("forge.items.get") + '?' + query);
    if (Array.isArray(payload)) return payload;
    items.push(...payload.items);
    const next = payload.truncated ? (payload.next_cursor ?? null) : null;
    if (next && seenCursors.has(next)) {
      throw new Error("Forge item pagination returned a repeated cursor");
    }
    if (next) seenCursors.add(next);
    cursor = next;
  } while (cursor);
  return items;
}

export async function getUndertaking(workId: string): Promise<ItemProjection> {
  return forgeFetch(operationPath("forge.items.by_work_id.get", { work_id: workId }));
}

export async function getUndertakingSource(
  workId: string,
  path: string,
): Promise<ForgeSourceFile> {
  const query = new URLSearchParams({ path });
  return forgeFetch(
    operationPath("forge.items.by_work_id.source.get", { work_id: workId }) + '?' + query,
  );
}

export async function getUndertakingSourceTree(
  workId: string,
): Promise<ForgeSourceTree> {
  return forgeFetch(operationPath("forge.items.by_work_id.tree.get", { work_id: workId }));
}

export async function searchUndertakingSource(
  workId: string,
  queryOrOptions: string | ForgeSourceSearchOptions,
): Promise<ForgeSourceSearch> {
  const options: ForgeSourceSearchOptions =
    typeof queryOrOptions === "string"
      ? { query: queryOrOptions }
      : queryOrOptions;
  const params = new URLSearchParams();
  params.set("query", options.query);
  if (options.mode) params.set("mode", options.mode);
  if (options.caseSensitive != null) {
    params.set("case_sensitive", String(options.caseSensitive));
  }
  if (options.wholeWord != null) {
    params.set("whole_word", String(options.wholeWord));
  }
  if (options.include?.trim()) params.set("include", options.include.trim());
  if (options.exclude?.trim()) params.set("exclude", options.exclude.trim());
  if (options.includeIgnored != null) {
    params.set("include_ignored", String(options.includeIgnored));
  }
  if (options.scope) params.set("scope", options.scope);
  if (options.limit != null) params.set("limit", String(options.limit));
  if (options.cursor) params.set("cursor", options.cursor);
  return forgeFetch(
    operationPath("forge.items.by_work_id.search.get", { work_id: workId }) + '?' + params,
  );
}

export type ForgeSourceReplaceFile = {
  path: string;
  expected_digest: string;
  match_count: number;
  before: string;
  after: string;
};

export type ForgeSourceReplacePlan = {
  work_id: string;
  files: ForgeSourceReplaceFile[];
  truncated: boolean;
  applied: boolean;
};

export type ForgeSourceReplaceOptions = ForgeSourceSearchOptions & {
  replacement: string;
  dryRun?: boolean;
  paths?: string[];
  preconditions?: Array<{ path: string; expected_digest: string }>;
  lease_id?: string;
  generation?: number;
};

export async function replaceUndertakingSource(
  workId: string,
  options: ForgeSourceReplaceOptions,
): Promise<ForgeSourceReplacePlan> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.search.replace.post", { work_id: workId }),
    {
      method: "POST",
      body: JSON.stringify({
        query: options.query,
        replacement: options.replacement,
        mode: options.mode,
        case_sensitive: options.caseSensitive,
        whole_word: options.wholeWord,
        include: options.include,
        exclude: options.exclude,
        include_ignored: options.includeIgnored,
        scope: options.scope,
        limit: options.limit,
        dry_run: options.dryRun ?? true,
        paths: options.paths,
        preconditions: options.preconditions,
        lease_id: options.lease_id,
        generation: options.generation,
      }),
    },
  );
}

export async function getCodeWorkspaceState(
  workId: string,
): Promise<ForgeCodeWorkspaceState> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.workspace_state.get", { work_id: workId }),
  );
}

export async function saveCodeWorkspaceState(
  workId: string,
  state: ForgeCodeWorkspaceState,
  lease?: { lease_id: string; generation: number } | null,
): Promise<ForgeCodeWorkspaceState> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.workspace_state.put", { work_id: workId }),
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
  return forgeFetch(operationPath("forge.items.by_work_id.source.put", { work_id: workId }), {
    method: "PUT",
    body: JSON.stringify(input),
  });
}

export async function saveUndertakingSources(
  workId: string,
  input: {
    files: Array<{ path: string; content: string; expected_digest: string }>;
    lease_id: string;
    generation: number;
  },
): Promise<ForgeSourceFile[]> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.source.batch.put", { work_id: workId }),
    {
      method: "PUT",
      body: JSON.stringify(input),
    },
  );
}

/** Apply one optimistic, all-or-nothing text and resource workspace edit. */
export async function applyUndertakingSourceWorkspaceEdit(
  workId: string,
  input: {
    preconditions: ForgeSourceWorkspacePrecondition[];
    operations: ForgeSourceWorkspaceOperation[];
    lease_id: string;
    generation: number;
  },
): Promise<ForgeSourceFile[]> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.source.workspace_edit.put", { work_id: workId }),
    {
      method: "PUT",
      body: JSON.stringify(input),
    },
  );
}

export async function createUndertakingSource(
  workId: string,
  input: {
    path: string;
    content?: string;
    kind?: "file" | "directory";
    lease_id: string;
    generation: number;
  },
): Promise<ForgeSourceFile> {
  return forgeFetch(operationPath("forge.items.by_work_id.source.post", { work_id: workId }), {
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
  return forgeFetch(operationPath("forge.items.by_work_id.source.patch", { work_id: workId }), {
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
  return forgeFetch(operationPath("forge.items.by_work_id.source.delete", { work_id: workId }), {
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
  return forgeFetch(operationPath("forge.items.post"), {
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
  suggested_base_ref?: string | null;
  /** Missing on older workshop daemons; only an explicit false blocks start. */
  has_commits?: boolean;
  dirty: boolean;
  changed_files: number;
  remotes: string[];
  /** Missing on older workshop daemons. */
  local_branches?: string[];
  /** Missing on older workshop daemons. */
  remote_branches?: Array<{
    name: string;
    branches: string[];
    default_branch?: string | null;
  }>;
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
  archived: boolean;
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

export type ProviderRepositoryAdapter = {
  provider: "github" | "gitlab" | string;
  label: string;
  available: boolean;
  message: string;
};

export type ProviderRepositoryCapabilities = {
  adapters: ProviderRepositoryAdapter[];
};

export type ProjectTask = {
  id: string;
  label: string;
  kind: "verify" | "test" | "build" | "run" | string;
  argv: string[];
  provider: string;
  long_running?: boolean;
  ready_pattern?: string | null;
  problem_matcher?: {
    regexp: string;
    file: number;
    line: number;
    column?: number | null;
    message?: number | null;
  } | null;
};

export type ProjectTaskResult = {
  task: ProjectTask;
  success: boolean;
  exit_code?: number | null;
  stdout: string;
  stderr: string;
  truncated: boolean;
  duration_ms: number;
  locations: Array<{ path: string; line: number; column?: number | null; message: string }>;
};

export type ProjectTaskLocation = {
  path: string;
  line: number;
  column?: number | null;
  message: string;
};

export type ProjectTaskRun = {
  run_id: string;
  work_id: string;
  state: "running" | "ready" | "passed" | "failed" | "cancelled" | string;
  task: ProjectTask;
  result?: ProjectTaskResult | null;
  /** Bounded live stdout (also retained after exit for replay). */
  stdout?: string;
  stderr?: string;
  output_truncated?: boolean;
  next_seq?: number;
  locations?: ProjectTaskLocation[];
  /** Loopback URL when a background task reports ready. */
  ready_url?: string | null;
};

export type ProjectTaskOutputEvent = {
  seq: number;
  run_id: string;
  kind: "output" | "state" | string;
  stream?: string | null;
  text?: string | null;
  state?: string | null;
  result?: ProjectTaskResult | null;
  locations?: ProjectTaskLocation[] | null;
  ready_url?: string | null;
};

export type ProjectTaskRunPreview = {
  work_id: string;
  run_id: string;
  ready_url: string;
  port: number;
  token: string;
  preview_path: string;
};

export type ProjectTest = {
  id: string;
  label: string;
  path: string;
  line: number;
  task_id: string;
};

export type ForgeChangesFile = {
  path: string;
  status: string;
  old_path?: string | null;
};

export type ForgeChanges = {
  work_id: string;
  branch?: string | null;
  detached?: boolean;
  base_ref?: string | null;
  baseline_oid?: string | null;
  upstream?: string | null;
  ahead?: number | null;
  behind?: number | null;
  conflict: boolean;
  dirty?: boolean;
  merge_in_progress?: boolean;
  files: ForgeChangesFile[];
};

export type ChangesFileDiff = {
  work_id: string;
  path: string;
  status: string;
  old_path?: string | null;
  baseline_oid: string;
  working_digest?: string | null;
  binary: boolean;
  conflict: boolean;
  baseline: ReviewFileVersion;
  working: ReviewFileVersion;
  hunks: ReviewDiffHunk[];
  truncated: boolean;
};

export type RestoreChangesFileResponse = {
  work_id: string;
  path: string;
  action: string;
  digest?: string | null;
};

export type ChangesSyncResult = {
  work_id: string;
  fetched: boolean;
  pulled: boolean;
  pushed: boolean;
  message: string;
  changes: ForgeChanges;
};

export type ChangesHistoryEntry = {
  oid: string;
  author_name: string;
  author_email: string;
  authored_at: number;
  subject: string;
};

export type ChangesBlameHunk = {
  oid: string;
  author_name: string;
  author_email: string;
  authored_at: number;
  summary: string;
  start_line: number;
  line_count: number;
};

export async function getProjectTasks(workId: string): Promise<ProjectTask[]> {
  return forgeFetch(operationPath("forge.items.by_work_id.tasks.get", { work_id: workId }));
}

export async function runProjectTask(
  workId: string,
  taskId: string,
  lease: { lease_id: string; generation: number; test_id?: string },
): Promise<ProjectTaskResult> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.tasks.by_task_id.run.post", { work_id: workId, task_id: taskId }),
    { method: "POST", body: JSON.stringify(lease) },
  );
}

export async function startProjectTaskRun(
  workId: string,
  taskId: string,
  lease: { lease_id: string; generation: number; test_id?: string },
): Promise<ProjectTaskRun> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.tasks.by_task_id.runs.post", { work_id: workId, task_id: taskId }),
    { method: "POST", body: JSON.stringify(lease) },
  );
}

export async function getProjectTaskRun(workId: string, runId: string): Promise<ProjectTaskRun> {
  return forgeFetch(operationPath("forge.items.by_work_id.task_runs.by_run_id.get", { work_id: workId, run_id: runId }));
}

export async function cancelProjectTaskRun(workId: string, runId: string): Promise<ProjectTaskRun> {
  return forgeFetch(operationPath("forge.items.by_work_id.task_runs.by_run_id.delete", { work_id: workId, run_id: runId }), {
    method: "DELETE",
  });
}

/** Mint or reuse a tokenized workshop preview path for a ready task run. */
export async function createProjectTaskRunPreview(
  workId: string,
  runId: string,
): Promise<ProjectTaskRunPreview> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.task_runs.by_run_id.preview.post", { work_id: workId, run_id: runId }),
    { method: "POST" },
  );
}

/** Resumable task-run output stream (`?since=` chunk replay). */
export async function forgeTaskRunEventsUrl(
  workId: string,
  runId: string,
  since = 0,
): Promise<string> {
  const path = streamPathWithSince(
    operationPath("forge.items.by_work_id.task_runs.by_run_id.events.get", { work_id: workId, run_id: runId }),
    since,
  );
  return forgeUrl(path);
}

export async function getProjectTests(workId: string): Promise<ProjectTest[]> {
  return forgeFetch(operationPath("forge.items.by_work_id.tests.get", { work_id: workId }));
}

export type ProviderHandoff = {
  provider: "github" | "gitlab" | "none" | string;
  available: boolean;
  repository?: string | null;
  remote_url?: string | null;
  branch?: string | null;
  base_branch?: string | null;
  shared: boolean;
  review_url?: string | null;
  links: string[];
  message: string;
};

export type ProviderComment = {
  id: string;
  author: string;
  body: string;
  url?: string | null;
};

export async function getProviderHandoff(workId: string): Promise<ProviderHandoff> {
  return forgeFetch(operationPath("forge.items.by_work_id.provider.get", { work_id: workId }));
}

export async function shareProviderHandoff(
  workId: string,
  input: { title?: string; body?: string; attempt_id?: string },
): Promise<ProviderHandoff> {
  return forgeFetch(operationPath("forge.items.by_work_id.provider.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function saveProviderContext(
  workId: string,
  links: string[],
): Promise<ProviderHandoff> {
  return forgeFetch(operationPath("forge.items.by_work_id.provider.context.put", { work_id: workId }), {
    method: "PUT",
    body: JSON.stringify({ links }),
  });
}

export async function getProviderComments(workId: string): Promise<ProviderComment[]> {
  return forgeFetch(operationPath("forge.items.by_work_id.provider.comments.get", { work_id: workId }));
}

export async function importProviderComment(
  workId: string,
  comment: ProviderComment,
): Promise<ItemProjection> {
  return forgeFetch(operationPath("forge.items.by_work_id.provider.comments.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify(comment),
  });
}

export async function inspectForgeRepository(path: string): Promise<RepositoryInspection> {
  try {
    return await forgeFetch(operationPath("forge.repositories.inspect.post"), {
      method: "POST",
      body: JSON.stringify({ path }),
    });
  } catch (err) {
    if (isMissingForgeRoute(err)) {
      return synthesizeRepositoryInspection(path);
    }
    throw err;
  }
}

export async function listForgeRepositories(): Promise<RepositoryCatalogEntry[]> {
  return forgeFetch(operationPath("forge.repositories.get"));
}

export async function setForgeRepositoryPinned(
  path: string,
  pinned: boolean,
): Promise<RepositoryCatalogEntry[]> {
  return forgeFetch(operationPath("forge.repositories.put"), {
    method: "PUT",
    body: JSON.stringify({ path, pinned }),
  });
}

export async function setForgeRepositoryArchived(
  path: string,
  archived: boolean,
): Promise<RepositoryCatalogEntry[]> {
  return forgeFetch(operationPath("forge.repositories.put"), {
    method: "PUT",
    body: JSON.stringify({ path, archived }),
  });
}

export async function browseForgeRepositories(
  path?: string | null,
): Promise<RepositoryBrowseResponse> {
  const query = path ? `?path=${encodeURIComponent(path)}` : "";
  return forgeFetch(operationPath("forge.repositories.browse.get") + query);
}

export async function getProviderRepositoryCapabilities(): Promise<ProviderRepositoryCapabilities> {
  return forgeFetch(operationPath("forge.repositories.provider.get"));
}

export async function cloneProviderRepository(input: {
  provider: string;
  repository: string;
  parent: string;
}): Promise<RepositoryInspection> {
  return forgeFetch(operationPath("forge.repositories.provider.post"), {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function startUndertaking(input: {
  title: string;
  brief: string;
  repo_path: string;
  base_ref?: string;
}): Promise<ItemProjection> {
  try {
    return await forgeFetch(operationPath("forge.items.start.post"), {
      method: "POST",
      body: JSON.stringify({
        title: input.title,
        brief: input.brief,
        repo_path: input.repo_path,
        base_ref: input.base_ref ?? "main",
      }),
    });
  } catch (err) {
    // Older workshops only have register + provision as separate steps.
    if (!isMissingForgeRoute(err)) throw err;
    const registered = await createUndertaking(input);
    return provisionUndertaking(registered.id);
  }
}

export async function provisionUndertaking(workId: string): Promise<ItemProjection> {
  return forgeFetch(operationPath("forge.items.by_work_id.provision.post", { work_id: workId }), {
    method: "POST",
    body: "{}",
  });
}

export async function beginHumanAttempt(workId: string): Promise<BeginAttemptResponse> {
  return forgeFetch(operationPath("forge.items.by_work_id.attempts.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify({ executor: { kind: "human", detail: {} } }),
  });
}

/** Reopen sealed review and begin a human attempt (no agent). */
export async function continueEditing(workId: string): Promise<BeginAttemptResponse> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.review.continue_editing.post", { work_id: workId }),
    { method: "POST", body: "{}" },
  );
}

/** True when a human can start or resume editing without an agent. */
export function canStartHumanEditing(actions: AllowedActions | null | undefined): boolean {
  return Boolean(actions?.begin_attempt.allowed || actions?.continue_editing?.allowed);
}

/**
 * Begin a human editing session: either a normal attempt (Ready) or continue
 * editing after review (AwaitingReview → reopen + human lease).
 */
export async function startHumanEditingSession(
  workId: string,
  actions: AllowedActions,
): Promise<BeginAttemptResponse> {
  if (actions.begin_attempt.allowed) {
    return beginHumanAttempt(workId);
  }
  if (actions.continue_editing?.allowed) {
    return continueEditing(workId);
  }
  throw new Error(
    actions.continue_editing?.reason
      ?? actions.begin_attempt.reason
      ?? "This project is not ready for file changes",
  );
}

export async function prepareExecutorHandoff(input: {
  work_id: string;
  lease_id: string;
  generation: number;
  to_executor: "codex" | "cursor" | "human";
}): Promise<ItemProjection> {
  return forgeFetch(operationPath("forge.items.by_work_id.handoff.post", { work_id: input.work_id }), {
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
  return forgeFetch(operationPath("forge.leases.by_lease_id.complete.post", { lease_id: leaseId }), {
    method: "POST",
    body: JSON.stringify({ generation }),
  });
}

export async function heartbeatLease(leaseId: string, generation: number): Promise<void> {
  return forgeFetch(operationPath("forge.leases.by_lease_id.heartbeat.post", { lease_id: leaseId }), {
    method: "POST",
    body: JSON.stringify({ generation }),
  });
}

export async function getReview(workId: string, attemptId?: string): Promise<ReviewProjection> {
  const query = new URLSearchParams();
  if (attemptId) query.set("attempt_id", attemptId);
  const suffix = query.size ? `?${query.toString()}` : "";
  return forgeFetch(operationPath("forge.items.by_work_id.review.get", { work_id: workId }) + suffix);
}

export async function getForgeChanges(workId: string): Promise<ForgeChanges> {
  return forgeFetch(operationPath("forge.items.by_work_id.changes.get", { work_id: workId }));
}

export async function getChangesFile(
  workId: string,
  path: string,
): Promise<ChangesFileDiff> {
  const query = new URLSearchParams({ path });
  return forgeFetch(
    operationPath("forge.items.by_work_id.changes.file.get", { work_id: workId }) + '?' + query.toString(),
  );
}

export async function restoreChangesFile(
  workId: string,
  input: {
    path: string;
    expected_working_digest?: string | null;
    lease_id: string;
    generation: number;
  },
): Promise<RestoreChangesFileResponse> {
  return forgeFetch(operationPath("forge.items.by_work_id.changes.file.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify(input),
  });
}

async function changesLeaseAction(
  workId: string,
  action: "fetch" | "pull" | "push" | "sync" | "checkpoint",
  lease: {
    lease_id: string;
    generation: number;
    remote?: string;
    ack_risks?: boolean;
  },
): Promise<ChangesSyncResult | ItemProjection> {
  return forgeFetch(operationPath(`forge.items.by_work_id.changes.${action}.post` as OperationId, { work_id: workId }), {
    method: "POST",
    body: JSON.stringify(lease),
  });
}

export async function fetchChanges(
  workId: string,
  lease: { lease_id: string; generation: number; remote?: string },
): Promise<ChangesSyncResult> {
  return changesLeaseAction(workId, "fetch", lease) as Promise<ChangesSyncResult>;
}

export async function pullChanges(
  workId: string,
  lease: { lease_id: string; generation: number; remote?: string },
): Promise<ChangesSyncResult> {
  return changesLeaseAction(workId, "pull", lease) as Promise<ChangesSyncResult>;
}

export async function pushChanges(
  workId: string,
  lease: { lease_id: string; generation: number; remote?: string },
): Promise<ChangesSyncResult> {
  return changesLeaseAction(workId, "push", lease) as Promise<ChangesSyncResult>;
}

export async function syncChanges(
  workId: string,
  lease: { lease_id: string; generation: number; remote?: string },
): Promise<ChangesSyncResult> {
  return changesLeaseAction(workId, "sync", lease) as Promise<ChangesSyncResult>;
}

export async function checkpointChanges(
  workId: string,
  lease: { lease_id: string; generation: number; ack_risks?: boolean },
): Promise<ItemProjection> {
  return changesLeaseAction(workId, "checkpoint", lease) as Promise<ItemProjection>;
}

export async function getChangesHistory(
  workId: string,
  limit = 50,
): Promise<{ work_id: string; commits: ChangesHistoryEntry[] }> {
  const query = new URLSearchParams({ limit: String(limit) });
  return forgeFetch(
    operationPath("forge.items.by_work_id.changes.history.get", { work_id: workId }) + '?' + query.toString(),
  );
}

export async function getChangesBlame(
  workId: string,
  path: string,
): Promise<{ work_id: string; path: string; hunks: ChangesBlameHunk[] }> {
  const query = new URLSearchParams({ path });
  return forgeFetch(
    operationPath("forge.items.by_work_id.changes.blame.get", { work_id: workId }) + '?' + query.toString(),
  );
}

export async function resolveChangesConflict(
  workId: string,
  input: {
    path: string;
    resolution: "ours" | "theirs" | "baseline";
    expected_working_digest?: string | null;
    lease_id: string;
    generation: number;
  },
): Promise<{ work_id: string; path: string; action: string; changes: ForgeChanges }> {
  return forgeFetch(operationPath("forge.items.by_work_id.changes.conflict.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function revertChangesHunk(
  workId: string,
  input: {
    path: string;
    hunk_index: number;
    expected_working_digest: string;
    lease_id: string;
    generation: number;
  },
): Promise<RestoreChangesFileResponse> {
  return forgeFetch(operationPath("forge.items.by_work_id.changes.file.hunk.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify(input),
  });
}

export async function getReviewFile(
  workId: string,
  path: string,
  attemptId?: string,
): Promise<ReviewFileDiff> {
  const query = new URLSearchParams({ path });
  if (attemptId) query.set("attempt_id", attemptId);
  return forgeFetch(
    operationPath("forge.items.by_work_id.review.file.get", { work_id: workId }) + '?' + query.toString(),
  );
}

export async function restoreReviewFile(
  workId: string,
  input: { path: string; expected_reviewed_oid: string; attempt_id?: string },
): Promise<RestoreReviewFileResponse> {
  return forgeFetch(operationPath("forge.items.by_work_id.review.file.post", { work_id: workId }), {
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
    operationPath("forge.evidence.by_evidence_id.patch.get", { evidence_id: evidenceId }) + (qs ? "?" + qs : ""),
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
    operationPath("forge.evidence.by_evidence_id.commands.get", { evidence_id: evidenceId }) + (qs ? "?" + qs : ""),
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
  return forgeFetch(operationPath("forge.items.by_work_id.decisions.post", { work_id: workId }), {
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

export async function listReviewComments(
  workId: string,
  attemptId?: string,
): Promise<ReviewComment[]> {
  const query = new URLSearchParams();
  if (attemptId) query.set("attempt_id", attemptId);
  const suffix = query.size ? `?${query.toString()}` : "";
  return forgeFetch(operationPath("forge.items.by_work_id.review.comments.get", { work_id: workId }) + suffix);
}

export async function addReviewComment(
  workId: string,
  input: {
    evidence_id: string;
    attempt_id?: string;
    path: string;
    side?: "new" | "old" | string;
    start_line: number;
    end_line?: number;
    anchor_text?: string | null;
    body: string;
    parent_id?: string | null;
  },
): Promise<ItemProjection> {
  return forgeFetch(operationPath("forge.items.by_work_id.review.comments.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify({
      evidence_id: input.evidence_id,
      attempt_id: input.attempt_id ?? null,
      path: input.path,
      side: input.side ?? "new",
      start_line: input.start_line,
      end_line: input.end_line ?? input.start_line,
      anchor_text: input.anchor_text ?? null,
      body: input.body,
      parent_id: input.parent_id ?? null,
    }),
  });
}

export async function resolveReviewComment(
  workId: string,
  commentId: string,
): Promise<ItemProjection> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.review.comments.by_comment_id.patch", { work_id: workId, comment_id: commentId }),
    {
      method: "PATCH",
      body: JSON.stringify({ resolve: true }),
    },
  );
}

export async function deleteReviewComment(
  workId: string,
  commentId: string,
): Promise<ItemProjection> {
  return forgeFetch(
    operationPath("forge.items.by_work_id.review.comments.by_comment_id.delete", { work_id: workId, comment_id: commentId }),
    { method: "DELETE" },
  );
}

export async function requestReviewChanges(
  workId: string,
  input: {
    evidence_id: string;
    evidence_digest: string;
    summary?: string;
    comment_ids?: string[];
  },
): Promise<ItemProjection> {
  return forgeFetch(operationPath("forge.items.by_work_id.review.request_changes.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify({
      evidence_id: input.evidence_id,
      evidence_digest: input.evidence_digest,
      summary: input.summary ?? null,
      comment_ids: input.comment_ids ?? null,
    }),
  });
}

export async function applyDecision(
  workId: string,
  decisionId: string,
): Promise<ItemProjection> {
  return forgeFetch(operationPath("forge.items.by_work_id.apply.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify({ decision_id: decisionId }),
  });
}

export async function discardUndertaking(workId: string): Promise<ItemProjection> {
  return forgeFetch(operationPath("forge.items.by_work_id.discard.post", { work_id: workId }), {
    method: "POST",
    body: "{}",
  });
}

export async function getWorldBinding(workId: string): Promise<WorldBindingStatus> {
  return forgeFetch(operationPath("world.bindings.by_work_id.get", { work_id: workId }));
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
  return forgeFetch(operationPath("world.code_avec.get") + '?' + worldQuery(workId, snapshot));
}

export async function getWorldFiles(
  workId: string,
  path?: string,
  snapshot?: WorldSnapshotRef | null,
): Promise<WorldFilesResult> {
  const q = worldQuery(workId, snapshot);
  if (path) q.set("path", path);
  return forgeFetch(operationPath("world.files.get") + '?' + q);
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
  return forgeFetch(operationPath("world.find.get") + '?' + q);
}

export async function getWorldImpact(
  workId: string,
  entityId: string,
  snapshot?: WorldSnapshotRef | null,
): Promise<WorldImpactResult> {
  const q = worldQuery(workId, snapshot);
  q.set("entity_id", entityId);
  return forgeFetch(operationPath("world.impact.get") + '?' + q);
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
  return forgeFetch(operationPath("world.at_location.get") + '?' + q);
}

export async function exportUndertakingBundle(
  workId: string,
  destination: string,
): Promise<{ destination: string }> {
  return forgeFetch(operationPath("forge.items.by_work_id.export.post", { work_id: workId }), {
    method: "POST",
    body: JSON.stringify({ destination }),
  });
}

export async function queueWorldIndex(
  workId: string,
  kind: "baseline" | "sealed" = "sealed",
): Promise<unknown> {
  return forgeFetch(operationPath("world.index.post"), {
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
      return "Set up the working copy to begin editing.";
    case "work":
      return "Edit, run, or hand off — same working copy.";
    case "review":
      return "Inspect the diff, then keep or discard.";
    case "complete":
      return "Preserved; reopen anytime.";
    case "needs_attention":
      return "A decision is blocked — resolve to continue.";
    default:
      return "Edit in the working copy.";
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
  const trimmed = message.trim();
  if (
    /no ready indexed snapshot/i.test(trimmed) ||
    (/HTTP\s+404\b/i.test(trimmed) && /\/v1\/world\//i.test(trimmed)) ||
    (/indexed snapshot/i.test(trimmed) && /not (ready|available|indexed)/i.test(trimmed))
  ) {
    return "The code map isn’t ready yet. Rebuild it, or wait for indexing to finish.";
  }
  if (
    /^workshop returned HTTP 404(\s+Not Found)?:?\s*$/i.test(trimmed) ||
    /^workshop returned HTTP 405(\s+Method Not Allowed)?:?\s*$/i.test(trimmed) ||
    (/HTTP\s+404\b/i.test(trimmed) &&
      /\/v1\/forge\/(repositories|items\/[^/]+\/(source|tree|workspace-state|tasks))/i.test(
        trimmed,
      ))
  ) {
    return "This workshop is older than Medousa’s project tools. Rebuild and restart medousa_daemon from this checkout, then try again.";
  }
  return trimmed
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
