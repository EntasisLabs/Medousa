import { untrack } from "svelte";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import { loadCompanionDrafts, saveCompanionDrafts, type CompanionDrafts } from "./companionDrafts";

/** Owned by the mounted companion; drafts survive releasing its WebView. */
export class CompanionDraftState {
  chat = $state("");
  ask = $state("");
  mode = $state<CompanionDrafts["mode"]>("chat");
  private scope = activeWorkshopId();

  constructor() {
    this.restore();
    $effect(() => {
      const scope = activeWorkshopId();
      if (scope === this.scope) return;
      untrack(() => {
        this.persist();
        this.scope = scope;
        this.restore();
      });
    });
    $effect(() => this.persist());
  }

  private restore() {
    const drafts = loadCompanionDrafts(this.scope);
    this.chat = drafts.chat;
    this.ask = drafts.ask;
    this.mode = drafts.mode;
  }

  private persist() {
    saveCompanionDrafts(this.scope, { chat: this.chat, ask: this.ask, mode: this.mode });
  }
}
