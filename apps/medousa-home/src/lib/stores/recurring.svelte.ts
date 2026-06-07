import {
  deleteRecurring,
  listRecurring,
  registerRecurringPrompt,
  updateRecurring,
} from "$lib/daemon";
import { formatDaemonErrorSummary } from "$lib/utils/formatDaemonError";
import type {
  RecurringDefinitionEntry,
  RegisterRecurringRequest,
  UpdateRecurringRequest,
} from "$lib/types/recurring";

export class RecurringStore {
  definitions = $state<RecurringDefinitionEntry[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  registerMessage = $state<string | null>(null);
  registering = $state(false);
  updatingId = $state<string | null>(null);
  deletingId = $state<string | null>(null);

  async refresh(enabledOnly = false) {
    this.loading = true;
    try {
      const response = await listRecurring(enabledOnly);
      this.definitions = response.recurring;
      this.error = null;
    } catch (err) {
      if (this.definitions.length === 0) {
        this.error = formatDaemonErrorSummary(err);
      }
    } finally {
      this.loading = false;
    }
  }

  activeCount(): { enabled: number; total: number } {
    const total = this.definitions.length;
    const enabled = this.definitions.filter((entry) => entry.enabled).length;
    return { enabled, total };
  }

  soonestEnabled(): RecurringDefinitionEntry | null {
    const enabled = this.definitions.filter((entry) => entry.enabled);
    if (enabled.length === 0) return null;
    return [...enabled].sort(
      (left, right) =>
        new Date(left.next_run_at_utc).getTime() -
        new Date(right.next_run_at_utc).getTime(),
    )[0];
  }

  labelFor(entry: RecurringDefinitionEntry): string {
    if (entry.manuscript_id) return entry.manuscript_id;
    if (entry.prompt_excerpt) return entry.prompt_excerpt;
    return entry.recurring_id;
  }

  originFor(entry: RecurringDefinitionEntry): string {
    if (entry.manuscript_id) return "Skill";
    return "Manual";
  }

  formatNextRun(value: string): string {
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString(undefined, {
      weekday: "short",
      month: "short",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
    });
  }

  async register(request: RegisterRecurringRequest) {
    this.registering = true;
    this.registerMessage = null;
    try {
      const response = await registerRecurringPrompt({
        prompt: request.prompt,
        cron_expr: request.cron_expr,
        manuscript_id: request.manuscript_id,
        timezone: request.timezone ?? "UTC",
        execution_mode:
          request.execution_mode ??
          (request.manuscript_id ? "agent_turn" : "prompt"),
        model_hint: request.model_hint,
        policy_profile: "scheduled",
        enabled: true,
        max_attempts: 1,
        queue: "default",
      });
      this.registerMessage = `Scheduled ${response.recurring_id} · next ${this.formatNextRun(response.next_run_at_utc)}`;
      await this.refresh();
      return response;
    } catch (err) {
      this.registerMessage = err instanceof Error ? err.message : String(err);
      throw err;
    } finally {
      this.registering = false;
    }
  }

  async setEnabled(recurringId: string, enabled: boolean) {
    this.updatingId = recurringId;
    try {
      await updateRecurring(recurringId, { enabled });
      await this.refresh();
    } finally {
      this.updatingId = null;
    }
  }

  async updateCron(
    recurringId: string,
    patch: Pick<UpdateRecurringRequest, "cron_expr" | "timezone">,
  ) {
    this.updatingId = recurringId;
    try {
      await updateRecurring(recurringId, patch);
      await this.refresh();
    } finally {
      this.updatingId = null;
    }
  }

  async remove(recurringId: string) {
    this.deletingId = recurringId;
    try {
      await deleteRecurring(recurringId);
      await this.refresh();
    } finally {
      this.deletingId = null;
    }
  }
}

export const recurring = new RecurringStore();
