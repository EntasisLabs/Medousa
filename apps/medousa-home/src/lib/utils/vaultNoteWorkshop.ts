/** Launch the floating vault note workshop with scoped chat context. */

import { chat } from "$lib/stores/chat.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { noteWorkshop } from "$lib/stores/noteWorkshop.svelte";
import { shouldUseMobileShell } from "$lib/platform";
import {
  prepareAddSelectionToChat,
  prepareTalkAboutNote,
  type VaultNoteSelection,
} from "$lib/utils/vaultNoteBridge";
import { vaultDisplayTitle } from "$lib/utils/formatVault";

export async function launchVaultNoteWorkshop(input: {
  path: string;
  title: string;
  content: string;
  wikilinksOut: string[];
  backlinks: string[];
  session: "fresh" | string;
  flushSave?: () => Promise<void | boolean>;
  selection?: VaultNoteSelection | null;
}) {
  if (input.flushSave) await input.flushSave();

  if (input.session === "fresh") {
    await chat.newSession();
  } else if (input.session !== chat.sessionId) {
    await chat.switchSession(input.session);
  }

  const selectionText = input.selection?.text.trim();
  if (selectionText) {
    const { scope, draft } = prepareAddSelectionToChat(
      input.path,
      input.title,
      {
        text: selectionText,
        start: input.selection?.start,
        end: input.selection?.end,
      },
      input.wikilinksOut,
      input.backlinks,
    );
    chat.vaultNoteContext = scope;
    chat.pinVaultNoteContext = true;
    if (!chat.draft.trim() || chat.messages.length === 0) {
      chat.prefillDraft(draft);
    } else if (!chat.draft.includes(selectionText)) {
      chat.prefillDraft(`${draft}\n\n---\n\n${chat.draft}`);
    }
  } else {
    const { scope, draft } = prepareTalkAboutNote(
      input.path,
      input.title,
      input.content,
      input.wikilinksOut,
      input.backlinks,
    );

    if (input.session === "fresh" || chat.messages.length === 0) {
      chat.prefillFromVaultNote(scope, draft, { pin: true });
    } else {
      chat.vaultNoteContext = scope;
      chat.pinVaultNoteContext = true;
    }
  }

  void chat.ensureSessionHydrated();
  noteWorkshop.openForNote(input.path);
}

/** Highlight → Add to chat (desktop workshop or mobile home chat). */
export async function addVaultSelectionToChat(input: {
  path: string;
  title?: string;
  content?: string;
  wikilinksOut?: string[];
  backlinks?: string[];
  selection: VaultNoteSelection;
  flushSave?: () => Promise<void | boolean>;
}) {
  const title =
    input.title?.trim() ||
    vaultDisplayTitle(input.path.split("/").pop()?.replace(/\.md$/i, "") ?? input.path, input.path);
  const selectionText = input.selection.text.trim();
  if (!selectionText) return;

  if (shouldUseMobileShell()) {
    if (input.flushSave) await input.flushSave();
    const { scope, draft } = prepareAddSelectionToChat(
      input.path,
      title,
      { ...input.selection, text: selectionText },
      input.wikilinksOut ?? [],
      input.backlinks ?? [],
    );
    await chat.newSession();
    chat.prefillFromVaultNote(scope, draft, { pin: true });
    void chat.ensureSessionHydrated();
    layout.setMobileTab("home", { bump: true });
    return;
  }

  const reuseSession =
    noteWorkshop.open &&
    noteWorkshop.notePath === input.path &&
    Boolean(chat.sessionId.trim());

  await launchVaultNoteWorkshop({
    path: input.path,
    title,
    content: input.content ?? "",
    wikilinksOut: input.wikilinksOut ?? [],
    backlinks: input.backlinks ?? [],
    selection: { ...input.selection, text: selectionText },
    session: reuseSession ? chat.sessionId : "fresh",
    flushSave: input.flushSave,
  });
}

/** When a note is pinned to the workshop chat, pass session id on save for linking tags. */
export function workshopSessionIdForVaultSave(path: string | null): string | undefined {
  if (!path) return undefined;
  if (!chat.pinVaultNoteContext || !chat.vaultNoteContext) return undefined;
  if (chat.vaultNoteContext.path !== path) return undefined;
  const sessionId = chat.sessionId?.trim();
  return sessionId || undefined;
}
