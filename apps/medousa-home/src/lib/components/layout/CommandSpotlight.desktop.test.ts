/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
vi.mock("$lib/stores/vault.svelte", () => ({ vault: { notes: [{ path: "placeholder" }], labelByPathMap: new Map() } }));
vi.mock("$lib/stores/workshop.svelte", () => ({ workshop: { scripts: [{ id: "placeholder" }] } }));
import CommandSpotlight from "./CommandSpotlight.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { commandSpotlight.closeSpotlight(); if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });
it("searches native destinations and uses the same keyboard selection and dismissal", async () => {
  const open = vi.fn(), dismiss = vi.fn();
  commandSpotlight.openSpotlight();
  component = mount(CommandSpotlight, { target: document.body, props: { onDismiss: dismiss, desktopCommands: [
    { id: "desktop-chat", section: "open", label: "Chat", run: () => {} },
    { id: "desktop-create:doc", section: "open", label: "Create design", run: async ctx => { await open(); ctx.callbacks.close(); } },
  ] } });
  await new Promise(resolve => setTimeout(resolve, 25)); flushSync();
  const input = document.querySelector<HTMLInputElement>("input")!;
  input.value = "Create design"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  expect(document.querySelectorAll(".command-spotlight-row")).toHaveLength(1);
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  await new Promise(resolve => setTimeout(resolve, 25)); flushSync();
  expect(open).toHaveBeenCalledOnce(); expect(dismiss).toHaveBeenCalledOnce(); expect(commandSpotlight.open).toBe(false);
});
