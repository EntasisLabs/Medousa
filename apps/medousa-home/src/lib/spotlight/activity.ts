import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard, WorkspaceEvent } from "$lib/types/workspace";
import type { ItemProjection, ProjectTaskRunSummary } from "$lib/forge";
import type { RecurringDefinitionEntry, RecurringRunEntry } from "$lib/types/recurring";
import type { SessionSummary } from "$lib/types/session";
import { formatSessionLabel } from "$lib/utils/formatSession";
import { formatCardTitle } from "$lib/utils/formatWork";
import { formatToolName } from "$lib/utils/formatTurn";

export type ActivityState = "running" | "attention" | "finished" | "idle";
export type ActivityFilter = "all" | "running" | "attention" | "finished";
export type ActivityTarget =
  | { kind: "chat"; sessionId: string }
  | { kind: "card"; cardId: string; jobId?: string; sessionId?: string }
  | { kind: "project"; workId: string; review?: boolean }
  | { kind: "code-run"; workId: string; runId: string }
  | { kind: "schedule"; recurringId: string }
  | { kind: "job"; jobId: string }
  | { kind: "note"; path: string };

export interface ActivityNode {
  id: string;
  title: string;
  kind: string;
  state: ActivityState;
  status: string;
  terminal?: boolean;
  updatedAt: string;
  summary?: string;
  error?: string;
  target?: ActivityTarget;
  children: ActivityNode[];
}

export interface ActivitySources {
  cards: WorkCard[];
  details: ReadonlyMap<string, WorkCardDetail>;
  sessions: SessionSummary[];
  feed: WorkspaceEvent[];
  projects: ItemProjection[];
  projectRuns: ReadonlyMap<string, ProjectTaskRunSummary[]>;
  schedules: RecurringDefinitionEntry[];
  scheduleRuns: ReadonlyMap<string, RecurringRunEntry[]>;
  turns?: Array<{ id: string; sessionId: string; phase: string; terminal: boolean; title: string; updatedAt: string }>;
}

const running = new Set(["running", "ready", "active", "working", "leased", "enqueued", "queued", "accepted", "in_flight", "wrapping_up", "backlog", "streaming"]);
const finished = new Set(["done", "complete", "completed", "succeeded", "passed", "cancelled", "canceled", "discarded", "applied", "sealed", "satisfied"]);
const attention = new Set(["blocked", "failed", "error", "needs_attention", "needs approval", "review", "dead_letter", "budget_blocked"]);

export function activityState(status: string, terminal = false): ActivityState {
  const value = status.trim().toLowerCase();
  if (attention.has(value)) return "attention";
  if (finished.has(value) || terminal) return "finished";
  return running.has(value) ? "running" : "idle";
}

const rank: Record<ActivityState, number> = { running: 0, attention: 1, idle: 2, finished: 3 };
const time = (value: string) => Date.parse(value) || 0;
const label = (value: string) => value.replaceAll("_", " ");

function rollUp(node: ActivityNode, container = false): ActivityNode {
  for (const child of node.children) rollUp(child, child.kind === "Conversation" || child.kind === "Turn");
  node.children.sort((a, b) => rank[a.state] - rank[b.state] || time(b.updatedAt) - time(a.updatedAt) || a.id.localeCompare(b.id));
  const states = node.children.map((child) => child.state);
  const liveAttention = node.children.some((child) => child.state === "attention" && !child.terminal);
  if (!node.terminal) {
    if (node.state === "attention" || liveAttention) node.state = "attention";
    else if (states.includes("running")) node.state = "running";
    else if (container && states.length > 0) node.state = states.includes("attention") ? "attention" : states.every((state) => state === "finished") ? "finished" : "idle";
  }
  if (container) node.status = node.state === "attention" ? "Needs you" : node.state === "running" ? "Running" : node.state === "finished" ? "Finished" : "Activity";
  for (const child of node.children) {
    if (time(child.updatedAt) > time(node.updatedAt)) node.updatedAt = child.updatedAt;
  }
  return node;
}

/** Own a small summary, never another copy of every worker transcript. */
export function activityDetailSummary(detail: WorkCardDetail): WorkCardDetail {
  return {
    ...detail,
    task_line: detail.task_line?.slice(0, 500),
    result_excerpt: detail.result_excerpt?.slice(0, 1000),
    error: detail.error?.slice(0, 2000),
    live_status_line: detail.live_status_line?.slice(0, 300),
    live_output: "",
    live_thinking: "",
    live_tool_activity: detail.live_tool_activity?.slice(-40).map((tool) => ({
      ...tool, input_params: [], input_summary: tool.input_summary?.slice(0, 300), output_summary: tool.output_summary?.slice(0, 500),
    })),
  };
}

export function buildWorkshopActivity(source: ActivitySources): ActivityNode[] {
  const roots: ActivityNode[] = [];
  const cardNodes = new Map<string, ActivityNode>();
  const conversationNodes = new Map<string, ActivityNode>();
  const turns = new Map<string, ActivityNode>();
  const sessions = new Map(source.sessions.map((session) => [session.session_id, session]));
  const consumedJobs = new Set<string>();
  const scheduledJobs = new Set([...source.scheduleRuns.values()].flatMap((runs) => runs.map((run) => run.job_id)));
  const conversation = (sessionId: string): ActivityNode => {
    let node = conversationNodes.get(sessionId);
    if (!node) {
      const session = sessions.get(sessionId);
      node = { id: `session:${sessionId}`, title: session ? formatSessionLabel(session) : "Background conversation", kind: "Conversation", state: "idle", status: "Activity", updatedAt: "", target: { kind: "chat", sessionId }, children: [] };
      conversationNodes.set(sessionId, node);
      roots.push(node);
    }
    return node;
  };

  for (const schedule of source.schedules) {
    const runs = source.scheduleRuns.get(schedule.recurring_id) ?? [];
    if (!schedule.last_run_at_utc && runs.length === 0) continue;
    roots.push({
      id: `schedule:${schedule.recurring_id}`, title: schedule.display_name || schedule.prompt_excerpt || "Scheduled activity", kind: "Schedule",
      state: activityState(schedule.last_run_status || "unknown"), status: label(schedule.last_run_status || "Recent activity"), updatedAt: schedule.last_run_at_utc || "",
      target: { kind: "schedule", recurringId: schedule.recurring_id }, children: runs.map((run): ActivityNode => {
        consumedJobs.add(run.job_id);
        return { id: `job:${run.job_id}`, title: `Run · ${new Date(run.scheduled_at_utc).toLocaleString()}`, kind: "Scheduled run",
          state: activityState(run.status, run.is_terminal), terminal: run.is_terminal, status: label(run.status), updatedAt: run.updated_at_utc,
          summary: run.latest_outcome || undefined, target: { kind: "job", jobId: run.job_id }, children: [] };
      }),
    });
  }
  const scheduledParents = new Map(roots.flatMap((root) => root.children.map((node) => [node.id.slice(4), node] as const)));
  for (const turn of source.turns ?? []) {
    const owner = conversation(turn.sessionId);
    const key = `${turn.sessionId}:${turn.id}`;
    const state = activityState(turn.phase, turn.terminal);
    const node: ActivityNode = { id: `turn:${key}`, title: turn.title || "Chat turn", kind: "Turn", state: state === "idle" && !turn.terminal ? "running" : state, terminal: turn.terminal, status: label(turn.phase), updatedAt: turn.updatedAt, target: owner.target, children: [] };
    turns.set(key, node); owner.children.push(node);
  }
  for (const card of source.cards) {
    const detail = source.details.get(card.id);
    if (scheduledJobs.has(detail?.job_id || card.id)) continue;
    const kind = detail?.kind === "turn_worker" ? "Agent" : detail?.kind === "turn_budget_request" ? "Request" : /grapheme|script/i.test(detail?.job_type ?? "") ? "Script run" : "Task";
    const status = card.status_label || card.column;
    const terminal = Boolean(detail?.terminal || card.column === "done" || finished.has(status));
    const state = activityState(status, terminal);
    const node: ActivityNode = {
      id: `card:${card.id}`, title: formatCardTitle(card), kind,
      state: state === "idle" ? activityState(card.column) : state,
      terminal,
      status: label(status), updatedAt: card.updated_at_utc,
      summary: detail?.live_status_line || detail?.result_excerpt || detail?.task_line || undefined,
      error: detail?.error || undefined,
      target: { kind: "card", cardId: card.id, jobId: detail?.job_id || undefined, sessionId: detail?.session_id || undefined },
      children: (detail?.live_tool_activity ?? []).slice(-40).map((tool) => ({
        id: `tool:${card.id}:${tool.run_id}`, title: formatToolName(tool.name), kind: "Tool run",
        state: terminal && tool.status === "running" ? "finished" : activityState(tool.status), terminal: terminal || tool.status !== "running", status: terminal && tool.status === "running" ? "unfinished when worker ended" : label(tool.status), updatedAt: tool.finished_at || tool.started_at,
        summary: tool.output_summary || tool.input_summary || undefined,
        target: { kind: "card", cardId: card.id, sessionId: detail?.session_id || undefined }, children: [],
      })),
    };
    cardNodes.set(card.id, node);
    consumedJobs.add(card.id);
    if (detail?.job_id) consumedJobs.add(detail.job_id);
  }
  for (const [cardId, node] of cardNodes) {
    const detail = source.details.get(cardId);
    const parentId = detail?.correlation_id?.trim();
    // A correlation is an explicit turn/parent link, never a title similarity.
    // Only attach to another card when doing so cannot introduce a cycle.
    let ancestor = parentId;
    const seen = new Set([cardId]);
    while (ancestor && !seen.has(ancestor)) {
      seen.add(ancestor);
      ancestor = source.details.get(ancestor)?.correlation_id?.trim();
    }
    const scheduledParent = parentId ? scheduledParents.get(parentId) : null;
    if (scheduledParent) { scheduledParent.children.push(node); continue; }
    const parent = parentId && !ancestor ? cardNodes.get(parentId) : null;
    if (parent && parent !== node) { parent.children.push(node); continue; }
    if (detail?.session_id) {
      const owner = conversation(detail.session_id);
      if (parentId && parentId !== cardId && !cardNodes.has(parentId)) {
        const key = `${detail.session_id}:${parentId}`;
        let turn = turns.get(key);
        if (!turn) {
          turn = { id: `turn:${key}`, title: "Delegated work", kind: "Turn", state: "idle", status: "Activity", updatedAt: "", target: owner.target, children: [] };
          turns.set(key, turn); owner.children.push(turn);
        }
        turn.children.push(node);
      } else owner.children.push(node);
    } else roots.push(node);
  }

  for (const project of source.projects) {
    const runs = source.projectRuns.get(project.id) ?? [];
    const attempts = project.attempts ?? [];
    // Forge's ready/accepted states mean different things from queued jobs.
    const terminal = ["accepted", "discarded", "failed"].includes(project.state) || project.human_phase === "complete";
    const state: ActivityState = project.human_phase === "review" || project.human_phase === "needs_attention" || project.state === "failed" ? "attention"
      : terminal ? "finished" : ["provisioning", "executing", "sealing", "applying_decision"].includes(project.state) ? "running" : "idle";
    roots.push({
      id: `project:${project.id}`, title: project.title, kind: "Code project", state, terminal, status: label(project.human_phase),
      updatedAt: project.updated_at || project.created_at || "", summary: project.brief,
      target: { kind: "project", workId: project.id, review: project.human_phase === "review" },
      children: [
        ...attempts.map((attempt): ActivityNode => ({
          id: `attempt:${project.id}:${attempt.id}`, title: `Attempt ${attempt.seq}`, kind: "Code attempt",
          state: activityState(attempt.state, attempt.state !== "running"), terminal: attempt.state !== "running", status: label(attempt.state), updatedAt: "",
          summary: attempt.executor?.kind ? `Executor: ${attempt.executor.kind}` : undefined,
          target: { kind: "project", workId: project.id, review: project.human_phase === "review" }, children: [],
        })),
        ...runs.map((run): ActivityNode => ({
          id: `code-run:${project.id}:${run.run_id}`, title: run.task.label, kind: "Code run",
          state: activityState(run.state, run.terminal), terminal: run.terminal, status: label(run.state), updatedAt: run.finished_at || run.started_at,
          summary: run.task.argv.join(" "), target: { kind: "code-run", workId: project.id, runId: run.run_id }, children: [],
        })),
      ],
    });
  }
  // Surface workshop activity without a card too (notes, scripts and run events).
  // Lifecycle receipts with the same resource belong to a single entry.
  const eventNodes = new Map<string, ActivityNode>();
  for (const event of [...source.feed].sort((a, b) => time(a.timestamp_utc) - time(b.timestamp_utc))) {
    const ref = (kind: string) => event.refs.find((entry) => entry.ref_type === kind)?.ref_id;
    if (ref("card") && consumedJobs.has(ref("card")!)) continue;
    const cardId = ref("card");
    const jobId = ref("job") || cardId;
    if (jobId && consumedJobs.has(jobId)) continue;
    const path = ref("vault_path");
    const sessionId = ref("session");
    if (!jobId && !path && !sessionId) continue;
    const id = cardId ? `card:${cardId}` : jobId ? `job:${jobId}` : path ? `note:${path}` : `event:${event.id}`;
    const state = /failed|error/.test(event.kind) ? "attention" : /completed|succeeded|updated|created|replied/.test(event.kind) ? "finished" : /started|enqueued|delegated/.test(event.kind) ? "running" : "idle";
    eventNodes.set(id, { id, title: event.detail_line || event.summary, kind: path ? "Note activity" : jobId ? "Run" : "Conversation activity", state, status: label(event.kind), updatedAt: event.timestamp_utc,
      summary: event.context_line || event.summary, target: cardId ? { kind: "card", cardId } : jobId ? { kind: "job", jobId } : path ? { kind: "note", path } : { kind: "chat", sessionId: sessionId! }, children: [] });
  }
  roots.push(...eventNodes.values());
  roots.forEach((node) => rollUp(node, node.kind === "Conversation" || node.kind === "Turn"));
  return roots.sort((a, b) => rank[a.state] - rank[b.state] || time(b.updatedAt) - time(a.updatedAt) || a.id.localeCompare(b.id));
}

/** Preserve the ancestry of a matching child; filters never orphan a failed run. */
export function filterActivityTree(nodes: ActivityNode[], filter: ActivityFilter, query: string): ActivityNode[] {
  const needle = query.trim().toLowerCase();
  const visit = (node: ActivityNode, ancestorMatches = false): ActivityNode | null => {
    const haystack = `${node.title} ${node.kind} ${node.status} ${node.summary ?? ""}`.toLowerCase();
    const matches = ancestorMatches || haystack.includes(needle);
    const children = node.children.map((child) => visit(child, matches)).filter((child): child is ActivityNode => child !== null);
    if ((matches && (filter === "all" || node.state === filter || (filter === "finished" && node.terminal))) || children.length > 0) return { ...node, children };
    return null;
  };
  return nodes.map((node) => visit(node)).filter((node): node is ActivityNode => node !== null);
}

export function flattenActivityTree(nodes: ActivityNode[], expanded: ReadonlySet<string>, searching = false): Array<{ node: ActivityNode; depth: number }> {
  const rows: Array<{ node: ActivityNode; depth: number }> = [];
  const visit = (node: ActivityNode, depth: number) => {
    rows.push({ node, depth });
    if (searching || expanded.has(node.id)) node.children.forEach((child) => visit(child, depth + 1));
  };
  nodes.forEach((node) => visit(node, 0));
  return rows;
}

export function activityPath(nodes: ActivityNode[], id: string): ActivityNode[] | null {
  for (const node of nodes) {
    if (node.id === id) return [node];
    const child = activityPath(node.children, id);
    if (child) return [node, ...child];
  }
  return null;
}
