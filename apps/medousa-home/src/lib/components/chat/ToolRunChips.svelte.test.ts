/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { createNode } from "$lib/liquid/core";
import type { ToolRunState } from "$lib/types/chat";
import ToolRunChips from "./ToolRunChips.svelte";
import ToolTrace from "$lib/liquid/archetypes/shell/tool_trace/ToolTrace.svelte";

vi.mock("$lib/haptics", () => ({ haptic: vi.fn() }));
vi.mock("$lib/mobileNavigation", () => ({ registerMobileBackHandler: () => () => {} }));
let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
});
const runs: ToolRunState[] = [
  { runId: "read", toolName: "vault.read", status: "succeeded", round: 1 },
  { runId: "edit", toolName: "code.edit", status: "succeeded", round: 2 },
  { runId: "test", toolName: "code.test", status: "succeeded", round: 3 },
];

it("keeps completed calls quiet and opens their full inspector", async () => {
  component = mount(ToolRunChips, { target: document.body, props: { runs } });
  flushSync();
  const trigger = document.querySelector<HTMLButtonElement>('.tool-context-trigger')!;
  expect(trigger.textContent).toContain("3 tools");
  expect(trigger.textContent).toContain("Read · Edit · Test");
  expect(document.querySelector('.tool-context-failed')).toBeNull();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  trigger.click();
  await tick();
  flushSync();
  expect(trigger.getAttribute("aria-expanded")).toBe("true");
  expect(document.querySelector('[role="dialog"]')?.textContent).toContain("vault.read");
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
  await tick();
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  expect(trigger.getAttribute("aria-expanded")).toBe("false");
});

it("keeps streaming activity compact and shows running context together with failures", () => {
  component = mount(ToolTrace, {
    target: document.body,
    props: { node: createNode({ id: "tools", type: "tool_trace", fillState: "ready", props: {
      streaming: true, runs: [ { ...runs[0], status: "failed" }, { ...runs[2], status: "running" } ],
    } }) },
  });
  flushSync();
  const trigger = document.querySelector('.tool-context-trigger')!;
  expect(trigger.textContent).toContain("2 tools");
  expect(trigger.textContent).toContain("Running tests");
  expect(trigger.textContent).toContain("1 failed");
  expect(document.querySelector('[aria-label="Tool lineage"]')).toBeNull();
});

it("preserves the explicit detailed transcript view", () => {
  component = mount(ToolRunChips, { target: document.body, props: { runs, inspectorCollapsed: false } });
  flushSync();
  expect(document.querySelector('.tool-context-trigger')).toBeNull();
  expect(document.querySelector('[aria-label="Tool lineage"]')).not.toBeNull();
});
