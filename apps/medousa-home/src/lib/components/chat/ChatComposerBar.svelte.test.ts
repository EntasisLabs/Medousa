/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import { fromStore, writable } from "svelte/store";
import ChatComposerBar from "./ChatComposerBar.svelte";
import { chat } from "$lib/stores/chat.svelte";

const actions = vi.hoisted(() => ({ attach: vi.fn(), cancel: vi.fn() }));
vi.mock("$lib/stores/chat.svelte", () => { const state = $state({ draft: "Unsent draft", focusedSessionId: "session", pendingMediaRefs: [], pendingMediaUploading: false, liveStreamActive: false, hasWorkshopHandoff: () => false, attachDroppedFiles: actions.attach, cancelActiveTurn: actions.cancel }); return { chat: state }; });
vi.mock("$lib/stores/runtime.svelte", () => ({ runtime: { savingControls: false } }));
vi.mock("$lib/stores/settings.svelte", () => ({ settings: { showChatModelPicker: false } }));
vi.mock("$lib/stores/bots.svelte", () => ({ bots: { forSession: () => null } }));
vi.mock("$lib/stores/narration.svelte", () => ({ narration: { stop: vi.fn() } }));
vi.mock("$lib/chat/composerModel", () => ({ composerModel: () => ({ provider: "test", model: "model" }) }));
vi.mock("$lib/utils/composerStt", () => ({ composerSttStatus: async () => ({ available: false }), appendComposerDraft: vi.fn(), transcribeComposerAudio: vi.fn() }));
vi.mock("$lib/utils/composerAudioCapture", () => ({ composerMicSupported: () => false, startComposerAudioCapture: vi.fn() }));
vi.mock("$lib/platform", () => ({ isTauri: () => false, isTauriMobilePlatform: () => false, isTauriMacDesktop: () => false }));
vi.mock("$lib/components/chat/ChatAttachmentChips.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/ChatModelPicker.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/ChatRuntimePicker.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/ChatVoiceRecorder.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/ComposerAgentChip.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/ComposerPlusMenu.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/ContextUsageIndicator.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/mobile/MobileComposerTurnSettings.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/mobile/ProfileSwitcherCompact.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/workshops/WorkshopSwitcherCompact.svelte", () => ({ default: () => {} }));

let component: ReturnType<typeof mount> | undefined;
async function settle() { flushSync(); await tick(); flushSync(); }
beforeEach(() => { vi.clearAllMocks(); chat.draft = "Unsent draft"; Object.assign(chat, { liveStreamActive: false }); });
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });

it.each([true, false])("keeps the %s mobile draft and caret editable across reconnects while blocking sends", async (mobile) => {
  const availability = fromStore(writable(false));
  component = mount(ChatComposerBar, { target: document.body, props: { mobile, get offline() { return availability.current; } } });
  await settle();
  const textarea = document.querySelector("textarea")!;
  const send = document.querySelector<HTMLButtonElement>('[aria-label="Send message"]')!;
  textarea.focus(); textarea.setSelectionRange(3, 3);
  expect(send.disabled).toBe(false);
  for (let attempt = 0; attempt < 3; attempt++) {
    availability.current = true; await settle();
    expect(document.querySelector("textarea")).toBe(textarea);
    expect(textarea.disabled).toBe(false);
    expect(textarea.selectionStart).toBe(3);
    expect(document.activeElement).toBe(textarea);
    expect(send.disabled).toBe(true);
    expect(send.title).toBe("Waiting for connection");
    availability.current = false; await settle();
    expect(send.disabled).toBe(false);
    expect(chat.draft).toBe("Unsent draft");
  }
  availability.current = true; await settle();
  textarea.value = "Draft edited while offline"; textarea.dispatchEvent(new Event("input", { bubbles: true })); await settle();
  expect(chat.draft).toBe("Draft edited while offline");
  expect(send.disabled).toBe(true);
  expect(actions.attach).not.toHaveBeenCalled();
});

it("keeps cancellation available during an outage without submitting another message", async () => {
  Object.assign(chat, { liveStreamActive: true });
  component = mount(ChatComposerBar, { target: document.body, props: { mobile: true, offline: true } }); await settle();
  const stop = document.querySelector<HTMLButtonElement>('[aria-label="Stop current turn"]')!;
  expect(stop.disabled).toBe(false); stop.click(); await settle();
  expect(actions.cancel).toHaveBeenCalledOnce();
  expect(chat.draft).toBe("Unsent draft");
});
