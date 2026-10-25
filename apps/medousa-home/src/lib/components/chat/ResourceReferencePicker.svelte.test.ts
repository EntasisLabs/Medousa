/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
const api = vi.hoisted(() => ({ projects: vi.fn(), list: vi.fn(), get: vi.fn() }));
vi.mock("$lib/vault/vaultSearchCatalog.svelte", () => ({ VaultSearchCatalog: class {
  error = null;
  async search() {}
  resultsFor() { return [
    { rootId: "medousa", rootLabel: "Medousa", note: { path: "spec.md", title: "Spec" } },
    { rootId: "obsidian", rootLabel: "Obsidian", note: { path: "spec.md", title: "Spec" } },
  ]; }
} }));
vi.mock("$lib/forge", () => ({ listUndertakings: api.projects }));
vi.mock("$lib/create/creativeApi", () => ({ creativeApi: () => ({ list: api.list, get: api.get }) }));
vi.mock("$lib/utils/workshopLocality", () => ({ activeWorkshopId: () => "local" }));
import ResourceReferencePicker from "./ResourceReferencePicker.svelte";
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.clearAllMocks(); });
it("keeps duplicate note paths distinct and captures keyboard selection before the composer sends", async () => {
  api.projects.mockResolvedValue([]); api.list.mockResolvedValue({ projects: [] });
  const input = document.createElement("input"); input.value = "Use @spec"; document.body.appendChild(input);
  const changed = vi.fn();
  component = mount(ResourceReferencePicker, { target: document.body, props: { value: input.value, references: [], element: input, onChange: changed } });
  flushSync(); input.setSelectionRange(9, 9); input.dispatchEvent(new Event("input")); await tick();
  await vi.waitFor(() => expect(document.querySelectorAll('[role="option"]')).toHaveLength(2));
  expect(document.body.textContent).toContain("Medousa · spec.md"); expect(document.body.textContent).toContain("Obsidian · spec.md");
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true }));
  const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
  input.dispatchEvent(enter); flushSync();
  expect(enter.defaultPrevented).toBe(true); expect(changed).toHaveBeenCalledOnce();
  expect(document.querySelector('.resource-chip')?.textContent).toContain("obsidian");
});
it("shows real frame references and keeps a Code binding an explicit separate action", async () => {
  api.projects.mockResolvedValue([{ id: "code", title: "Shell" }]);
  api.list.mockResolvedValue({ projects: [{ id: "design", title: "Shell" }] });
  api.get.mockResolvedValue({ document: { scene: { nodes: [{ id: "home", name: "Home", visible: true, content: { kind: "frame" } }] } } });
  const input = document.createElement("input"); input.value = "/frame Shell"; document.body.appendChild(input);
  const bind = vi.fn().mockResolvedValue(undefined);
  component = mount(ResourceReferencePicker, { target: document.body, props: { value: input.value, references: [{ kind: "code_project", work_id: "code", title: "Shell" }], element: input, onBindProject: bind } });
  flushSync(); input.setSelectionRange(input.value.length, input.value.length); input.dispatchEvent(new Event("input")); await tick();
  await vi.waitFor(() => expect(document.querySelector('[role="option"]')?.textContent).toContain("Shell · Home"));
  expect(bind).not.toHaveBeenCalled();
  const workHere = Array.from(document.querySelectorAll('button')).find(button => button.textContent === 'Work here')!;
  workHere.click();
  await vi.waitFor(() => expect(bind).toHaveBeenCalledWith("code"));
});
