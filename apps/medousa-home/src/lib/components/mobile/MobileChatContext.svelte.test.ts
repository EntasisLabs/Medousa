/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { createAgentSessionController } from "$lib/chat/agentSessionController.svelte";
import type { ChatAgentRuntime } from "$lib/utils/sessionAgentRuntime";

const botState = vi.hoisted(() => ({ active: false }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { forSession: () => botState.active ? { display_name: "Ada" } : null } }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { sessionId: "main", workshopScopeId: "mac" } }));
vi.mock("$lib/stores/undertakings.svelte", () => ({ undertakings: { forChat: () => null } }));
vi.mock("$lib/stores/executionTargets.svelte", () => ({ executionTargets: { selectionFor: () => null, selectionLabel: () => "Auto", selectionUnavailable: () => false } }));
vi.mock("$lib/stores/accountConnections.svelte", () => ({ accountConnections: { refresh: vi.fn(), connection: () => ({ binaryPresent: true, authStatus: "signed_in" }) } }));
vi.mock("$lib/stores/settings.svelte", () => ({ settings: { medousaMark: "default", darkMode: true } }));
vi.mock("$lib/stores/settingsNav.svelte", () => ({ settingsNav: { setActiveSection: vi.fn() } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { openMore: vi.fn() } }));
vi.mock("$lib/mobileNavigation", () => ({ registerMobileBackHandler: () => () => {} }));
vi.mock("$lib/utils/mobileSheetGestures", () => ({ attachMobileSheetGestures: () => () => {} }));
vi.mock("$lib/components/chat/ChatAgentModePicker.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/ChatExecutionTargetPicker.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/work/UndertakingContextChip.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/brand/MedousaMark.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/brand/ExternalAgentLogo.svelte", () => ({ default: () => {} }));
import MobileChatContext from "./MobileChatContext.svelte";

let component: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (component) await unmount(component); document.body.replaceChildren(); });
function click(label: string) {
  const root = label === "Chat context" ? document : document.querySelector("dialog[open]")!;
  const button = [...root.querySelectorAll("button")].find((item) => !item.closest("[hidden]") &&
    (item.getAttribute("aria-label") === label || item.querySelector(".chat-runtime-option-title")?.textContent?.trim() === label || item.textContent?.trim().startsWith(label)));
  expect(button, label).toBeTruthy();
  button!.click();
  flushSync();
}

function render(bot = false) {
  botState.active = bot;
  const state = $state({ runtime: "medousa" as ChatAgentRuntime });
  const change = vi.fn((value: ChatAgentRuntime) => { state.runtime = value; });
  const agentSession = { get sessionRuntime() { return state.runtime; }, onRuntimeChange: change } as unknown as ReturnType<typeof createAgentSessionController>;
  const target = document.createElement("div"); document.body.append(target);
  component = mount(MobileChatContext, { target, props: { agentSession } });
  flushSync();
  return { state, change };
}

it("offers only swappable execution runtimes in an ordinary mobile chat", () => {
  const { state, change } = render();
  click("Chat context");
  expect([...document.querySelectorAll(".context-row")].filter((row) => !row.closest("[hidden]")).map((row) => row.firstElementChild?.textContent)).toEqual(["Runtime", "Mode", "Project", "Workers"]);
  click("Runtime");
  expect([...document.querySelectorAll(".chat-runtime-option-title")].map((item) => item.textContent?.trim())).toEqual(["Medousa", "Cursor", "ChatGPT / Codex", "Hermes"]);
  click("Hermes");
  expect(state.runtime).toBe("hermes");
  expect(change).toHaveBeenCalledWith("hermes");
  expect(document.querySelector("dialog")?.getAttribute("aria-label")).toBe("Chat context");
});

it("keeps mode, project, and workers available without a runtime picker in a Bot chat", () => {
  const { change } = render(true);
  click("Chat context");
  expect([...document.querySelectorAll(".context-row")].filter((row) => !row.closest("[hidden]")).map((row) => row.firstElementChild?.textContent)).toEqual(["Mode", "Project", "Workers"]);
  expect(document.querySelector(".chat-runtime-option")).toBeNull();
  expect(change).not.toHaveBeenCalled();
});
