/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync, tick } from "svelte";
import CreateInspector from "./CreateInspector.svelte";
import { createNode } from "$lib/create/creativeScene";
let components: ReturnType<typeof mount>[] = [];
afterEach(async () => { for (const c of components) await unmount(c); components = []; document.body.replaceChildren(); });
it("shows parents first, collapses groups, and renames labels without editing text", async () => {
  const frame = createNode({ kind: "frame", name: "Artboard", background: "#fff" }, { x: 0, y: 0, width: 500, height: 300 });
  const group = createNode({ kind: "group" }, { x: 20, y: 20, width: 300, height: 200 }, frame.id); group.name = "Smiley";
  const text = createNode({ kind: "text", text: "Original text", font_family: "sans-serif", font_size: 32, color: "#000" }, { x: 0, y: 0, width: 100, height: 40 }, group.id);
  const target = document.createElement("div"); document.body.append(target);
  const commit = vi.fn(), select = vi.fn();
  components.push(mount(CreateInspector, { target, props: { nodes: [text, group, frame], selected: [], disabled: false, onselect: select, oncommit: commit, onclose: vi.fn() } })); flushSync();
  expect([...target.querySelectorAll("[data-layer-id]")].map(n => n.getAttribute("data-layer-id"))).toEqual([frame.id, group.id]);
  target.querySelector<HTMLButtonElement>('[aria-label="Expand Smiley"]')!.click(); flushSync();
  expect(target.querySelector(`[data-layer-id="${text.id}"]`)?.getAttribute("aria-level")).toBe("3");
  const name = target.querySelector<HTMLButtonElement>(`[data-layer-id="${text.id}"] .layer-name`)!;
  name.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })); await tick(); flushSync();
  const input = target.querySelector<HTMLInputElement>('[aria-label="Layer name"]')!;
  input.value = "Caption"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); flushSync();
  expect(commit).toHaveBeenCalledWith("Rename layer", [{ op: "put_node", node: expect.objectContaining({ id: text.id, name: "Caption", content: expect.objectContaining({ text: "Original text" }) }) }]);
  expect(text.name).toBeUndefined();
});
it("multiselection does not expose misleading single-object properties", () => {
  const a = createNode({ kind: "group" }, { x: 0, y: 0, width: 100, height: 100 }), b = { ...a, id: "second", transform: { ...a.transform, x: 200 } };
  const target = document.createElement("div"); document.body.append(target);
  components.push(mount(CreateInspector, { target, props: { nodes: [a, b], selected: [a.id, b.id], disabled: false, onselect: vi.fn(), oncommit: vi.fn(), onclose: vi.fn() } })); flushSync();
  expect(target.textContent).toContain("2 objects");
  expect(target.querySelector('[aria-label="width"]')).toBeNull();
});
