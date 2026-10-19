/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
const api = vi.hoisted(() => ({ jump: vi.fn(() => true) }));
vi.mock("$lib/platform", async (original) => ({ ...await original<typeof import("$lib/platform")>(), isTauriDesktop: () => true }));
vi.mock("$lib/utils/sshApi", () => ({ sshTargets: async () => [{ target_id: "ssh-test", name: "Test server", username: "ops", host: "10.12.0.16", port: 22, agent_access: true }] }));
vi.mock("$lib/stores/vault.svelte", () => ({ vault: { notes: [{ path: "placeholder" }], labelByPathMap: new Map() } }));
vi.mock("$lib/stores/workshop.svelte", () => ({ workshop: { scripts: [{ id: "placeholder" }] } }));
vi.mock("$lib/commands/pinCommands", () => ({ jumpPinSlot: api.jump }));
vi.mock("$lib/commands/collectCommands", async (original) => {
  const actual = await original<typeof import("$lib/commands/collectCommands")>();
  const groups = [
    { section: "open", label: "Terminals", commands: [{ id: "terminal-test", label: "SSH · Test server", run: () => {} }] },
    { section: "open", label: "Open", commands: [{ id: "note-test", label: "Related note", run: () => {} }] },
  ];
  return { ...actual, collectWorkshopCommands: () => groups, collectWorkshopLensCommands: () => groups };
});
import CommandSpotlight from "./CommandSpotlight.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
let component: ReturnType<typeof mount> | undefined;
async function settle() { await new Promise((resolve) => setTimeout(resolve, 25)); flushSync(); }
afterEach(async () => { commandSpotlight.closeSpotlight(); if (component) await unmount(component); component = undefined; document.body.replaceChildren(); vi.clearAllMocks(); });
it("renders terminal and other Open groups together and keeps query updates working", async () => {
  commandSpotlight.openSpotlight(); component = mount(CommandSpotlight, { target: document.body }); await settle();
  const input = document.querySelector<HTMLInputElement>("input")!;
  input.value = "10.12.0.16"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  expect(document.body.textContent).toContain("SSH · Test server"); expect(document.body.textContent).toContain("Related note");
  input.value = "another query"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  expect(input.value).toBe("another query");
});
it("focuses the SSH picker and keeps numeric search input out of the pin shortcuts", async () => {
  commandSpotlight.openSpotlight(); component = mount(CommandSpotlight, { target: document.body }); await settle();
  let input = document.querySelector<HTMLInputElement>("input")!;
  input.focus(); input.dispatchEvent(new KeyboardEvent("keydown", { key: "1", bubbles: true, cancelable: true }));
  expect(api.jump).not.toHaveBeenCalled();
  commandSpotlight.openSshTargets(); await settle();
  input = document.querySelector<HTMLInputElement>("input")!;
  expect(input.placeholder).toBe("Search saved SSH servers…"); expect(document.activeElement).toBe(input);
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "1", bubbles: true, cancelable: true }));
  expect(api.jump).not.toHaveBeenCalled();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })); await settle();
  expect(commandSpotlight.open).toBe(true); expect(commandSpotlight.mode).toBe("default");
});
