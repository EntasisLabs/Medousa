/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import CreateTextControls from "./CreateTextControls.svelte";
import CreateAppearanceControls from "./CreateAppearanceControls.svelte";
import CreateViewControls from "./CreateViewControls.svelte";
import { createNode } from "$lib/create/creativeScene";
import { defaultLayout } from "$lib/create/creativeLayout";
const components: ReturnType<typeof mount>[] = [];
afterEach(async () => { for (const component of components.splice(0)) await unmount(component); document.body.innerHTML = ""; });
it("applies paragraph width and spacing while the numeric field still has focus", () => {
  const onchange = vi.fn();
  components.push(mount(CreateTextControls, { target: document.body, props: { value: { text_width: 240 }, onchange } })); flushSync();
  for (const [label, value, patch] of [["Text box width", "180", { text_width: 180 }], ["Line spacing", "1.6", { line_height: 1.6 }]] as const) {
    const input = document.querySelector<HTMLInputElement>(`[aria-label="${label}"]`)!;
    input.focus(); input.value = value; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
    expect(document.activeElement).toBe(input); expect(onchange).toHaveBeenLastCalledWith(patch);
  }
});
it("keeps fill, border and radius edits separate without needing a blur", () => {
  const onchange = vi.fn(), node = createNode({ kind: "shape", shape: "rectangle", fill: "#fff", stroke: "#000", stroke_width: 1 }, { x: 0, y: 0, width: 200, height: 100 });
  components.push(mount(CreateAppearanceControls, { target: document.body, props: { node, onchange } })); flushSync();
  const radius = document.querySelector<HTMLInputElement>('[aria-label="Corner radius"]')!;
  radius.value = "20"; radius.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  expect(onchange).toHaveBeenLastCalledWith({ corner_radius: 20 });
  const border = document.querySelector<HTMLInputElement>('[aria-label="Border width"]')!;
  border.value = "0"; border.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  expect(onchange).toHaveBeenLastCalledWith({ border_width: 0 });
});
it("uses accessible app check rows rather than native checkbox widgets in View", () => {
  components.push(mount(CreateViewControls, { target: document.body, props: { settings: { ...defaultLayout }, onfit: vi.fn(), onselection: vi.fn(), hasSelection: false } })); flushSync();
  expect(document.querySelector('input[type="checkbox"]')).toBeNull();
  const row = document.querySelector<HTMLButtonElement>('button[role="checkbox"]')!;
  const before = row.getAttribute("aria-checked"); row.click(); flushSync();
  expect(row.getAttribute("aria-checked")).not.toBe(before);
});
