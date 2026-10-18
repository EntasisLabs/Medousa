import { describe, expect, it } from "vitest";
import { activityTaskIds, activityDetailSummary, activityPath, buildWorkshopActivity, filterActivityTree, flattenActivityTree, type ActivitySources } from "./activity";
import type { WorkCard, WorkspaceEvent } from "$lib/types/workspace";
import type { WorkCardDetail } from "$lib/types/card";
import type { ItemProjection, ProjectTaskRunSummary } from "$lib/forge";
import type { RecurringDefinitionEntry } from "$lib/types/recurring";

export const card = (id: string, changes: Partial<WorkCard> = {}): WorkCard => ({ id, title: `Task ${id}`, column: "in_flight", status_label: "running", created_at_utc: "2026-10-17T10:00:00Z", updated_at_utc: "2026-10-17T11:00:00Z", ...changes });
export const detail = (id: string, changes: Partial<WorkCardDetail> = {}): WorkCardDetail => ({ card: card(id), kind: "turn_worker", terminal: false, wrapping_up_reasons: [], associations: { vault_paths: [], artifact_ids: [], locus_node_ids: [] }, ...changes });
const sources = (changes: Partial<ActivitySources> = {}): ActivitySources => ({ cards: [], details: new Map(), sessions: [], feed: [], projects: [], projectRuns: new Map(), schedules: [], scheduleRuns: new Map(), ...changes });

describe("workshop activity hierarchy", () => {
  it("does not keep a completed worker running because a tool finish event is missing", () => {
    const done = card("worker", { column: "done", status_label: "completed" });
    const tree = buildWorkshopActivity(sources({ cards: [done], details: new Map([["worker", detail("worker", { terminal: true, session_id: "session", live_tool_activity: [{ run_id: "orphan", name: "llm.chat", status: "running", round: 1, started_at: "2026-10-17T10:00:00Z" }] })]]) }));
    expect(tree[0].state).toBe("finished"); expect(filterActivityTree(tree, "running", "")).toEqual([]);
    expect(tree[0].children[0].children[0].status).toBe("unfinished when worker ended");
  });
  it("groups delegated workers by recorded conversation and turn, then tool run", () => {
    const input = sources({ cards: [card("a"), card("b")], details: new Map([
      ["a", detail("a", { session_id: "session", correlation_id: "turn", live_tool_activity: [{ run_id: "tool", name: "shell", round: 1, status: "running", started_at: "2026-10-17T11:00:00Z" }] })],
      ["b", detail("b", { session_id: "session", correlation_id: "turn" })],
    ]) });
    const tree = buildWorkshopActivity(input);
    expect(tree).toHaveLength(1);
    expect(tree[0].id).toBe("session:session");
    const turn = tree[0].children[0];
    expect(turn.kind).toBe("Turn");
    expect(turn.children.map((node) => node.id)).toEqual(["card:a", "card:b"]);
    expect(turn.children[0].children[0].id).toBe("tool:a:tool");
  });

  it("attaches explicit parent cards and leaves unrelated matching titles separate", () => {
    const tree = buildWorkshopActivity(sources({ cards: [card("parent"), card("child"), card("other", { title: "Task parent" })], details: new Map([["child", detail("child", { correlation_id: "parent" })]]) }));
    expect(tree).toHaveLength(2);
    expect(tree.find((node) => node.id === "card:parent")?.children[0].id).toBe("card:child");
    expect(tree.some((node) => node.id === "card:other")).toBe(true);
  });

  it("breaks cyclic and self-referential parent records without dropping work", () => {
    const tree = buildWorkshopActivity(sources({ cards: [card("a"), card("b"), card("self")], details: new Map([
      ["a", detail("a", { correlation_id: "b" })], ["b", detail("b", { correlation_id: "a" })], ["self", detail("self", { correlation_id: "self" })],
    ]) }));
    expect(flattenActivityTree(tree, new Set(), true)).toHaveLength(3);
  });

  it("shows a scheduled job once and attaches its worker beneath the exact run", () => {
    const schedule = { recurring_id: "schedule", display_name: "Morning brief", last_run_at_utc: "2026-10-17T11:00:00Z", last_run_status: "running" } as RecurringDefinitionEntry;
    const tree = buildWorkshopActivity(sources({ cards: [card("job"), card("worker")], details: new Map([
      ["job", detail("job", { job_id: "job" })], ["worker", detail("worker", { correlation_id: "job", session_id: "session" })],
    ]), schedules: [schedule], scheduleRuns: new Map([["schedule", [{ job_id: "job", status: "running", is_terminal: false, attempt_count: 2, scheduled_at_utc: "2026-10-17T11:00:00Z", updated_at_utc: "2026-10-17T11:00:00Z" }]]]) }));
    expect(tree).toHaveLength(1);
    expect(tree[0].children[0].id).toBe("job:job");
    expect(tree[0].children[0].children[0].id).toBe("card:worker");
  });

  it("uses the existing attention and finished filters for task actions", () => {
    const cards = [card("fail", { column: "blocked", status_label: "failed" }), card("stop", { column: "blocked", status_label: "canceled" }), card("done", { column: "done", status_label: "succeeded" }), card("stuck", { column: "blocked", status_label: "blocked" })];
    const tree = buildWorkshopActivity(sources({ cards, details: new Map(cards.map((entry) => [entry.id, detail(entry.id, { session_id: "session", terminal: entry.id !== "stuck" })])) }));
    expect([...activityTaskIds(filterActivityTree(tree, "attention", ""))].sort()).toEqual(["fail", "stuck"]);
    expect([...activityTaskIds(filterActivityTree(tree, "finished", ""))].sort()).toEqual(["done", "fail", "stop"]);
    expect([...activityTaskIds(filterActivityTree(tree, "attention", "no match"))]).toEqual([]);
  });

  it("does not include a context-only ancestor in a searched bulk action", () => {
    const cards = [card("parent", { column: "blocked", status_label: "failed" }), card("needle", { column: "blocked", status_label: "failed" })];
    const tree = buildWorkshopActivity(sources({ cards, details: new Map([["needle", detail("needle", { correlation_id: "parent" })]]) }));
    const filtered = filterActivityTree(tree, "attention", "needle");
    expect(filtered[0].id).toBe("card:parent");
    expect([...activityTaskIds(filtered)]).toEqual(["needle"]);
  });

  it("keeps actual Code attempts and runs under their project without inventing attempt ownership", () => {
    const project = { id: "project", title: "Browser", state: "ready", human_phase: "work", attempts: [{ id: "one", seq: 1, state: "failed" }, { id: "two", seq: 2, state: "running" }] } as ItemProjection;
    const run = { run_id: "build", work_id: "project", state: "running", task: { label: "Build", argv: ["npm", "run", "build"] }, started_at: "2026-10-17T11:00:00Z", terminal: false } as ProjectTaskRunSummary;
    const root = buildWorkshopActivity(sources({ projects: [project], projectRuns: new Map([["project", [run]]]) }))[0];
    expect(root.children).toHaveLength(3);
    expect(root.children.every((child) => child.children.length === 0)).toBe(true);
    expect(root.state).toBe("running");
    expect(activityPath([root], "code-run:project:build")?.map((node) => node.id)).toEqual(["project:project", "code-run:project:build"]);
  });

  it("retains ancestors when filtering a failed child and includes terminal failures in Finished", () => {
    const tree = buildWorkshopActivity(sources({ cards: [card("fail", { column: "blocked", status_label: "failed" }), card("ok", { column: "done", status_label: "succeeded" })], details: new Map([
      ["fail", detail("fail", { session_id: "session", terminal: true })], ["ok", detail("ok", { session_id: "session", terminal: true })],
    ]) }));
    const filtered = filterActivityTree(tree, "attention", "fail");
    expect(filtered[0].id).toBe("session:session");
    expect(filtered[0].children.map((node) => node.id)).toEqual(["card:fail"]);
    expect(filterActivityTree(tree, "finished", "")[0].children).toHaveLength(2);
  });

  it("does not let an older failed attempt turn a completed project into a live blocker", () => {
    const project = { id: "project", title: "Done", state: "accepted", human_phase: "complete", attempts: [{ id: "old", seq: 1, state: "failed" }] } as ItemProjection;
    expect(buildWorkshopActivity(sources({ projects: [project] }))[0].state).toBe("finished");
  });

  it.each([["ready", "idle"], ["executing", "running"], ["provisioning", "running"], ["sealing", "running"], ["applying_decision", "running"], ["accepted", "finished"]] as const)("classifies Forge %s independently of job statuses", (state, expected) => {
    const project: ItemProjection = { id: "project", title: "Code", brief: "", owner: "user", state, human_phase: "work", attempts: [], allowed_actions: {
      provision: { allowed: false }, start_agent: { allowed: false }, open_terminal: { allowed: false }, begin_attempt: { allowed: false }, seal: { allowed: false }, review: { allowed: false }, apply: { allowed: false }, discard: { allowed: false },
    } };
    expect(buildWorkshopActivity(sources({ projects: [project] }))[0].state).toBe(expected);
  });

  it("collapses lifecycle receipts and keeps activity after a card expires", () => {
    const event = (id: string, kind: string): WorkspaceEvent => ({ id, kind, actor: "agent", timestamp_utc: id === "2" ? "2026-10-17T12:00:00Z" : "2026-10-17T11:00:00Z", summary: "Build", refs: [{ ref_type: "card", ref_id: "expired" }] });
    const tree = buildWorkshopActivity(sources({ feed: [event("1", "job_started"), event("2", "job_succeeded")] }));
    expect(tree).toHaveLength(1);
    expect(tree[0]).toMatchObject({ id: "card:expired", state: "finished", target: { kind: "card", cardId: "expired" } });
  });

  it("keeps rows available before optional detail enrichment arrives", () => {
    const tree = buildWorkshopActivity(sources({ cards: [card("unknown")] }));
    expect(tree[0]).toMatchObject({ id: "card:unknown", state: "running" });
    expect(flattenActivityTree(tree, new Set())).toHaveLength(1);
  });

  it("retains bounded tool summaries without duplicating reasoning and output buffers", () => {
    const summary = activityDetailSummary(detail("worker", { live_output: "x".repeat(100000), live_thinking: "y".repeat(100000), live_tool_activity: Array.from({ length: 80 }, (_, i) => ({ run_id: String(i), name: "shell", status: "succeeded", round: 1, started_at: "", output_summary: "x".repeat(1000) })) }));
    expect(summary.live_output).toBe(""); expect(summary.live_thinking).toBe("");
    expect(summary.live_tool_activity).toHaveLength(40);
    expect(summary.live_tool_activity![0].run_id).toBe("40");
    expect(summary.live_tool_activity![0].output_summary).toHaveLength(500);
  });
});


describe("reusable tool activity", () => {
  const tool = { entry_id: "saved", session_id: "session", slice_id: "turn:1", turn_index: 1, tool_round: 2, run_id: "tool", tool_name: "shell", status: "succeeded", input_summary: "git status", sanitized_input: {}, args_hash: "hash", redacted: false, timestamp: "2026-10-17T11:00:00Z" };
  it("places history under the source conversation with an exact replay reference", () => {
    const tree = buildWorkshopActivity(sources({ toolRuns: [tool] }));
    expect(tree[0].id).toBe("session:session");
    expect(tree[0].children[0]).toMatchObject({ id: "history:saved", kind: "Tool run", replayRef: { session_id: "session", slice_id: "turn:1", run_id: "tool", tool_round: 2 } });
  });
  it("enriches a live tool instead of listing it twice", () => {
    const tree = buildWorkshopActivity(sources({ cards: [card("worker")], details: new Map([["worker", detail("worker", { session_id: "session", live_tool_activity: [{ run_id: "tool", name: "shell", status: "succeeded", round: 2, started_at: tool.timestamp }] })]]), toolRuns: [tool] }));
    const tools = flattenActivityTree(tree, new Set(), true).filter(({ node }) => node.kind === "Tool run");
    expect(tools).toHaveLength(1); expect(tools[0].node.replayRef?.run_id).toBe("tool");
  });
  it("keeps historical tool failures out of the actionable attention list", () => {
    const tree = buildWorkshopActivity(sources({ toolRuns: [{ ...tool, status: "failed" }] }));
    expect(tree[0].children[0]).toMatchObject({ state: "finished", status: "failed" });
    expect(filterActivityTree(tree, "attention", "")).toEqual([]);
  });
  it("keeps redacted activity visible without offering replay", () => {
    const tree = buildWorkshopActivity(sources({ toolRuns: [{ ...tool, redacted: true }] }));
    expect(tree[0].children[0].replayRef).toBeUndefined();
  });
});
