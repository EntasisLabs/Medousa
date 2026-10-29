/** @vitest-environment happy-dom */
import { describe, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import ChatMessageList from "./ChatMessageList.svelte";
import type { SubagentRow } from "$lib/utils/subagentRows";

vi.mock("./LiquidChatMessage.svelte", () => ({ default: vi.fn() }));
vi.mock("./ChatUserWhisper.svelte", () => ({ default: vi.fn() }));
vi.mock("./ChatForkMenu.svelte", () => ({ default: vi.fn() }));
vi.mock("./ToolActivitySheet.svelte", () => ({ default: vi.fn() }));
vi.mock("$lib/utils/saveChatTurnToVault", () => ({ canSaveAssistantTurn: () => false }));
vi.mock("$lib/share", () => ({ shareText: vi.fn() }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { sessionId: "chat", draft: "" } }));
vi.mock("$lib/stores/undertakings.svelte", () => ({ undertakings: {} }));
vi.mock("$lib/stores/connection.svelte", () => ({ connection: {} }));
vi.mock("$lib/stores/workshops.svelte", () => ({ workshops: {} }));
vi.mock("$lib/stores/executionTargets.svelte", () => ({ executionTargets: { runtimeLabel: () => "Mac mini" } }));
vi.mock("$lib/stores/narration.svelte", () => ({ narration: { initialize: vi.fn(), available: false } }));

describe("inline agent results", () => {
  it("returns to the spawning task link from its later result without legacy blocks", async () => {
    const target = document.createElement("div"); document.body.append(target);
    const workers = new Map(Array.from({ length: 6 }, (_, i) => [`work-${i}`, {
      workId: `work-${i}`, parentMessageId: "spawn", title: `Task ${i}`,
      disposition: "parallel", statusLine: "Work completed", toolRuns: [],
      thinking: "", thinkingSeconds: null, streaming: false, terminal: true,
    } as SubagentRow]));
    const component = mount(ChatMessageList, { target, props: { sessionId: "chat", subagentRows: workers,
      messages: [{ id: "request", role: "user", content: "Start six tasks" },
        { id: "spawn", role: "assistant", content: "Starting" },
        { id: "later", role: "user", content: "A later question" },
        { id: "reply", role: "assistant", content: "Reply" },
        { id: "result", role: "assistant", content: "Verified result", lane: "worker", workId: "work-5" }],
    } });
    flushSync();
    try {
      const group = target.querySelector(".task-receipts")!;
      expect(target.querySelector(".chat-turn-beat")?.nextElementSibling).toBe(group);
      expect(target.querySelector('.agent-group, .agent-work-line')).toBeNull();
      const scroll = HTMLElement.prototype.scrollIntoView;
      HTMLElement.prototype.scrollIntoView = vi.fn();
      try { (target.querySelector(".agent-return") as HTMLButtonElement).click(); flushSync(); }
      finally { HTMLElement.prototype.scrollIntoView = scroll; }
      expect(document.activeElement).toBe(group.querySelector('[data-worker-id="work-5"]'));
      expect(group.querySelectorAll('.worker-receipt')).toHaveLength(6);
    } finally { await unmount(component); target.remove(); }
  });
  it("keeps the task link mounted when the handoff shell is hidden after completion", async () => {
    const target = document.createElement("div"); document.body.append(target);
    const row: SubagentRow = { workId: "work", parentMessageId: "shell", title: "Verify setup", disposition: "parallel", statusLine: "Working", toolRuns: [], thinking: "", thinkingSeconds: null, streaming: true, terminal: false };
    const props = $state({ sessionId: "chat", subagentRows: new Map([["work", row]]), messages: [
      { id: "request", role: "user" as const, content: "Verify setup" },
      { id: "shell", role: "assistant" as const, content: "", streaming: true, stageWhisper: "Delegated" },
    ] });
    const component = mount(ChatMessageList, { target, props }); flushSync();
    try {
      const group = target.querySelector(".task-receipts")!;
      const task = target.querySelector('[data-worker-id="work"]')!;
      props.messages = props.messages.map(message => message.role === "assistant" ? { ...message, streaming: false } : message);
      props.subagentRows = new Map([["work", { ...row, attention: true, terminal: true, streaming: false, statusLine: "Work failed" }]]);
      flushSync();
      expect(target.querySelector(".task-receipts")).toBe(group);
      expect(target.querySelector('[data-worker-id="work"]')).toBe(task);
      expect(task.textContent).toContain('Needs attention');
      expect(target.querySelector('.agent-group, .agent-work-line')).toBeNull();
    } finally { await unmount(component); target.remove(); }
  });

});
