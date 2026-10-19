/** @vitest-environment happy-dom */
import { expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import ChatHistoryPanelHarness from "./test-fixtures/ChatHistoryPanelHarness.svelte";
import type { ChatHistoryController } from "./chatHistoryController.svelte";

it("keeps viewport ownership stable through token updates and releases only on visibility changes", async () => {
  const data = $state({ messages: [{ id: "live", role: "assistant", content: "first" }] });
  const stop = vi.fn();
  const retain = vi.fn(() => { void data.messages.length; return stop; });
  const history = { retain, messagesFor: () => data.messages } as unknown as ChatHistoryController;
  const props = $state({ history, sessionId: "session", visible: true });
  const target = document.createElement("div"); document.body.append(target);
  const component = mount(ChatHistoryPanelHarness, { target, props }); flushSync();
  try {
    expect(retain).toHaveBeenCalledTimes(1);
    data.messages = [...data.messages, { id: "next", role: "assistant", content: "streaming" }];
    flushSync();
    expect(target.textContent).toBe("2 messages");
    expect(retain).toHaveBeenCalledTimes(1);
    expect(stop).not.toHaveBeenCalled();
    props.visible = false; flushSync();
    expect(stop).toHaveBeenCalledTimes(1);
    props.visible = true; flushSync();
    expect(retain).toHaveBeenCalledTimes(2);
  } finally { await unmount(component); target.remove(); }
});
