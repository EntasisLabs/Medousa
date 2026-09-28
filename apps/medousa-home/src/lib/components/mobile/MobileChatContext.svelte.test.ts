/** @vitest-environment happy-dom */
import { afterEach, expect, it, vi } from "vitest";
import { flushSync, mount, unmount } from "svelte";
import type { createAgentSessionController } from "$lib/chat/agentSessionController.svelte";
import type { createExternalConversationController } from "$lib/chat/externalConversationController.svelte";
import type { ChatAgentRuntime } from "$lib/utils/sessionAgentRuntime";

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

it("opens Runtime above Mode, then selects and reselects a Muse session in the sheet", () => {
  const state = $state({ runtime: "medousa" as ChatAgentRuntime, selectedId: null as string | null });
  const choices = [{ id: "personal", provider: "muse", label: "Personal Muse" }, { id: "work", provider: "muse", label: "Work Muse" }];
  const change = vi.fn((value: ChatAgentRuntime) => { state.runtime = value; });
  const select = vi.fn((id: string) => { state.selectedId = id; });
  const agentSession = { get sessionRuntime() { return state.runtime; }, onRuntimeChange: change } as unknown as ReturnType<typeof createAgentSessionController>;
  const externalConversation = {
    choices, loading: false, error: null, refresh: vi.fn(), select,
    get selectedId() { return state.selectedId; },
    get selected() { return choices.find((item) => item.id === state.selectedId) ?? null; },
  } as unknown as ReturnType<typeof createExternalConversationController>;
  const target = document.createElement("div"); document.body.append(target);
  component = mount(MobileChatContext, { target, props: { agentSession, externalConversation } });
  flushSync();
  click("Chat context");
  expect([...document.querySelectorAll(".context-row")].filter((row) => !row.closest("[hidden]")).map((row) => row.firstElementChild?.textContent)).toEqual(["Runtime", "Mode", "Project", "Workers"]);
  click("Runtime");
  click("Muse");
  expect(document.querySelector("dialog")?.getAttribute("aria-label")).toBe("Muse session");
  click("Work Muse");
  expect(select).toHaveBeenLastCalledWith("work");
  expect(document.querySelector("dialog")?.getAttribute("aria-label")).toBe("Chat context");
  expect(document.querySelector(".context-summary")?.textContent).toContain("Work Muse");
  click("Runtime");
  click("Muse"); // Already-selected runtime must still let us choose another session.
  click("Personal Muse");
  expect(select).toHaveBeenLastCalledWith("personal");
  expect(change).toHaveBeenCalledTimes(1);
  click("Runtime");
  click("Medousa");
  expect(state.runtime).toBe("medousa");
  expect([...document.querySelectorAll(".context-row")].filter((row) => !row.closest("[hidden]")).map((row) => row.firstElementChild?.textContent)).toContain("Mode");
});
