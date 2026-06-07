import { listRecurring, registerRecurringPrompt } from "$lib/daemon";
import type {
  RecurringDefinitionEntry,
  RegisterRecurringRequest,
} from "$lib/types/recurring";

export class RecurringStore {
  definitions = $state<RecurringDefinitionEntry[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  registerMessage = $state<string | null>(null);
  registering = $state(false);

  async refresh(enabledOnly = false) {
    this.loading = true;
    this.error = null;
    try {
      const response = await listRecurring(enabledOnly);
      this.definitions = response.recurring;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loading = false;
    }
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
        execution_mode: request.execution_mode ?? (request.manuscript_id ? "agent_turn" : "prompt"),
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
}

export const recurring = new RecurringStore();
