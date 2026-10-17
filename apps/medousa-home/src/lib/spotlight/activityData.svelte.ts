import type { WorkCardDetail } from "$lib/types/card";
import type { WorkCard } from "$lib/types/workspace";
import type { ItemProjection, ProjectTaskRunSummary } from "$lib/forge";
import type { RecurringDefinitionEntry, RecurringRunEntry } from "$lib/types/recurring";
import { activityDetailSummary } from "./activity";
import { isActionableBlockedCard } from "$lib/utils/workCardRetention";

export interface ActivityDataPort {
  projects(cursor: string | null): Promise<{ items: ItemProjection[]; nextCursor: string | null }>;
  projectRuns(workId: string): Promise<ProjectTaskRunSummary[]>;
  schedules(): Promise<RecurringDefinitionEntry[]>;
  scheduleRuns(id: string): Promise<RecurringRunEntry[]>;
  card(id: string): Promise<WorkCardDetail>;
}

async function batches<T>(items: T[], visit: (item: T) => Promise<void>) {
  for (let i = 0; i < items.length; i += 3) await Promise.all(items.slice(i, i + 3).map(visit));
}

/** Lens-owned summaries. Closing the lens invalidates requests and releases its caches. */
export class ActivityData {
  projects = $state<ItemProjection[]>([]);
  projectRuns = $state<Map<string, ProjectTaskRunSummary[]>>(new Map());
  schedules = $state<RecurringDefinitionEntry[]>([]);
  scheduleRuns = $state<Map<string, RecurringRunEntry[]>>(new Map());
  details = $state<Map<string, WorkCardDetail>>(new Map());
  errors = $state<Record<string, string>>({});
  loading = $state(false);
  detailsLoading = $state(false);
  cardLimit = $state(24);
  nextCursor = $state<string | null>(null);
  #disposed = false;
  #generation = 0;
  #versions = new Map<string, string>();
  #pendingCards = new Set<string>();
  #pendingSchedules = new Set<string>();
  #pendingProjects = new Set<string>();
  #cardIds = new Set<string>();
  #scheduleVersions = new Map<string, string>();
  constructor(private port: ActivityDataPort) {}

  async refresh(more = false) {
    if (this.#disposed || this.loading) return;
    const generation = this.#generation;
    const current = () => !this.#disposed && generation === this.#generation;
    this.loading = true;
    const error = (key: string, cause: unknown) => {
      if (current()) this.errors = { ...this.errors, [key]: cause instanceof Error ? cause.message : String(cause) };
    };
    try {
      await Promise.all([
        (async () => {
          try {
            this.errors = { ...this.errors, Code: "", "Code runs": "" };
            let cursor = more ? this.nextCursor : null;
            const seen = new Set<string>();
            const catalog = new Map<string, ItemProjection>(more ? this.projects.map((project) => [project.id, project]) : []);
            do {
              const response = await this.port.projects(cursor);
              if (!current()) return;
              for (const item of response.items) {
                // Retain only preview metadata, not the project's full manifests.
                const { id, title, brief, state, human_phase, updated_at, created_at, attempts } = item;
                catalog.set(id, { id, title, brief: brief?.slice(0, 1500), state, human_phase, updated_at, created_at, attempts } as ItemProjection);
              }
              this.nextCursor = response.nextCursor;
              this.projects = [...new Map([...this.projects, ...catalog.values()].map((item) => [item.id, item])).values()];
              cursor = response.nextCursor;
              if (cursor && seen.has(cursor)) throw new Error("Code catalog returned a repeated cursor");
              if (cursor) seen.add(cursor);
            } while (cursor && current());
            if (!current()) return;
            this.projects = [...catalog.values()];
            // The catalog is ordered by ID. Prioritize recent work for run summaries;
            // older projects load their command history when expanded.
            const ordered = [...this.projects].sort((a, b) => Date.parse(b.updated_at || b.created_at || "") - Date.parse(a.updated_at || a.created_at || ""));
            const recent = ordered.filter((project, index) => index < 12 || this.projectRuns.has(project.id));
            await batches(recent, async (project) => {
              if (!current()) return;
              await this.loadProjectRuns(project.id);
            });
          } catch (cause) { error("Code", cause); }
        })(),
        (async () => {
          try {
            const schedules = await this.port.schedules();
            if (!current()) return;
            this.errors = { ...this.errors, Schedules: "" };
            this.schedules = schedules;
            await batches(schedules.filter((schedule) => schedule.last_run_at_utc).slice(0, 24), async (schedule) => {
              if (!current()) return;
              const version = `${schedule.last_run_at_utc}:${schedule.last_run_status}`;
              if (this.#scheduleVersions.get(schedule.recurring_id) === version && schedule.last_run_status !== "running") return;
              const loaded = await this.loadScheduleRuns(schedule.recurring_id);
              if (current() && loaded) this.#scheduleVersions.set(schedule.recurring_id, version);
            });
          } catch (cause) { error("Schedules", cause); }
        })(),
      ]);
    } finally { if (current()) this.loading = false; }
  }

  async loadProjectRuns(id: string) {
    if (this.#disposed || this.#pendingProjects.has(id)) return;
    const generation = this.#generation;
    this.#pendingProjects.add(id);
    try {
      const runs = await this.port.projectRuns(id);
      if (!this.#disposed && generation === this.#generation) this.projectRuns = new Map(this.projectRuns).set(id, runs);
    } catch (cause) {
      if (!this.#disposed && generation === this.#generation) this.errors = { ...this.errors, "Code runs": String(cause) };
    } finally { this.#pendingProjects.delete(id); }
  }

  async loadScheduleRuns(id: string) {
    if (this.#disposed || this.#pendingSchedules.has(id)) return;
    const generation = this.#generation;
    this.#pendingSchedules.add(id);
    try {
      const runs = await this.port.scheduleRuns(id);
      if (!this.#disposed && generation === this.#generation) {
        // Run output is fetched for the selected run, never retained for all schedules.
        this.scheduleRuns = new Map(this.scheduleRuns).set(id, runs.map((run) => ({ ...run, output_text: null })));
        this.errors = { ...this.errors, "Schedule runs": "" };
        return true;
      }
    } catch (cause) {
      if (!this.#disposed && generation === this.#generation) this.errors = { ...this.errors, "Schedule runs": String(cause) };
    } finally { this.#pendingSchedules.delete(id); }
  }

  async loadCards(cards: WorkCard[], cached: ReadonlyMap<string, WorkCardDetail>) {
    if (this.#disposed) return;
    this.#cardIds = new Set(cards.map((card) => card.id));
    const generation = this.#generation;
    const targets = [...cards].sort((a, b) => {
      const priority = (card: WorkCard) => ["in_flight", "wrapping_up", "backlog"].includes(card.column) ? 0 : isActionableBlockedCard(card) ? 1 : card.column === "done" ? 2 : 3;
      return priority(a) - priority(b) || (Date.parse(b.updated_at_utc) || 0) - (Date.parse(a.updated_at_utc) || 0);
    }).slice(0, this.cardLimit).filter((card) => this.#versions.get(card.id) !== card.updated_at_utc && !this.#pendingCards.has(card.id));
    // Reserve the entire batch before awaiting so a stream tick cannot schedule it twice.
    targets.forEach((card) => this.#pendingCards.add(card.id));
    this.detailsLoading = this.#pendingCards.size > 0;
    await batches(targets, async (card) => {
      if (this.#disposed || generation !== this.#generation) return;
      try {
        const existing = cached.get(card.id);
        const detail = existing?.card.updated_at_utc === card.updated_at_utc ? existing : await this.port.card(card.id);
        if (this.#disposed || generation !== this.#generation) return;
        this.details = new Map(this.details).set(card.id, activityDetailSummary(detail));
        this.#versions.set(card.id, card.updated_at_utc);
      } catch {
        // Keep the summary row available even when a single detail has expired.
      } finally {
        this.#pendingCards.delete(card.id);
        if (!this.#disposed && generation === this.#generation) this.detailsLoading = this.#pendingCards.size > 0;
      }
    });
    if (!this.#disposed && generation === this.#generation) {
      this.details = new Map([...this.details].filter(([id]) => this.#cardIds.has(id)));
    }
  }

  dispose() {
    this.#disposed = true;
    this.#generation += 1;
    this.details = new Map();
    this.projectRuns = new Map();
    this.scheduleRuns = new Map();
    this.projects = [];
    this.schedules = [];
  }
}
