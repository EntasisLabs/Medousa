/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import { invalidateCodeWorkshopContext } from "$lib/code/codeWorkspaceContext.svelte";
const api = vi.hoisted(() => ({ replace: vi.fn(), begin: vi.fn(), activate: vi.fn() }));
vi.mock("$lib/forge", () => ({
  canStartHumanEditing: () => true, startHumanEditingSession: api.begin,
  humanizeForgeMessage: (message: string) => message,
  replaceUndertakingSource: api.replace, searchUndertakingSource: vi.fn(),
}));
vi.mock("$lib/stores/undertakings.svelte", () => ({ undertakings: {
  active: { workId: "work", leaseId: null, leaseGeneration: null },
  detail: { id: "work", allowed_actions: {} }, setActiveFromItem: api.activate,
} }));
import CodeWorkspaceSearch from "./CodeWorkspaceSearch.svelte";

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
});

it("does not apply a replacement when editing control arrives after a workshop switch", async () => {
  api.replace.mockResolvedValueOnce({ files: [{
    path: "app.ts", before: "old", after: "new", expected_digest: "digest", match_count: 1,
  }] });
  let resolve!: (value: unknown) => void;
  api.begin.mockReturnValueOnce(new Promise((done) => { resolve = done; }));
  const target = document.createElement("div");
  document.body.append(target);
  component = mount(CodeWorkspaceSearch, { target, props: { workId: "work", workspaceScope: "workshop-a/work" } });
  flushSync();
  const input = target.querySelector<HTMLInputElement>('input[placeholder="Search in project…"]')!;
  input.value = "old";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  const button = (label: string) => Array.from(document.querySelectorAll("button"))
    .find((element) => element.textContent?.trim() === label)!;
  button("Replace…").click();
  await vi.waitFor(() => expect(button("Apply replace")).toBeTruthy());
  button("Apply replace").click();
  await vi.waitFor(() => expect(api.begin).toHaveBeenCalledOnce());
  invalidateCodeWorkshopContext();
  resolve({ item: { id: "work" }, lease: { lease_id: "old-lease", generation: 1 } });
  await Promise.resolve();
  flushSync();
  expect(api.activate).not.toHaveBeenCalled();
  expect(api.replace).toHaveBeenCalledOnce();
  expect(api.replace.mock.calls[0]![1].dryRun).toBe(true);
});
