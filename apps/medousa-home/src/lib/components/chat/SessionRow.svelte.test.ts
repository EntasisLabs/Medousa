/** @vitest-environment happy-dom */
import { readFileSync } from "node:fs";
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import SessionRow from "./SessionRow.svelte";

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
});

it.each([false, true])("exposes current location only when selected=%s and preserves actions", async (selected) => {
  const onSelect = vi.fn();
  const onRename = vi.fn();
  const onDelete = vi.fn();
  const onTogglePin = vi.fn();
  component = mount(SessionRow, {
    target: document.body,
    props: {
      session: { session_id: "chat-1", display_name: "Current conversation", preview: "", turns: 4, verification_runs: 0 },
      selected, onSelect, onRename, onDelete, onTogglePin,
    },
  });
  flushSync(); await tick();
  const main = document.querySelector<HTMLButtonElement>(".session-row-main")!;
  expect(main.getAttribute("aria-current")).toBe(selected ? "page" : null);
  expect(document.querySelector(".session-row")!.classList.contains("session-row--selected")).toBe(selected);
  main.click();
  document.querySelector<HTMLButtonElement>('[aria-label="Rename session"]')!.click();
  document.querySelector<HTMLButtonElement>('[aria-label="Delete session"]')!.click();
  document.querySelector<HTMLButtonElement>('[aria-label="Pin session"]')!.click();
  for (const callback of [onSelect, onRename, onDelete, onTogglePin]) expect(callback).toHaveBeenCalledOnce();
});

it.each([false, true])("keeps the inline disclosure independent of navigation when expanded=%s", async (relatedExpanded) => {
  const onSelect = vi.fn();
  const onToggleRelated = vi.fn();
  component = mount(SessionRow, {
    target: document.body,
    props: {
      session: { session_id: "parent", display_name: "Parent chat", preview: "", turns: 0, verification_runs: 0 },
      relatedCount: 2, relatedExpanded, onToggleRelated, onSelect,
      onRename: vi.fn(), onDelete: vi.fn(), onTogglePin: vi.fn(),
    },
  });
  flushSync(); await tick();
  const disclosure = document.querySelector<HTMLButtonElement>('.session-row-disclosure')!;
  expect(disclosure.getAttribute('aria-label')).toBe('2 related chats for Parent chat');
  expect(disclosure.getAttribute('aria-expanded')).toBe(String(relatedExpanded));
  expect(disclosure.closest('.session-row-main')).toBeNull();
  disclosure.click();
  expect(onToggleRelated).toHaveBeenCalledOnce();
  expect(onSelect).not.toHaveBeenCalled();
  document.querySelector<HTMLButtonElement>('.session-row-main')!.click();
  expect(onSelect).toHaveBeenCalledOnce();
  expect(onToggleRelated).toHaveBeenCalledOnce();
});

it("uses tone and title weight for selection, reserving outlines for keyboard focus", () => {
  const css = readFileSync("src/lib/styles/chat.postcss", "utf8");
  const selection = css.match(/  \.session-row--selected \{([^}]+)\}/)![1];
  expect(selection).toContain("--theme-card-hover");
  expect(selection).not.toMatch(/box-shadow|border|outline|--theme-focus|--theme-selection/);
  expect(css).toContain(".session-row--selected .session-row-title");
  expect(css).toContain("font-medium text-content-primary");
  expect(css).toContain(".session-row-main:focus-visible,");
  expect(css).toContain(".session-row-action:focus-visible");
});
