/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { mount, unmount, flushSync } from "svelte";
import type { SpotlightHost, SpotlightSnapshot } from "$lib/spotlight/host";
vi.mock("$lib/stores/vault.svelte", () => ({ vault: { notes: [{ path: "placeholder" }], labelByPathMap: new Map() } }));
vi.mock("$lib/stores/workshop.svelte", () => ({ workshop: { scripts: [{ id: "placeholder" }] } }));
vi.mock("$lib/platform", async importOriginal => ({ ...await importOriginal<typeof import("$lib/platform")>(), isTauriMacDesktop: () => true }));
import CommandSpotlight from "./CommandSpotlight.svelte";
import { commandSpotlight } from "$lib/stores/commandSpotlight.svelte";
let component: ReturnType<typeof mount> | undefined;
const snapshot: SpotlightSnapshot = { scopes: [{ id: "home", label: "Home" }, { id: "desk", label: "Labbing" }], layout: null, groups: [{ id: "open", label: "Open", commands: [{ id: "open-note:hello", section: "open", label: "Hello note", token: "note-ticket" }] }] };
function host(): SpotlightHost {
  return { collect: vi.fn(async () => snapshot), execute: vi.fn(async () => { commandSpotlight.closeSpotlight(); }), pin: vi.fn(), openConversation: vi.fn(), openActivity: vi.fn(), newAsk: vi.fn(), manageConnection: vi.fn(), createFlow: vi.fn(), openSsh: vi.fn() };
}
async function settle(ms = 30) { await new Promise(resolve => setTimeout(resolve, ms)); flushSync(); }
async function start(owner: SpotlightHost) {
  commandSpotlight.openSpotlight();
  component = mount(CommandSpotlight, { target: document.body, props: { host: owner } });
  await settle();
}
afterEach(async () => { commandSpotlight.closeSpotlight(); if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });

it("uses shared navigation and routes selected commands to their real owner", async () => {
  const owner = host(); await start(owner);
  expect(document.querySelector('[aria-label="Spotlight categories"]')).not.toBeNull();
  expect(document.body.textContent).toContain("Activity");
  expect(document.body.textContent).toContain("Labbing");
  expect(document.querySelector(".command-spotlight-footer")).toBeNull();
  const input = document.querySelector<HTMLInputElement>("input")!;
  input.value = "Hello note"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); await settle(110);
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await settle();
  expect(owner.execute).toHaveBeenCalledWith(snapshot.groups[0].commands[0], undefined);
  expect(commandSpotlight.open).toBe(false);
});

it("keeps background catalog notices quiet when commands are available", async () => {
  const owner = host();
  vi.mocked(owner.collect).mockResolvedValue({ ...snapshot, notice: "An optional catalog could not load" });
  await start(owner);
  expect(document.body.textContent).toContain("Hello note");
  expect(document.body.textContent).not.toContain("An optional catalog could not load");
});

it("passes create, run and advanced prefixes through the shared catalog", async () => {
  const owner = host(); await start(owner);
  for (const query of ["+note", "!script", ">terminal"]) {
    const input = document.querySelector<HTMLInputElement>("input")!;
    input.value = query; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); await settle(110);
    expect(owner.collect).toHaveBeenLastCalledWith(expect.objectContaining({ query, mode: "default" }));
    expect(document.body.textContent).toContain("Hello note");
  }
});

it("drops late catalog responses after search changes and keeps failures visible", async () => {
  const owner = host();
  let finish!: (value: SpotlightSnapshot) => void;
  vi.mocked(owner.collect).mockImplementationOnce(() => new Promise(resolve => { finish = resolve; }));
  await start(owner);
  const input = document.querySelector<HTMLInputElement>("input")!;
  input.value = "current"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); await settle(110);
  finish({ ...snapshot, groups: [{ id: "old", label: "Old", commands: [{ ...snapshot.groups[0].commands[0], label: "Stale result" }] }] }); await settle();
  expect(document.body.textContent).not.toContain("Stale result");
  vi.mocked(owner.execute).mockRejectedValueOnce(new Error("The document changed"));
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await settle();
  expect(commandSpotlight.open).toBe(true);
  expect(document.body.textContent).toContain("The document changed");
});

it("keeps @ addressing available in desktop search", async () => {
  await start(host());
  const input = document.querySelector<HTMLInputElement>("input")!;
  input.value = "@"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); await settle();
  expect(document.body.textContent).toContain("Choose who to talk to");
});

it("navigates results using AppKit arrow keys without sending controls to search", async () => {
  const owner = host();
  const second = { ...snapshot.groups[0].commands[0], id: "open-note:second", label: "Second note", token: "second-ticket" };
  vi.mocked(owner.collect).mockResolvedValue({ ...snapshot, groups: [{ ...snapshot.groups[0], commands: [...snapshot.groups[0].commands, second] }] });
  await start(owner);
  const input = document.querySelector<HTMLInputElement>("input")!;
  input.value = "sd"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); await settle(110);
  const arrow = new KeyboardEvent("keydown", { key: "\uF701", bubbles: true, cancelable: true });
  input.dispatchEvent(arrow); flushSync();
  expect(arrow.defaultPrevented).toBe(true);
  const leaked = new InputEvent("beforeinput", { data: "\uF701", inputType: "insertText", cancelable: true });
  input.dispatchEvent(leaked); expect(leaked.defaultPrevented).toBe(true);
  // Defensive input cleanup must also update the Svelte binding before collection.
  input.value = "sd\uF700\uF72B"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync(); await settle(110);
  expect(input.value).toBe("sd");
  expect(owner.collect).toHaveBeenLastCalledWith(expect.objectContaining({ query: "sd" }));
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "\uF701", bubbles: true, cancelable: true })); flushSync();
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await settle();
  expect(owner.execute).toHaveBeenCalledWith(second, undefined);
});

it("keeps navigation controls out of follow-up command arguments", async () => {
  await start(host());
  const run = vi.fn();
  commandSpotlight.beginPrompt({ commandId: "test-prompt", label: "Project name", placeholder: "Name", submitLabel: "Create" },
    { id: "test-prompt", section: "do", label: "Create", run });
  flushSync(); await settle();
  const input = document.querySelector<HTMLInputElement>("input")!;
  input.value = "project\uF72B"; input.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  expect(input.value).toBe("project");
  input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await settle();
  expect(run).toHaveBeenCalledWith(expect.anything(), "project");
});
