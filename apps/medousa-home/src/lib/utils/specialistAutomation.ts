import type { ManuscriptCatalogEntry } from "$lib/types/catalog";
import type { ManuscriptDetailResponse } from "$lib/types/manuscript";
import type { AutomationCreateDraft } from "$lib/types/recurring";

export function automationDraftForSpecialist(
  entry: ManuscriptCatalogEntry,
  detail?: ManuscriptDetailResponse | null,
): Partial<AutomationCreateDraft> {
  const resolved =
    detail && detail.id === entry.id ? detail : null;
  const taskTemplate = resolved?.task_template?.trim();
  return {
    display_name: entry.name,
    prompt: taskTemplate || `Run ${entry.name} on schedule`,
    cron_expr: resolved?.schedule_cron?.trim() || "0 9 * * *",
    manuscript_id: entry.id,
  };
}
