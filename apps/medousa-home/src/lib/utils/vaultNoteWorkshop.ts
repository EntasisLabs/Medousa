/** Launch the floating vault note workshop with scoped chat context. */

import { chat } from "$lib/stores/chat.svelte";
import { noteWorkshop } from "$lib/stores/noteWorkshop.svelte";
import { prepareTalkAboutNote } from "$lib/utils/vaultNoteBridge";

export async function launchVaultNoteWorkshop(input: {
  path: string;
  title: string;
  content: string;
  wikilinksOut: string[];
  backlinks: string[];
  session: "fresh" | string;
  flushSave?: () => Promise<void | boolean>;
}) {
  if (input.flushSave) await input.flushSave();

  if (input.session === "fresh") {
    await chat.newSession();
  } else if (input.session !== chat.sessionId) {
    await chat.switchSession(input.session);
  }

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

  void chat.ensureSessionHydrated();
  noteWorkshop.openForNote(input.path);
}
