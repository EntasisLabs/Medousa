/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { WorkCard } from "$lib/types/workspace";
import WorkMotionPeek from "./WorkMotionPeek.svelte";
import StatusBar from "./StatusBar.svelte";

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (component) await unmount(component);
  component = undefined;
  document.body.replaceChildren();
});

function card(id: string, column: string, status_label = "running", date = "2026-10-28T00:00:00Z"): WorkCard {
  return { id, column, status_label, title: id, created_at_utc: date, updated_at_utc: date };
}

it("groups real states without treating failures or cancellations as decisions", () => {
  const select = vi.fn();
  component = mount(WorkMotionPeek, { target: document.body, props: {
    cards: [card("approval", "blocked", "needs approval"), card("waiting", "blocked", "needs input"),
      card("working", "in_flight"), card("queued", "backlog", "queued"),
      card("finished", "done", "completed"), card("failed", "blocked", "failed"),
      card("stopped", "blocked", "canceled", "2026-10-28T01:00:00Z")],
    selectedId: "approval", onSelect: select,
  } });
  flushSync();
  const decision = document.querySelector('ul[aria-label="Needs your decision"]')!;
  expect(decision.querySelectorAll("button")).toHaveLength(2);
  expect(decision.textContent).toContain("approval");
  expect(decision.textContent).not.toContain("failed");
  expect(document.querySelector('ul[aria-label="In progress"]')?.querySelectorAll("button")).toHaveLength(2);
  const finished = document.querySelector('ul[aria-label="Recently finished"]')!;
  expect(finished.querySelectorAll("button")).toHaveLength(3);
  expect(finished.querySelector("button")?.textContent).toContain("stopped");
  (decision.querySelector('[aria-current="true"]') as HTMLButtonElement).click();
  expect(select).toHaveBeenCalledExactlyOnceWith("approval");
});

it.each([
  ["waiting", "blocked", "needs approval"],
  ["finished", "done", "completed"],
])("keeps the popover available with only %s work and zero running", (id, column, status) => {
  const select = vi.fn();
  component = mount(StatusBar, { target: document.body, props: {
    health: null, minimal: true, inMotionCount: 0, needsAttentionCount: column === "blocked" ? 1 : 0,
    motionCards: [card(id, column, status)], onSelectMotion: select,
  } });
  flushSync();
  const trigger = document.querySelector<HTMLButtonElement>('[title="Workshop activity"]')!;
  expect(trigger).not.toBeNull();
  trigger.click();
  flushSync();
  const popup = document.querySelector('[role="dialog"][aria-label="Workshop activity"]')!;
  expect(popup).not.toBeNull();
  expect(popup.parentElement?.tagName).toBe('FOOTER');
  expect(popup.closest('.status-bar-trailing')).toBeNull();
  document.querySelector<HTMLButtonElement>(".work-motion-peek-card")!.click();
  flushSync();
  expect(select).toHaveBeenCalledExactlyOnceWith(id);
  expect(document.querySelector('[role="dialog"][aria-label="Workshop activity"]')).toBeNull();
});

it('dismisses activity with Escape and outside click', () => {
  component = mount(StatusBar, { target: document.body, props: { health: null, inMotionCount: 1, needsAttentionCount: 0, motionCards: [card('work', 'in_flight')], onSelectMotion: vi.fn() } });
  flushSync();
  const trigger = document.querySelector<HTMLButtonElement>('[title="Workshop activity"]')!;
  trigger.click(); flushSync();
  document.querySelector('footer')!.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })); flushSync();
  expect(document.querySelector('.work-motion-peek')).toBeNull();
  trigger.click(); flushSync();
  document.querySelector<HTMLElement>('.work-motion-scrim')!.click(); flushSync();
  expect(document.querySelector('.work-motion-peek')).toBeNull();
});

it("shows an honest empty state", () => {
  component = mount(WorkMotionPeek, { target: document.body, props: { cards: [], selectedId: null, onSelect: vi.fn() } });
  flushSync();
  expect(document.body.textContent).toContain("No recent activity");
  expect(document.querySelector("button")).toBeNull();
});
