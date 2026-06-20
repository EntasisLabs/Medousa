/** Launch scoped script chat inside the Automations workbench. */

import { chat } from "$lib/stores/chat.svelte";
import type { GraphemeRunResponse } from "$lib/types/grapheme";
import {
  buildScriptWorkbenchChatDraft,
  type ScriptWorkbenchContextScope,
} from "$lib/utils/scriptWorkbenchBridge";

export async function launchScriptWorkbenchChat(input: {
  scope: ScriptWorkbenchContextScope;
  body: string;
  session: "fresh" | string;
  runError?: string | null;
  runResult?: GraphemeRunResponse["result"] | null;
  compileError?: string | null;
  compileHints?: string[];
}) {
  if (input.session === "fresh") {
    await chat.newSession();
  } else if (input.session !== chat.sessionId) {
    await chat.switchSession(input.session);
  }

  const draft = buildScriptWorkbenchChatDraft(input);

  if (input.session === "fresh" || chat.messages.length === 0) {
    chat.prefillFromScriptWorkbench(input.scope, draft, { pin: true });
  } else {
    chat.syncScriptWorkbenchContext(input.scope);
    chat.pinScriptWorkbenchContext = true;
  }

  await chat.ensureSessionHydrated();
}
