import {
  deleteRecurring,
  getRecurringDelivery,
  listRecurring,
  listRecurringRuns,
  registerRecurringPrompt,
  updateRecurring,
} from "$lib/daemon";
import { formatDaemonErrorSummary } from "$lib/utils/formatDaemonError";
import type {
  AutomationDeliveryMode,
  RecurringDefinitionEntry,
  RecurringRunEntry,
  RegisterRecurringRequest,
  UpdateRecurringRequest,
} from "$lib/types/recurring";

export class AutomationsStore {
  definitions = $state<RecurringDefinitionEntry[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  registerMessage = $state<string | null>(null);
  registering = $state(false);
  updatingId = $state<string | null>(null);
  deletingId = $state<string | null>(null);
  runsById = $state<Record<string, RecurringRunEntry[]>>({});
  runsLoadingId = $state<string | null>(null);
  runsErrorById = $state<Record<string, string>>({});

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

  async loadRuns(recurringId: string, limit = 20) {
    this.runsLoadingId = recurringId;
    try {
      const response = await listRecurringRuns(recurringId, limit);
      this.runsById = { ...this.runsById, [recurringId]: response.runs };
      const { [recurringId]: _removed, ...rest } = this.runsErrorById;
      this.runsErrorById = rest;
    } catch (err) {
      this.runsErrorById = {
        ...this.runsErrorById,
        [recurringId]: err instanceof Error ? err.message : String(err),
      };
    } finally {
      this.runsLoadingId = null;
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
    if (entry.display_name?.trim()) return entry.display_name.trim();
    if (entry.manuscript_id) return entry.manuscript_id;
    if (entry.prompt_excerpt) return entry.prompt_excerpt;
    return entry.recurring_id;
  }

  originFor(entry: RecurringDefinitionEntry): string {
    if (entry.manuscript_id) return "Specialist";
    return "Manual";
  }

  deliveryLabelFor(entry: RecurringDefinitionEntry): string {
    return entry.delivery_label?.trim() || "In Medousa";
  }

  statusLabel(status: string | null | undefined): string {
    if (!status) return "—";
    if (status === "succeeded") return "Succeeded";
    if (status === "failed") return "Failed";
    if (status === "running") return "Running";
    if (status === "queued") return "Queued";
    return status;
  }

  formatTimestamp(value: string | null | undefined): string {
    if (!value) return "—";
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

  formatNextRun(value: string): string {
    return this.formatTimestamp(value);
  }

  buildDeliveryPayload(
    mode: AutomationDeliveryMode,
    telegramChatId?: string,
  ): Record<string, unknown> | null {
    if (mode === "in_app" || mode === "quiet") return null;
    if (mode === "telegram") {
      const chatId = telegramChatId?.trim();
      if (!chatId) return null;
      return {
        channel: "telegram",
        telegram_chat_id: chatId,
      };
    }
    return null;
  }

  async register(request: RegisterRecurringRequest) {
    this.registering = true;
    this.registerMessage = null;
    try {
      const delivery = this.buildDeliveryPayload(
        request.delivery_mode ?? "in_app",
        request.telegram_chat_id,
      );
      const response = await registerRecurringPrompt({
        prompt: request.prompt,
        cron_expr: request.cron_expr,
        display_name: request.display_name,
        manuscript_id: request.manuscript_id,
        timezone: request.timezone ?? "UTC",
        execution_mode: request.execution_mode ?? "agent_turn",
        model_hint: request.model_hint,
        policy_profile: "scheduled",
        enabled: true,
        max_attempts: 1,
        queue: "default",
        delivery,
      });
      this.registerMessage = `Scheduled · next ${this.formatNextRun(response.next_run_at_utc)}`;
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

  async updateDelivery(
    recurringId: string,
    mode: AutomationDeliveryMode,
    telegramChatId?: string,
  ) {
    this.updatingId = recurringId;
    try {
      const delivery = this.buildDeliveryPayload(mode, telegramChatId);
      await updateRecurring(recurringId, { delivery });
      await this.refresh();
      await getRecurringDelivery(recurringId);
    } finally {
      this.updatingId = null;
    }
  }

  async updateCron(
    recurringId: string,
    patch: Pick<UpdateRecurringRequest, "cron_expr" | "timezone" | "display_name">,
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
      const { [recurringId]: _runs, ...restRuns } = this.runsById;
      this.runsById = restRuns;
    } finally {
      this.deletingId = null;
    }
  }
}

export const automations = new AutomationsStore();

/** @deprecated use automations */
export const recurring = automations;
