import type { CronCreateDraft } from "$lib/types/recurring";

export class CronDraftStore {
  createDraft = $state<CronCreateDraft | null>(null);
  showCreate = $state(false);

  openCreate(draft?: Partial<CronCreateDraft>) {
    this.createDraft = {
      prompt: draft?.prompt ?? "",
      cron_expr: draft?.cron_expr ?? "0 9 * * *",
      manuscript_id: draft?.manuscript_id,
      timezone: draft?.timezone ?? "UTC",
    };
    this.showCreate = true;
  }

  clearCreate() {
    this.createDraft = null;
    this.showCreate = false;
  }
}

export const cronDraft = new CronDraftStore();
