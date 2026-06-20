import type { AutomationCreateDraft } from "$lib/types/recurring";

export class AutomationDraftStore {
  createDraft = $state<AutomationCreateDraft | null>(null);
  showCreate = $state(false);

  openCreate(draft?: Partial<AutomationCreateDraft>) {
    this.createDraft = {
      prompt: draft?.prompt ?? "",
      cron_expr: draft?.cron_expr ?? "0 9 * * *",
      display_name: draft?.display_name,
      manuscript_id: draft?.manuscript_id,
      timezone: draft?.timezone ?? "UTC",
      delivery_mode: draft?.delivery_mode ?? "in_app",
      telegram_chat_id: draft?.telegram_chat_id,
    };
    this.showCreate = true;
  }

  clearCreate() {
    this.createDraft = null;
    this.showCreate = false;
  }
}

export const automationDraft = new AutomationDraftStore();

/** @deprecated use automationDraft */
export const cronDraft = automationDraft;
