import { creativeApi } from "./creativeApi";
import { starterScene, type CreativeStarter } from "./creativeScene";
import { randomUuid } from "$lib/utils/randomUuid";
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
  private epoch = 0;
  private api;
  constructor(readonly scope: string) {
    this.api = creativeApi(scope);
  }
  acknowledge(document: CreativeDocument) {
    const summary: CreativeProjectSummary = {
      id: document.id, title: document.title, pinned: document.pinned,
      revision: document.revision, updated_at: document.updated_at,
    };
    const current = this.projects.find((p) => p.id === document.id);
    if (current && JSON.stringify(current) === JSON.stringify(summary)) return;
    this.projects = [summary, ...this.projects.filter((p) => p.id !== document.id)];
  }
  async load(more = false) {
    if (this.loading) return;
    const generation = ++this.epoch;
    this.loading = true;
    this.error = null;
    try {
      const result = await this.api.list(
        more ? (this.cursor ?? undefined) : undefined,
      );
      if (generation !== this.epoch) return;
      const byId = new Map((more ? this.projects : []).map((p) => [p.id, p]));
      result.projects.forEach((p) => byId.set(p.id, p));
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
      this.projects = [
        {
          id: doc.id,
          title: doc.title,
          pinned: doc.pinned,
          revision: doc.revision,
          updated_at: doc.updated_at,
        },
        ...this.projects.filter((p) => p.id !== doc.id),
      ];
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
