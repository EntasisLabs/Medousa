import { untrack } from "svelte";
import { chat } from "$lib/stores/chat.svelte";
import { bots } from "$lib/stores/bots.svelte";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import { createConversationSurface } from "$lib/spotlight/conversationSurface.svelte";
import { type ConversationTarget } from "$lib/spotlight/conversationTargets";
import { loadAddressedDraft, savedAddress, saveAddressedDraft } from "./addressedDraft";

/** The mounted companion owns its address, independently of the focused chat. */
export class CompanionConversation {
  query = $state("");
  restoring = $state(true);
  restoreError = $state<string | null>(null);
  private scope = $state<string | null>(null);
  private restoreVersion = 0;
  readonly conversation;

  constructor(visible: () => boolean, ready: () => boolean) {
    this.conversation = createConversationSurface(visible, () => !this.conversation.target, () => {});
    $effect(() => {
      const scope = activeWorkshopId();
      if (!ready() || chat.workshopScopeId !== scope || this.scope === scope) return;
      untrack(() => { void this.restore(scope); });
    });
    const browsing = $derived(!this.conversation.target);
    $effect(() => {
      if (!visible() || !ready() || !browsing) return;
      untrack(() => { void bots.refresh(); });
    });
    $effect(() => { if (this.conversation.target) this.restoreError = null; });
    $effect(() => {
      const scope = this.scope;
      const draft = { query: this.query, address: savedAddress(this.conversation.target, this.conversation.sessionId) };
      if (scope && !this.restoring && !this.restoreError && scope === activeWorkshopId() && scope === chat.workshopScopeId) saveAddressedDraft(scope, draft);
    });
  }

  get available() { return !this.restoring && this.scope === activeWorkshopId() && this.scope === chat.workshopScopeId; }

  newTarget(): ConversationTarget {
    return { id: "new:medousa", kind: "new", runtime: "medousa", label: "Medousa", group: "Start new", scope: chat.workshopScopeId ?? "", description: "Start a new assistant conversation" };
  }
  startNew() {
    if (this.conversation.sending || this.restoring) return;
    this.restoreError = null;
    this.conversation.select(this.newTarget());
  }
  private async restore(scope: string) {
    const version = ++this.restoreVersion;
    this.scope = scope;
    this.restoring = true;
    this.restoreError = null;
    this.conversation.clear();
    const draft = loadAddressedDraft(scope);
    this.query = draft.query;
    try {
      const address = draft.address;
      if (address?.kind === "bot") {
        await bots.refresh();
        if (version !== this.restoreVersion || scope !== chat.workshopScopeId) return;
        const bot = bots.bots.find(item => item.bot_id === address.botId && !item.archived);
        if (!bot) throw new Error(`Select a destination again. ${address.label} is no longer available.`);
        this.conversation.select({ id: `bot:${bot.bot_id}`, kind: "bot", bot, label: bot.display_name, group: "Bots", scope, description: bot.role_description || "" });
      } else if (address?.kind === "session") {
        this.conversation.select({ id: `session:${address.sessionId}`, kind: "session", sessionId: address.sessionId, label: address.label, group: "Conversations", scope, description: "Continue this conversation" });
      } else if (address?.kind === "new") {
        this.conversation.select({ id: `new:${address.runtime}`, kind: "new", runtime: address.runtime, label: address.label, group: "Start new", scope, description: "" }, address.sessionId);
      }
    } catch (cause) {
      if (version === this.restoreVersion && scope === chat.workshopScopeId) this.restoreError = cause instanceof Error ? cause.message : String(cause);
    } finally {
      if (version === this.restoreVersion) this.restoring = false;
    }
  }
}
