/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
const mocks = vi.hoisted(() => ({ api: { getGraphemeScript: vi.fn(), getJobResult: vi.fn(), getWorkflow: vi.fn(), listGraphemeScripts: vi.fn(), listWorkflowRuns: vi.fn(), listWorkflows: vi.fn(), listRecurringRuns: vi.fn(), runGraphemeSource: vi.fn(), runWorkflow: vi.fn() }, scope: vi.fn(() => "local"), editor: vi.fn(), sidebar: vi.fn(), create: vi.fn(), activity: vi.fn(), pause: vi.fn() }));
vi.mock("$lib/daemon", () => mocks.api);
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: mocks.scope }));
vi.mock("$lib/stores/lmeWorkspace.svelte", () => ({ lmeWorkspace: { openScriptById: mocks.editor, openFlow: mocks.editor, openSchedule: mocks.editor, setExplorerMode: mocks.editor } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { openShellSidebarView: mocks.sidebar } }));
vi.mock("$lib/stores/commandSpotlight.svelte", () => ({ commandSpotlight: { openActivity: mocks.activity } }));
vi.mock("$lib/stores/automationDraft.svelte", () => ({ automationDraft: { openCreate: mocks.create } }));
vi.mock("./ScheduleCreatePopover.svelte", async () => import("./AutomationsRunList.testfixture.svelte"));
vi.mock("$lib/stores/automations.svelte", () => ({ automations: {
  definitions: [{ recurring_id: "prompt", display_name: "Daily brief", prompt_excerpt: "Truncated prompt…", enabled: true, cron_expr: "0 0 9 * * * *", timezone: "UTC", last_run_at_utc: null, next_run_at_utc: "2026-10-20T09:00:00Z" }],
  refresh: vi.fn(async () => {}), setEnabled: mocks.pause, formatTimestamp: (value: string) => value, statusLabel: (value: string) => value, deliveryLabelFor: () => "In Medousa",
} }));
import AutomationsRunList from "./AutomationsRunList.svelte";
let component: ReturnType<typeof mount> | undefined;
const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.trim() === text)!;
function selectScript() { [...document.querySelectorAll<HTMLButtonElement>(".automation-row")].find((button) => button.textContent?.includes("Backup"))!.click(); flushSync(); }
async function ready() { await vi.waitFor(() => { flushSync(); expect(document.body.textContent).toContain("Backup"); }); }
beforeEach(() => {
  vi.clearAllMocks(); mocks.scope.mockReturnValue("local");
  mocks.api.listGraphemeScripts.mockResolvedValue({ scripts: [{ id: "backup", name: "Backup", tags: [] }] });
  mocks.api.listWorkflows.mockResolvedValue({ workflows: [] }); mocks.api.listRecurringRuns.mockResolvedValue({ runs: [] });
  mocks.api.getGraphemeScript.mockResolvedValue({ body_preview: "full source", body_truncated: false });
  mocks.api.runGraphemeSource.mockResolvedValue({ result: { job_id: "queued" } });
  component = mount(AutomationsRunList, { target: document.body }); flushSync();
});
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });
it("opens with the run list and filters without losing selection controls", async () => {
  await ready(); expect(document.querySelectorAll(".automation-row")).toHaveLength(2);
  button("Scheduled").click(); flushSync(); expect(document.querySelectorAll(".automation-row")).toHaveLength(1);
  expect(button("Pause")).toBeTruthy(); expect(button("Run now")).toBeUndefined();
  button("Pause").click(); await vi.waitFor(() => expect(mocks.pause).toHaveBeenCalledWith("prompt", false));
  expect(mocks.api.runGraphemeSource).not.toHaveBeenCalled();
});
it("loads the full script before running and reports enqueue rather than completion", async () => {
  await ready(); selectScript(); button("Run now").click();
  await vi.waitFor(() => expect(mocks.api.runGraphemeSource).toHaveBeenCalledExactlyOnceWith("full source"));
  flushSync(); expect(document.body.textContent).toContain("Script enqueued");
});
it("does not execute shortened source", async () => {
  await ready(); selectScript(); mocks.api.getGraphemeScript.mockResolvedValue({ body_preview: "partial", body_truncated: true });
  button("Run now").click(); await vi.waitFor(() => { flushSync(); expect(document.body.textContent).toContain("full source"); });
  expect(mocks.api.runGraphemeSource).not.toHaveBeenCalled();
});
it("drops a source reply after the connected workshop changes", async () => {
  await ready(); selectScript(); let resolve!: (value: unknown) => void;
  mocks.api.getGraphemeScript.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
  button("Run now").click(); mocks.scope.mockReturnValue("remote"); resolve({ body_preview: "wrong workshop", body_truncated: false });
  await Promise.resolve(); await Promise.resolve(); expect(mocks.api.runGraphemeSource).not.toHaveBeenCalled();
});
it("keeps script resources and prompt creation reachable", async () => {
  await ready(); button("New automation").click(); flushSync();
  [...document.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent?.startsWith("Quick prompt"))!.click();
  expect(mocks.create).toHaveBeenCalledOnce();
  button("Scripts").click(); expect(mocks.editor).toHaveBeenCalledWith("scripts"); expect(mocks.sidebar).toHaveBeenCalledWith("automations");
});
