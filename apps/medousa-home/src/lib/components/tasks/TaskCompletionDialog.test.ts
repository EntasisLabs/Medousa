/** @vitest-environment happy-dom */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
const fixture = vi.hoisted(() => ({ jobId: "a", scope: "one", complete: vi.fn(), archive: vi.fn(), config: vi.fn(), clear: vi.fn(), openTask: vi.fn(), openNote: vi.fn(), push: vi.fn(), pop: vi.fn() }));
vi.mock("$lib/daemon", () => ({ completeAskJobActions: fixture.complete, archiveAskJob: fixture.archive }));
vi.mock("$lib/messaging", () => ({ loadProductConfigSummary: fixture.config }));
vi.mock("$lib/runtime/shellUseCases", () => ({ openTaskDetails: fixture.openTask, openVaultNote: fixture.openNote }));
vi.mock("$lib/stores/workspace.svelte", () => ({ workspace: { get pendingAskCompletion() { return { jobId: fixture.jobId }; }, clearPendingAskCompletion: fixture.clear } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { isMobile: false } }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => fixture.scope }));
vi.mock("$lib/utils/browserPopoverOverlay", () => ({ pushBrowserPopoverOverlay: fixture.push, popBrowserPopoverOverlay: fixture.pop }));
import TaskCompletionDialog from "./TaskCompletionDialog.svelte";
let component: ReturnType<typeof mount> | null = null;
const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>("button")].find((node) => node.textContent?.trim() === label)!;
async function settle() { await new Promise((done) => setTimeout(done, 0)); flushSync(); }
async function start() { component = mount(TaskCompletionDialog, { target: document.body, props: { pending: { jobId: "a", title: "Research" } } }); flushSync(); await settle(); }
beforeEach(() => {
  vi.clearAllMocks(); fixture.jobId = "a"; fixture.scope = "one";
  fixture.push.mockResolvedValue(undefined); fixture.pop.mockResolvedValue(undefined); fixture.config.mockRejectedValue(new Error("unavailable"));
  fixture.complete.mockResolvedValue({ ok: true, journal_path: "journal/today.md" }); fixture.archive.mockResolvedValue({ archived: true });
  vi.spyOn(HTMLDialogElement.prototype, "showModal").mockImplementation(function (this: HTMLDialogElement) { this.open = true; });
  vi.spyOn(HTMLDialogElement.prototype, "close").mockImplementation(function (this: HTMLDialogElement) { this.open = false; });
});
afterEach(async () => { if (component) await unmount(component); component = null; document.body.replaceChildren(); vi.restoreAllMocks(); });

describe("shared task completion", () => {
  it("saves and archives the captured job before advancing completion and opening its journal", async () => {
    await start(); button("Save & clear").click(); await settle();
    expect(fixture.complete).toHaveBeenCalledWith("a", expect.objectContaining({ writeJournalPath: expect.stringMatching(/^journal\//) }));
    expect(fixture.archive).toHaveBeenCalledWith("a", true); expect(fixture.clear).toHaveBeenCalledWith("a");
    expect(fixture.openNote).toHaveBeenCalledWith("journal/today.md");
  });
  it("opens the result without applying journal or messaging actions", async () => {
    await start(); button("View task").click(); await settle();
    expect(fixture.openTask).toHaveBeenCalledWith("a"); expect(fixture.clear).toHaveBeenCalledWith("a");
    expect(fixture.complete).not.toHaveBeenCalled(); expect(fixture.archive).not.toHaveBeenCalled();
  });
  it("keeps completion available when the server rejects saving", async () => {
    fixture.complete.mockResolvedValue({ ok: false, message: "Journal is unavailable" });
    await start(); button("Save & clear").click(); await settle();
    expect(document.body.textContent).toContain("Journal is unavailable"); expect(fixture.clear).not.toHaveBeenCalled(); expect(fixture.archive).not.toHaveBeenCalled();
  });
  it("never archives or navigates after the workshop changes during a save", async () => {
    let resolve!: (value: unknown) => void; fixture.complete.mockImplementation(() => new Promise((done) => { resolve = done; }));
    await start(); button("Save & clear").click(); flushSync(); fixture.scope = "two";
    resolve({ ok: true, journal_path: "old/workshop.md" }); await settle();
    expect(fixture.archive).not.toHaveBeenCalled(); expect(fixture.clear).not.toHaveBeenCalled(); expect(fixture.openNote).not.toHaveBeenCalled();
  });
  it("releases native browser occlusion on close and remains usable without delivery config", async () => {
    await start(); expect(document.body.textContent).toContain("Delivery options are unavailable"); expect(button("Save").disabled).toBe(false);
    expect(fixture.push).toHaveBeenCalledOnce();
    await unmount(component!); component = null; expect(fixture.pop).toHaveBeenCalledOnce();
  });
});
