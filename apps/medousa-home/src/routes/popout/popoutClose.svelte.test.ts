/** @vitest-environment happy-dom */
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { flushSync, mount, tick, unmount } from "svelte";
import StickyNote from "./vault/+page.svelte";
import ChatPopout from "./chat/+page.svelte";

const host = vi.hoisted(() => ({
  close: undefined as (() => Promise<void>) | undefined,
  unlisten: vi.fn(), hideNote: vi.fn(), hideChat: vi.fn(), persistDraft: vi.fn(),
  vault: {
    dirty: false, selectedPath: null, labelByPathMap: new Map(), title: "",
    saveWhisper: () => "", applyStickyLivePlane: vi.fn(), flushSave: vi.fn(),
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async (_event, handler) => { host.close = handler; return host.unlisten; }) }));
vi.mock("$lib/stores/vault.svelte", () => ({ vault: host.vault }));
vi.mock("$lib/stores/chat.svelte", () => ({ chat: { flushDraftPersist: host.persistDraft } }));
vi.mock("$lib/runtime/layout.svelte", () => ({ layout: { sessionDrawerOpen: false } }));
vi.mock("$lib/window", () => ({ isTauri: () => true, hideVaultSticky: host.hideNote, hideChatPopout: host.hideChat, setVaultStickyWindowTitle: vi.fn() }));
vi.mock("$lib/workshopConnection", () => ({ connectWorkshop: () => () => {} }));
vi.mock("$lib/utils/vaultSticky", () => ({ resolveVaultStickyOpenPath: () => null, writeVaultStickyPath: vi.fn(), VAULT_STICKY_PATH_KEY: "sticky" }));
vi.mock("$lib/components/vault/VaultEditor.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/vault/VaultEditorOverflowMenu.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/vault/VaultNewNoteDialog.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/vault/VaultNoteWorkshop.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/vault/VaultQuickSwitcher.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/ChatPanel.svelte", () => ({ default: () => {} }));
vi.mock("$lib/components/chat/SessionSidebar.svelte", () => ({ default: () => {} }));

let component: ReturnType<typeof mount> | undefined;
async function settle() { flushSync(); await tick(); await Promise.resolve(); }
beforeEach(() => { vi.clearAllMocks(); host.close = undefined; host.vault.dirty = false; });
afterEach(async () => { if (component) await unmount(component); component = undefined; document.body.replaceChildren(); });

it("keeps a dirty note open when native close cannot save, then allows a successful retry", async () => {
  component = mount(StickyNote, { target: document.body });
  await settle();
  host.vault.dirty = true;
  host.vault.flushSave.mockResolvedValueOnce(false);
  await host.close!();
  expect(host.hideNote).not.toHaveBeenCalled();
  host.vault.flushSave.mockImplementationOnce(async () => { host.vault.dirty = false; return true; });
  await host.close!();
  expect(host.hideNote).toHaveBeenCalledTimes(1);
});

it("waits for note persistence and deduplicates repeated native close requests", async () => {
  component = mount(StickyNote, { target: document.body });
  await settle();
  host.vault.dirty = true;
  let finish!: (saved: boolean) => void;
  host.vault.flushSave.mockImplementationOnce(() => new Promise<boolean>((resolve) => { finish = resolve; }));
  const pending = host.close!();
  await host.close!();
  expect(host.vault.flushSave).toHaveBeenCalledTimes(1);
  expect(host.hideNote).not.toHaveBeenCalled();
  host.vault.dirty = false;
  finish(true);
  await pending;
  expect(host.hideNote).toHaveBeenCalledTimes(1);
});

it("preserves edits made during a save instead of closing over the newer draft", async () => {
  component = mount(StickyNote, { target: document.body });
  await settle();
  host.vault.dirty = true;
  host.vault.flushSave.mockResolvedValueOnce(true);
  await host.close!();
  expect(host.hideNote).not.toHaveBeenCalled();
});

it("persists a chat draft before releasing the native popout and removes the close listener", async () => {
  component = mount(ChatPopout, { target: document.body });
  await settle();
  await host.close!();
  expect(host.persistDraft).toHaveBeenCalledTimes(1);
  expect(host.persistDraft.mock.invocationCallOrder[0]).toBeLessThan(host.hideChat.mock.invocationCallOrder[0]);
  await unmount(component); component = undefined;
  expect(host.unlisten).toHaveBeenCalledTimes(1);
});
