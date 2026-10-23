import { creativeApi } from "./creativeApi";
import { starterScene, type CreativeStarter } from "./creativeScene";
import { randomUuid } from "$lib/utils/randomUuid";
import { loadCreativeRailPreferences, type CreativeRailPreferences } from "./creativeRailPreferences";
import type {
  CreativeCreateRequest,
  CreativeProjectSummary,
  CreativeDocument,
} from "$lib/types/generated/daemon_api";
export class CreateBrowser {
  projects = $state.raw<CreativeProjectSummary[]>([]);
  loading = $state(false);
  creating = $state(false);
  error = $state<string | null>(null);
  cursor = $state<string | null>(null);
  pending: CreativeCreateRequest | null = null;
  rail = $state<CreativeRailPreferences>({ sort: "recent", pinnedOpen: true, recentOpen: true, scrollTop: 0 });
  private changes = 0;
  private changed = new Map<string, number>();
  private removed = new Set<string>();
  private epoch = 0;
  private api;
  constructor(readonly scope: string) {
    this.api = creativeApi(scope);
    this.rail = loadCreativeRailPreferences(scope);
  }
  acknowledge(document: CreativeDocument) {
    if (this.removed.has(document.id)) return;
    const summary: CreativeProjectSummary = {
      id: document.id, title: document.title, pinned: document.pinned,
      revision: document.revision, updated_at: document.updated_at,
    };
    const current = this.projects.find((p) => p.id === document.id);
    if (current && current.revision > summary.revision) return;
    if (current && JSON.stringify(current) === JSON.stringify(summary)) return;
    this.changed.set(document.id, ++this.changes);
    this.projects = [summary, ...this.projects.filter((p) => p.id !== document.id)];
  }
  forget(id: string) {
    this.removed.add(id);
    this.changed.set(id, ++this.changes);
    this.projects = this.projects.filter((p) => p.id !== id);
  }
  async load(more = false) {
    if (this.loading) return;
    const generation = ++this.epoch;
    const before = this.changes;
    this.loading = true;
    this.error = null;
    try {
      const result = await this.api.list(
        more ? (this.cursor ?? undefined) : undefined,
      );
      if (generation !== this.epoch) return;
      const byId = new Map((more ? this.projects : []).map((p) => [p.id, p]));
      result.projects.forEach((p) => {
        const current = this.projects.find((entry) => entry.id === p.id);
        if (!this.removed.has(p.id)) byId.set(p.id, current && current.revision > p.revision ? current : p);
      });
      // A read admitted before a local save/delete must not undo its receipt.
      this.projects.forEach((p) => {
        if ((this.changed.get(p.id) ?? 0) > before && !this.removed.has(p.id)) byId.set(p.id, p);
      });
      this.removed.forEach((id) => byId.delete(id));
      this.projects = [...byId.values()];
      this.cursor = result.next_cursor ?? null;
    } catch (e) {
      if (generation === this.epoch)
        this.error = e instanceof Error ? e.message : String(e);
    } finally {
      if (generation === this.epoch) this.loading = false;
    }
  }
  async create(starter: CreativeStarter) {
    if (this.creating) throw new Error("A project is already being created");
    this.pending ??= {
      id: randomUuid(),
      title:
        starter === "blank"
          ? "Untitled canvas"
          : starter === "screenshot"
            ? "Screenshot markup"
            : starter === "diagram"
              ? "Diagram"
              : starter === "concept"
                ? "UI concept"
                : "Image composition",
      scene: starterScene(starter),
    };
    this.creating = true;
    this.error = null;
    try {
      const doc = await this.api.create(this.pending);
      this.pending = null;
      this.acknowledge(doc);
      return doc;
    } catch (e) {
      this.error = e instanceof Error ? e.message : String(e);
      throw e;
    } finally {
      this.creating = false;
    }
  }
}
const browsers = new Map<string, CreateBrowser>();
export function createBrowser(scope: string) {
  let browser = browsers.get(scope);
  if (!browser) {
    if (browsers.size >= 8) browsers.delete(browsers.keys().next().value!);
    browser = new CreateBrowser(scope);
    browsers.set(scope, browser);
  }
  return browser;
}
