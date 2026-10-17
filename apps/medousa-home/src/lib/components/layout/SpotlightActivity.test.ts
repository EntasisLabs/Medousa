/** @vitest-environment happy-dom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { WorkshopCommandContext } from "$lib/commands/types";

const fixture = vi.hoisted(() => {
  const card = (id: string, status = "running") => ({ id, title: `Task ${id}`, column: status === "failed" ? "blocked" : "in_flight", status_label: status, created_at_utc: "2026-10-17T10:00:00Z", updated_at_utc: "2026-10-17T11:00:00Z" });
  const a = card("a"); const b = card("b", "failed");
  const detail = (entry: typeof a) => ({ card: entry, kind: "turn_worker", session_id: "session", correlation_id: "turn", terminal: entry.id === "b", wrapping_up_reasons: [], task_line: entry.title, associations: { vault_paths: [], artifact_ids: [], locus_node_ids: [] } });
  return { cards: [a, b], details: new Map([["a", detail(a)], ["b", detail(b)]]), card: vi.fn(), projects: vi.fn(), schedules: vi.fn(), navigate: vi.fn(), close: vi.fn(), switchSession: vi.fn() };
});
vi.mock("$lib/stores/workspace.svelte", () => ({ workspace: { get cards() { return fixture.cards; }, visibleCards: () => fixture.cards, get cardDetailsCache() { return fixture.details; }, feed: [] } }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { sessions: [{ session_id: "session", display_name: "Browser work", preview: "", turns: 2, verification_runs: 0 }], sessionRuntimes: new Map(), turns: new Map(), messages: [], sessionId: "", runtimeRevision: 0 } }));
vi.mock("$lib/stores/connection.svelte", () => ({ connection: { offline: false } }));
vi.mock("$lib/work/workerTranscripts.svelte", () => ({ workerTranscripts: { transcripts: new Map() } }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => "workshop" }));
vi.mock("$lib/spotlight/activityApi", () => ({ activityDataPort: { projects: fixture.projects, schedules: fixture.schedules, projectRuns: async () => [], scheduleRuns: async () => [], card: fixture.card }, readActivityCodeRun: vi.fn() }));
vi.mock("$lib/daemon", () => ({ getWorkspaceCard: fixture.card, getJobResult: vi.fn() }));
import SpotlightActivity from "./SpotlightActivity.svelte";

let component: ReturnType<typeof mount> | null = null;
const ctx = () => ({ chat: { switchSession: fixture.switchSession }, navigate: fixture.navigate, callbacks: { close: fixture.close } }) as unknown as WorkshopCommandContext;
const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((entry) => entry.textContent?.trim() === text)!;
const tree = () => document.querySelector('[role="tree"]')!;
async function start(query = "") { component = mount(SpotlightActivity, { target: document.body, props: { query, ctx: ctx() } }); flushSync(); await vi.advanceTimersByTimeAsync(200); flushSync(); }
beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks();
  fixture.projects.mockResolvedValue({ items: [], nextCursor: null }); fixture.schedules.mockResolvedValue([]);
  fixture.switchSession.mockResolvedValue(undefined);
  fixture.card.mockImplementation(async (id: string) => fixture.details.get(id));
});
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); vi.useRealTimers(); });

describe("Spotlight Activity interactions", () => {
  it("expands conversation → turn → tasks and previews before navigation", async () => {
    await start();
    expect(tree().textContent).toContain("Browser work"); expect(tree().textContent).not.toContain("Task a");
    document.querySelector<HTMLButtonElement>('[aria-label="Expand Browser work"]')!.click(); flushSync();
    document.querySelector<HTMLButtonElement>('[aria-label="Expand Delegated work"]')!.click(); flushSync();
    expect(tree().textContent).toContain("Task a"); expect(tree().textContent).toContain("Task b");
    const row = [...document.querySelectorAll<HTMLElement>('[role="treeitem"]')].find((entry) => entry.textContent?.includes("Task a"))!;
    row.click(); flushSync(); await vi.advanceTimersByTimeAsync(200); flushSync();
    expect(document.querySelector('[aria-label="Activity preview"]')?.textContent).toContain("Task a");
    expect(fixture.navigate).not.toHaveBeenCalled();
    button("Open chat").click(); await vi.advanceTimersByTimeAsync(0); flushSync();
    expect(fixture.switchSession).toHaveBeenCalledWith("session"); expect(fixture.navigate).toHaveBeenCalledWith("chat");
  });
  it("keeps parent context while searching and filters running tasks", async () => {
    await start("Task a");
    expect(tree().textContent).toContain("Browser work"); expect(tree().textContent).toContain("Task a"); expect(tree().textContent).not.toContain("Task b");
    button("Running").click(); flushSync(); expect(tree().textContent).toContain("Task a");
  });
  it("stops summary refreshes when the lens is unmounted", async () => {
    await start(); expect(fixture.projects).toHaveBeenCalledOnce();
    await unmount(component!); component = null;
    await vi.advanceTimersByTimeAsync(60000);
    expect(fixture.projects).toHaveBeenCalledOnce();
  });
  it("ignores a late selected task output after another row is selected", async () => {
    let resolve!: (value: unknown) => void;
    fixture.card.mockImplementation(() => new Promise((done) => { resolve = done; }));
    await start("Task");
    const rows = [...document.querySelectorAll<HTMLElement>('[role="treeitem"]')];
    rows.find((entry) => entry.textContent?.includes("Task a"))!.click(); flushSync(); await vi.advanceTimersByTimeAsync(200);
    rows.find((entry) => entry.textContent?.includes("Task b"))!.click(); flushSync();
    resolve({ live_output: "LATE OUTPUT FROM A" }); await Promise.resolve(); flushSync();
    expect(document.body.textContent).not.toContain("LATE OUTPUT FROM A");
  });
});
