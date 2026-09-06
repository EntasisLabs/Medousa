/** Unified Browser source state: daemon-owned worlds plus explicit device browser. */

import {
  createIsolatedBrowserWorld,
  inputIsolatedBrowserWorld,
  listIsolatedBrowserWorlds,
  navigateIsolatedBrowserWorld,
  observeIsolatedBrowserWorld,
  screenshotIsolatedBrowserWorld,
  setIsolatedBrowserWorldLifecycle,
  type BrowserHumanInput,
  type BrowserObservation,
  type BrowserScreenshot,
  type IsolatedBrowserWorld,
} from "$lib/daemon/browserWorlds";
import {
  chooseBrowserSurfaceSource,
  type BrowserSourcePreference,
  type BrowserSurfaceSource,
} from "$lib/browser/worldPresentation";
import { resolveBrowserDestination } from "$lib/utils/resolveBrowserDestination";
import { activeWorkshopId, workshopScopedStorageKey } from "$lib/utils/workshopLocality";

const SOURCE_KEY = "medousa-browser-surface-v1";
const IDLE_FRAME_MS = 850;
const ACTIVE_FRAME_MS = 180;
const HIDDEN_FRAME_MS = 2_500;
const ACTIVE_WINDOW_MS = 2_000;
const INITIAL_SCOPE_ID = activeWorkshopId();
const INITIAL_PREFERENCE = readPreference(INITIAL_SCOPE_ID);

type BoundBrowserWorld = IsolatedBrowserWorld & { execution_runtime_id: string };

export interface BrowserExecutionContext {
  runtimeId: string;
  parentRuntimeId: string;
  runtimeLabels: Record<string, string>;
}

function readPreference(
  scopeId: string,
  fallbackRuntimeId?: string | null,
): BrowserSourcePreference | null {
  if (typeof localStorage === "undefined") return null;
  try {
    const raw = localStorage.getItem(workshopScopedStorageKey(SOURCE_KEY, scopeId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as BrowserSourcePreference;
    if (!Number.isFinite(parsed?.selectedAt) || parsed.selectedAt < 0) return null;
    if (parsed.source?.kind === "device") {
      return { source: { kind: "device" }, selectedAt: parsed.selectedAt };
    }
    if (parsed.source?.kind === "workshop" && parsed.source.worldId?.trim()) {
      const runtimeId =
        typeof parsed.source.runtimeId === "string" && parsed.source.runtimeId.trim()
          ? parsed.source.runtimeId.trim()
          : fallbackRuntimeId?.trim();
      if (!runtimeId) return null;
      const browserDriverId =
        typeof parsed.browserDriverId === "string" && parsed.browserDriverId.trim().length <= 256
          ? parsed.browserDriverId.trim()
          : undefined;
      return {
        source: {
          kind: "workshop",
          worldId: parsed.source.worldId.trim(),
          runtimeId,
        },
        selectedAt: parsed.selectedAt,
        browserDriverId,
      };
    }
  } catch {
    // Ignore malformed local presentation state.
  }
  return null;
}

function writePreference(
  scopeId: string,
  source: BrowserSurfaceSource,
  browserDriverId?: string | null,
) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(
    workshopScopedStorageKey(SOURCE_KEY, scopeId),
    JSON.stringify({
      source,
      selectedAt: Date.now(),
      browserDriverId: browserDriverId || undefined,
    } satisfies BrowserSourcePreference),
  );
}

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export class GovernedBrowserStore {
  scopeId = $state(INITIAL_SCOPE_ID);
  source = $state<BrowserSurfaceSource>(INITIAL_PREFERENCE?.source ?? { kind: "device" });
  worlds = $state<BoundBrowserWorld[]>([]);
  targetRuntimeId = $state<string | null>(null);
  parentRuntimeId = $state<string | null>(null);
  runtimeLabels = $state<Record<string, string>>({});
  cachedWorkshopDriverId = $state<string | null>(
    INITIAL_PREFERENCE?.browserDriverId?.trim() || null,
  );
  observation = $state<BrowserObservation | null>(null);
  screenshot = $state<BrowserScreenshot | null>(null);
  urlDraft = $state("");
  loadingWorlds = $state(false);
  creating = $state(false);
  frameLoading = $state(false);
  inputBusy = $state(false);
  error = $state<string | null>(null);

  private loadEpoch = 0;
  private framePromise: Promise<void> | null = null;
  private inputQueue: Promise<void> = Promise.resolve();
  private historyBack: string[] = [];
  private historyForward: string[] = [];
  private fastUntil = 0;

  selectedWorld = $derived.by(() => {
    const source = this.source;
    if (source.kind !== "workshop") return null;
    return (
      this.worlds.find(
        (world) =>
          world.world_id === source.worldId &&
          world.execution_runtime_id === source.runtimeId,
      ) ?? null
    );
  });
  targetWorlds = $derived(
    this.targetRuntimeId
      ? this.worlds.filter(
          (world) => world.execution_runtime_id === this.targetRuntimeId,
        )
      : [],
  );
  usesWorkshopWorld = $derived(this.source.kind === "workshop");
  turnBrowserDriverId = $derived(
    this.source.kind === "workshop"
      ? this.selectedWorld?.driver.driver_id ?? this.cachedWorkshopDriverId
      : null,
  );
  activeUrl = $derived(this.selectedWorld?.url ?? "about:blank");
  activeTitle = $derived(this.selectedWorld?.title?.trim() || "Workshop browser");
  targetRuntimeLabel = $derived(
    this.labelForRuntime(this.targetRuntimeId),
  );
  sourceRuntimeLabel = $derived(
    this.source.kind === "workshop"
      ? this.labelForRuntime(this.source.runtimeId)
      : null,
  );
  canGoBack = $derived(this.historyBack.length > 0);
  canGoForward = $derived(this.historyForward.length > 0);
  frameDataUrl = $derived(
    this.screenshot
      ? `data:${this.screenshot.mime};base64,${this.screenshot.image_base64}`
      : null,
  );

  async load(
    scopeId = activeWorkshopId(),
    context?: BrowserExecutionContext | null,
  ): Promise<void> {
    const epoch = ++this.loadEpoch;
    if (this.scopeId !== scopeId) {
      this.scopeId = scopeId;
      this.worlds = [];
      this.targetRuntimeId = null;
      this.parentRuntimeId = null;
      this.runtimeLabels = {};
      this.source = { kind: "device" };
      this.cachedWorkshopDriverId = null;
      this.clearFrame();
    }
    this.loadingWorlds = true;
    this.error = null;
    try {
      const defaultRuntimeId = context?.runtimeId.trim() || null;
      const preference = readPreference(scopeId, defaultRuntimeId);
      if (!defaultRuntimeId) {
        throw new Error("No authorized workshop advertises an isolated browser driver");
      }
      this.parentRuntimeId = context?.parentRuntimeId.trim() || null;
      this.runtimeLabels = context?.runtimeLabels ?? {};
      this.targetRuntimeId = defaultRuntimeId;
      this.source = preference?.source ?? { kind: "device" };
      this.cachedWorkshopDriverId = preference?.browserDriverId?.trim() || null;
      const sourceRuntimeId =
        preference?.source.kind === "workshop"
          ? preference.source.runtimeId
          : defaultRuntimeId;
      const runtimeIds = [...new Set([defaultRuntimeId, sourceRuntimeId])];
      const loadedRuntimeIds = new Set<string>();
      const failures: string[] = [];
      for (const runtimeId of runtimeIds) {
        try {
          const worlds = await listIsolatedBrowserWorlds(this.transportRuntimeId(runtimeId));
          if (epoch !== this.loadEpoch || scopeId !== this.scopeId) return;
          this.replaceRuntimeWorlds(runtimeId, worlds);
          loadedRuntimeIds.add(runtimeId);
        } catch (error) {
          failures.push(`${this.labelForRuntime(runtimeId)}: ${messageFrom(error)}`);
        }
      }
      if (loadedRuntimeIds.size === 0) {
        throw new Error(failures.join("; "));
      }
      const sourceWorlds = this.worlds.filter(
        (world) => world.execution_runtime_id === sourceRuntimeId,
      );
      const next =
        preference?.source.kind === "workshop" && !loadedRuntimeIds.has(sourceRuntimeId)
          ? preference.source
          : chooseBrowserSurfaceSource(sourceWorlds, preference, sourceRuntimeId);
      this.applySource(next);
      if (loadedRuntimeIds.has(sourceRuntimeId)) {
        writePreference(scopeId, next, this.turnBrowserDriverId);
      }
      if (failures.length > 0) {
        this.error = `Some workshop browsers could not load: ${failures.join("; ")}`;
      }
    } catch (error) {
      if (epoch !== this.loadEpoch || scopeId !== this.scopeId) return;
      this.error = `Could not load workshop browsers: ${messageFrom(error)}`;
    } finally {
      if (epoch === this.loadEpoch) this.loadingWorlds = false;
    }
  }

  async refreshWorlds(): Promise<void> {
    const runtimeId = this.targetRuntimeId;
    if (!runtimeId) return;
    const worlds = await listIsolatedBrowserWorlds(this.transportRuntimeId(runtimeId));
    this.replaceRuntimeWorlds(runtimeId, worlds);
    if (this.source.kind === "workshop" && this.source.runtimeId !== runtimeId) return;
    const next = chooseBrowserSurfaceSource(
      worlds,
      readPreference(this.scopeId, runtimeId),
      runtimeId,
    );
    if (
      next.kind !== this.source.kind ||
      (next.kind === "workshop" &&
        (this.source.kind !== "workshop" ||
          next.worldId !== this.source.worldId ||
          next.runtimeId !== this.source.runtimeId))
    ) {
      this.applySource(next);
      writePreference(this.scopeId, next, this.turnBrowserDriverId);
    }
  }

  async selectTarget(context: BrowserExecutionContext): Promise<void> {
    const id = context.runtimeId.trim();
    if (!id || this.loadingWorlds) return;
    this.parentRuntimeId = context.parentRuntimeId.trim() || null;
    this.runtimeLabels = context.runtimeLabels;
    this.targetRuntimeId = id;
    this.loadingWorlds = true;
    this.error = null;
    try {
      const worlds = await listIsolatedBrowserWorlds(this.transportRuntimeId(id));
      if (this.targetRuntimeId !== id) return;
      this.replaceRuntimeWorlds(id, worlds);
    } catch (error) {
      if (this.targetRuntimeId === id) {
        this.error = `Could not load browsers on ${this.targetRuntimeLabel}: ${messageFrom(error)}`;
      }
    } finally {
      if (this.targetRuntimeId === id) this.loadingWorlds = false;
    }
  }

  chooseDevice() {
    this.source = { kind: "device" };
    this.cachedWorkshopDriverId = null;
    this.clearFrame();
    this.historyBack = [];
    this.historyForward = [];
    writePreference(this.scopeId, this.source);
  }

  async selectWorld(worldId: string, runtimeId = this.targetRuntimeId): Promise<void> {
    if (!runtimeId) return;
    const selected = this.worlds.find(
      (world) =>
        world.world_id === worldId && world.execution_runtime_id === runtimeId,
    );
    if (!selected) return;
    this.error = null;
    this.applySource({ kind: "workshop", worldId, runtimeId });
    writePreference(this.scopeId, this.source, this.turnBrowserDriverId);
    try {
      let world: IsolatedBrowserWorld = selected;
      if (world.run_state !== "running" && world.run_state !== "starting") {
        world = await setIsolatedBrowserWorldLifecycle(
          worldId,
          "resume",
          this.transportRuntimeId(runtimeId),
        );
        this.replaceWorld(world, runtimeId);
      }
      world = await setIsolatedBrowserWorldLifecycle(
        worldId,
        "attach_view",
        this.transportRuntimeId(runtimeId),
      );
      this.replaceWorld(world, runtimeId);
      this.markInteractive();
    } catch (error) {
      this.error = `Could not open this workshop browser: ${messageFrom(error)}`;
    }
  }

  async createWorld(options?: { persistent?: boolean }): Promise<void> {
    if (this.creating) return;
    this.creating = true;
    this.error = null;
    try {
      const runtimeId = this.targetRuntimeId;
      if (!runtimeId) {
        throw new Error("Choose an authorized browser workshop first");
      }
      const profile = options?.persistent
        ? { kind: "persistent" as const, profile_id: `home-${Date.now().toString(36)}` }
        : { kind: "ephemeral" as const };
      const world = await createIsolatedBrowserWorld(
        { profile },
        this.transportRuntimeId(runtimeId),
      );
      this.replaceWorld(world, runtimeId);
      await this.selectWorld(world.world_id, runtimeId);
    } catch (error) {
      this.error = `Could not start a workshop browser: ${messageFrom(error)}`;
    } finally {
      this.creating = false;
    }
  }

  async takeControl(): Promise<void> {
    const world = this.selectedWorld;
    if (!world) return;
    try {
      this.replaceWorld(
        await setIsolatedBrowserWorldLifecycle(
          world.world_id,
          "takeover",
          this.transportRuntimeId(world.execution_runtime_id),
        ),
        world.execution_runtime_id,
      );
      this.markInteractive();
    } catch (error) {
      this.error = messageFrom(error);
    }
  }

  async handBackToAgent(): Promise<void> {
    const world = this.selectedWorld;
    if (!world) return;
    try {
      this.replaceWorld(
        await setIsolatedBrowserWorldLifecycle(
          world.world_id,
          "return_to_agent",
          this.transportRuntimeId(world.execution_runtime_id),
        ),
        world.execution_runtime_id,
      );
    } catch (error) {
      this.error = messageFrom(error);
    }
  }

  async navigate(input: string, options?: { skipHistory?: boolean }): Promise<void> {
    const world = this.selectedWorld;
    if (!world) return;
    const destination = await resolveBrowserDestination(input);
    if (!options?.skipHistory && world.url && world.url !== "about:blank") {
      this.historyBack = [...this.historyBack, world.url];
      this.historyForward = [];
    }
    this.urlDraft = destination;
    this.error = null;
    try {
      this.replaceWorld(
        await navigateIsolatedBrowserWorld(
          world.world_id,
          destination,
          this.transportRuntimeId(world.execution_runtime_id),
        ),
        world.execution_runtime_id,
      );
      this.clearFrame();
      this.markInteractive();
    } catch (error) {
      this.error = `Could not navigate: ${messageFrom(error)}`;
    }
  }

  async goBack(): Promise<void> {
    const current = this.activeUrl;
    const destination = this.historyBack.at(-1);
    if (!destination) return;
    this.historyBack = this.historyBack.slice(0, -1);
    if (current && current !== "about:blank") this.historyForward = [...this.historyForward, current];
    await this.navigate(destination, { skipHistory: true });
  }

  async goForward(): Promise<void> {
    const current = this.activeUrl;
    const destination = this.historyForward.at(-1);
    if (!destination) return;
    this.historyForward = this.historyForward.slice(0, -1);
    if (current && current !== "about:blank") this.historyBack = [...this.historyBack, current];
    await this.navigate(destination, { skipHistory: true });
  }

  async reload(): Promise<void> {
    if (!this.activeUrl || this.activeUrl === "about:blank") return;
    await this.navigate(this.activeUrl, { skipHistory: true });
  }

  async sendInput(input: BrowserHumanInput): Promise<void> {
    this.inputQueue = this.inputQueue.then(async () => {
      const world = this.selectedWorld;
      if (!world || !this.observation) return;
      if (this.framePromise) await this.framePromise;
      const observation = this.observation;
      if (!observation) return;
      this.inputBusy = true;
      this.error = null;
      try {
        this.replaceWorld(
          await inputIsolatedBrowserWorld(
            world.world_id,
            observation,
            input,
            this.transportRuntimeId(world.execution_runtime_id),
          ),
          world.execution_runtime_id,
        );
        this.markInteractive();
      } catch (error) {
        this.error = `The page changed before that input landed: ${messageFrom(error)}`;
        this.observation = null;
      } finally {
        this.inputBusy = false;
      }
    });
    await this.inputQueue;
  }

  startPresentation(worldId: string, maxWidth: () => number): () => void {
    const source = this.source;
    if (source.kind !== "workshop" || source.worldId !== worldId) return () => {};
    const runtimeId = source.runtimeId;
    let active = true;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const loop = async () => {
      if (
        !active ||
        this.source.kind !== "workshop" ||
        this.source.worldId !== worldId ||
        this.source.runtimeId !== runtimeId
      ) return;
      await this.refreshFrame(maxWidth());
      if (!active) return;
      const hidden = typeof document !== "undefined" && document.hidden;
      const delay = hidden
        ? HIDDEN_FRAME_MS
        : Date.now() < this.fastUntil
          ? ACTIVE_FRAME_MS
          : IDLE_FRAME_MS;
      timer = setTimeout(loop, delay);
    };

    void setIsolatedBrowserWorldLifecycle(
      worldId,
      "attach_view",
      this.transportRuntimeId(runtimeId),
    )
      .then((world) => this.replaceWorld(world, runtimeId))
      .catch((error) => {
        this.error = `Could not attach browser view: ${messageFrom(error)}`;
      })
      .finally(() => void loop());

    return () => {
      active = false;
      if (timer) clearTimeout(timer);
      void setIsolatedBrowserWorldLifecycle(
        worldId,
        "detach_view",
        this.transportRuntimeId(runtimeId),
      )
        .then((world) => this.replaceWorld(world, runtimeId))
        .catch(() => undefined);
    };
  }

  markInteractive() {
    this.fastUntil = Date.now() + ACTIVE_WINDOW_MS;
  }

  private async refreshFrame(maxWidth: number): Promise<void> {
    if (this.framePromise) return this.framePromise;
    const world = this.selectedWorld;
    if (!world || world.run_state !== "running") return;
    const worldId = world.world_id;
    const runtimeId = world.execution_runtime_id;
    this.framePromise = (async () => {
      this.frameLoading = !this.screenshot;
      try {
        const observation = await observeIsolatedBrowserWorld(
          worldId,
          this.observation?.revision,
          this.transportRuntimeId(runtimeId),
        );
        const screenshot = await screenshotIsolatedBrowserWorld(
          worldId,
          observation,
          Math.max(320, Math.min(1280, Math.round(maxWidth || 960))),
          this.transportRuntimeId(runtimeId),
        );
        if (
          this.source.kind !== "workshop" ||
          this.source.worldId !== worldId ||
          this.source.runtimeId !== runtimeId
        ) return;
        this.observation = observation;
        this.screenshot = screenshot;
        this.urlDraft = observation.url === "about:blank" ? "" : observation.url;
        this.patchWorldIdentity(runtimeId, worldId, observation.url, observation.title);
        this.error = null;
      } catch (error) {
        if (
          this.source.kind === "workshop" &&
          this.source.worldId === worldId &&
          this.source.runtimeId === runtimeId
        ) {
          this.error = `Workshop browser view paused: ${messageFrom(error)}`;
        }
      } finally {
        this.frameLoading = false;
        this.framePromise = null;
      }
    })();
    return this.framePromise;
  }

  private applySource(source: BrowserSurfaceSource) {
    const changed =
      this.source.kind !== source.kind ||
      (this.source.kind === "workshop" &&
        source.kind === "workshop" &&
        (this.source.worldId !== source.worldId ||
          this.source.runtimeId !== source.runtimeId));
    this.source = source;
    if (changed) {
      this.clearFrame();
      this.historyBack = [];
      this.historyForward = [];
    }
    const world =
      source.kind === "workshop"
        ? this.worlds.find(
            (candidate) =>
              candidate.world_id === source.worldId &&
              candidate.execution_runtime_id === source.runtimeId,
          )
        : null;
    this.cachedWorkshopDriverId =
      source.kind === "workshop"
        ? world?.driver.driver_id ?? (changed ? null : this.cachedWorkshopDriverId)
        : null;
    this.urlDraft = world?.url === "about:blank" ? "" : world?.url ?? "";
  }

  private replaceWorld(world: IsolatedBrowserWorld, runtimeId: string) {
    const bound = { ...world, execution_runtime_id: runtimeId };
    const existing = this.worlds.some(
      (candidate) =>
        candidate.world_id === world.world_id &&
        candidate.execution_runtime_id === runtimeId,
    );
    this.worlds = existing
      ? this.worlds.map((candidate) =>
          candidate.world_id === world.world_id &&
          candidate.execution_runtime_id === runtimeId
            ? bound
            : candidate,
        )
      : [...this.worlds, bound];
  }

  private replaceRuntimeWorlds(runtimeId: string, worlds: IsolatedBrowserWorld[]) {
    this.worlds = [
      ...this.worlds.filter((world) => world.execution_runtime_id !== runtimeId),
      ...worlds.map((world) => ({ ...world, execution_runtime_id: runtimeId })),
    ];
  }

  private patchWorldIdentity(runtimeId: string, worldId: string, url: string, title: string) {
    this.worlds = this.worlds.map((world) =>
      world.world_id === worldId && world.execution_runtime_id === runtimeId
        ? { ...world, url, title }
        : world,
    );
  }

  private transportRuntimeId(runtimeId: string): string | null {
    return runtimeId === this.parentRuntimeId ? null : runtimeId;
  }

  private labelForRuntime(runtimeId?: string | null): string {
    const id = runtimeId?.trim();
    if (!id) return "Unavailable workshop";
    const label = this.runtimeLabels[id]?.trim();
    if (label) return label;
    if (id.length <= 24) return id;
    return `${id.slice(0, 10)}…${id.slice(-6)}`;
  }

  private clearFrame() {
    this.observation = null;
    this.screenshot = null;
  }
}

export const governedBrowser = new GovernedBrowserStore();
