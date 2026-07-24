import {
  applyEnvironmentPending,
  dismissEnvironmentPending,
  getEnvironmentPending,
  getEnvironmentSpec,
  getEnvironmentStatus,
  putEnvironmentSpec,
  startEnvironmentStream,
  stopEnvironmentStream,
} from "$lib/daemon";
import type {
  ComponentDef,
  EnvironmentPendingProposal,
  EnvironmentSpec,
  EnvironmentStatusResponse,
  EnvironmentStreamEvent,
  SurfaceDef,
} from "$lib/types/environment";
import {
  activateLayoutPreset,
} from "$lib/utils/environmentLayout";
import {
  SAFETY_SURFACE_RUNTIME,
  SAFETY_SURFACE_SETTINGS,
} from "$lib/types/environment";
import {
  defaultEnvironmentSpec,
  ensureCalendarSurfaceInSpec,
  ensurePeersSurfaceInSpec,
} from "$lib/utils/environmentDefault";
import {
  resolveDesktopShellChrome,
  seedDesktopShellChromeFromPreferredMode,
} from "$lib/utils/desktopEnvironmentChrome";
import { mainComponentsForSurface, resolveLayoutRoot } from "$lib/utils/layoutPresentation";
import type { LayoutNode, ShellChromeDesktop } from "$lib/types/environment";
import type { PreferredMode } from "$lib/utils/preferredMode";
import { layout } from "$lib/stores/layout.svelte";

function migrateBuiltinNavSurfaces(spec: EnvironmentSpec): EnvironmentSpec {
  return ensureCalendarSurfaceInSpec(ensurePeersSurfaceInSpec(spec));
}

function peersNavMigrationNeeded(
  original: EnvironmentSpec,
  migrated: EnvironmentSpec,
): boolean {
  const originalIds = original.surfaces.map((surface) => surface.id).join("\0");
  const migratedIds = migrated.surfaces.map((surface) => surface.id).join("\0");
  if (originalIds !== migratedIds) return true;
  const originalPresets = JSON.stringify(
    (original.layoutPresets ?? []).map((preset) => [preset.id, preset.surfaces]),
  );
  const migratedPresets = JSON.stringify(
    (migrated.layoutPresets ?? []).map((preset) => [preset.id, preset.surfaces]),
  );
  return originalPresets !== migratedPresets;
}

export class EnvironmentStore {
  spec = $state<EnvironmentSpec | null>(null);
  revision = $state(0);
  loading = $state(false);
  streamError = $state<string | null>(null);
  pendingProposal = $state<EnvironmentPendingProposal | null>(null);
  pendingBusy = $state(false);
  feedStateByComponentId = $state<Map<string, Record<string, unknown>>>(new Map());
  pendingRuntimeProbes = $state<Map<string, import("$lib/types/environment").ComponentRuntimeProbeRequest>>(
    new Map(),
  );
  canvasStatus = $state<EnvironmentStatusResponse | null>(null);
  canvasStatusError = $state<string | null>(null);
  canvasStatusLoading = $state(false);
  private peersMigrationPersisted = false;

  get loaded(): boolean {
    return this.spec !== null;
  }

  get shellChrome() {
    return this.spec?.shellChrome ?? defaultEnvironmentSpec().shellChrome;
  }

  get mobileDefaultHome(): string {
    return this.shellChrome?.mobile?.defaultHome ?? "home";
  }

  get mobileAskEntry() {
    return this.shellChrome?.mobile?.askEntry ?? "inline";
  }

  get desktopShellChrome() {
    return resolveDesktopShellChrome(this.spec);
  }

  navSurfaces(): SurfaceDef[] {
    const spec = migrateBuiltinNavSurfaces(this.spec ?? defaultEnvironmentSpec());
    const preset =
      spec.layoutPresets?.find((entry) => entry.active) ??
      spec.layoutPresets?.find((entry) => entry.id === spec.activePresetId);
    const orderedIds = preset?.surfaces ?? spec.surfaces.map((surface) => surface.id);
    const byId = new Map(spec.surfaces.map((surface) => [surface.id, surface]));
    const ordered = orderedIds
      .map((id) => byId.get(id))
      .filter((surface): surface is SurfaceDef => Boolean(surface));

    // Peers is a Life destination — always surface it next to Chat when present on the spec.
    const peers = byId.get("peers");
    if (peers && !ordered.some((surface) => surface.id === "peers")) {
      const chatAt = ordered.findIndex((surface) => surface.id === "chat");
      ordered.splice(chatAt >= 0 ? chatAt + 1 : 0, 0, peers);
    } else if (peers) {
      const withoutPeers = ordered.filter((surface) => surface.id !== "peers");
      const chatAt = withoutPeers.findIndex((surface) => surface.id === "chat");
      withoutPeers.splice(chatAt >= 0 ? chatAt + 1 : 0, 0, peers);
      ordered.splice(0, ordered.length, ...withoutPeers);
    }

    // Automations is Library's twin door — keep it available even when older
    // presets dropped it after modes were folded under Library.
    const automations = byId.get("automations");
    const libraryAt = ordered.findIndex((surface) => surface.id === "library");
    if (automations && libraryAt >= 0 && !ordered.some((surface) => surface.id === "automations")) {
      ordered.splice(libraryAt + 1, 0, automations);
    }

    for (const safetyId of [SAFETY_SURFACE_SETTINGS, SAFETY_SURFACE_RUNTIME]) {
      if (!ordered.some((surface) => surface.id === safetyId)) {
        const safety = byId.get(safetyId);
        if (safety) ordered.push(safety);
      }
    }
    return ordered;
  }

  surfaceById(surfaceId: string): SurfaceDef | null {
    const spec = this.spec ?? defaultEnvironmentSpec();
    return spec.surfaces.find((surface) => surface.id === surfaceId) ?? null;
  }

  isCustomSurface(surfaceId: string): boolean {
    const surface = this.surfaceById(surfaceId);
    return surface?.kind === "custom";
  }

  componentsForSurface(surfaceId: string, slot?: string): ComponentDef[] {
    const spec = this.spec ?? defaultEnvironmentSpec();
    return spec.components.filter((component) => {
      if (component.surfaceId !== surfaceId) return false;
      if (slot && component.slot !== slot) return false;
      return true;
    });
  }

  mainComponentsForSurface(surfaceId: string): ComponentDef[] {
    const spec = this.spec ?? defaultEnvironmentSpec();
    return mainComponentsForSurface(surfaceId, spec.components);
  }

  layoutRootForSurface(surfaceId: string): LayoutNode | null {
    const surface = this.surfaceById(surfaceId);
    if (!surface) return null;
    const spec = this.spec ?? defaultEnvironmentSpec();
    return resolveLayoutRoot(surface, spec.components);
  }

  mobileTabSurfaces(): SurfaceDef[] {
    return this.navSurfaces().filter((surface) => Boolean(surface.mobileTab));
  }

  async load(profileId?: string): Promise<void> {
    this.loading = true;
    try {
      const response = await getEnvironmentSpec(profileId);
      this.applySpec(response.spec, response.revision);
      await this.persistPeersMigrationIfNeeded(response.spec);
      await this.seedDesktopChromeFromPreferredModeIfNeeded(profileId);
      this.syncDesktopChromeToLayout();
      await this.refreshPending(profileId);
      this.streamError = null;
    } catch (err) {
      this.spec = migrateBuiltinNavSurfaces(defaultEnvironmentSpec(profileId));
      this.revision = 0;
      this.streamError =
        err instanceof Error ? err.message : "Could not load environment spec";
    } finally {
      this.loading = false;
    }
  }

  /** Soft seed when preferred mode is known and desktop chrome fields are still unset. */
  private async seedDesktopChromeFromPreferredModeIfNeeded(
    profileId?: string,
  ): Promise<void> {
    const { loadPreferredMode } = await import("$lib/utils/preferredMode");
    const mode = loadPreferredMode();
    if (!mode) return;
    await this.seedDesktopChromeFromPreferredMode(mode, profileId);
  }

  applyEvent(event: EnvironmentStreamEvent) {
    this.revision = event.revision;
    this.streamError = null;
    if (event.spec) {
      this.applySpec(event.spec, event.revision);
      void this.persistPeersMigrationIfNeeded(event.spec);
    }
    if (event.componentPatches?.length) {
      const next = new Map(this.feedStateByComponentId);
      for (const patch of event.componentPatches) {
        next.set(patch.componentId, patch.patch);
      }
      this.feedStateByComponentId = next;
    }
    if (event.runtimeProbe?.componentId) {
      const next = new Map(this.pendingRuntimeProbes);
      next.set(event.runtimeProbe.componentId, event.runtimeProbe);
      this.pendingRuntimeProbes = next;
    }
  }

  clearRuntimeProbe(componentId: string) {
    const next = new Map(this.pendingRuntimeProbes);
    next.delete(componentId);
    this.pendingRuntimeProbes = next;
  }

  feedStateForComponent(componentId: string): Record<string, unknown> | null {
    return this.feedStateByComponentId.get(componentId) ?? null;
  }

  applySpec(spec: EnvironmentSpec, revision: number) {
    this.spec = migrateBuiltinNavSurfaces(spec);
    this.revision = revision;
  }

  /**
   * Apply persisted desktop chrome defaults onto session layout flags.
   * Called on load / Your-space edits — not on every stream applySpec, so a
   * temporary expand while mode is collapsed is not immediately wiped.
   */
  private syncDesktopChromeToLayout() {
    const chrome = resolveDesktopShellChrome(this.spec);
    if (chrome.activityRail === "collapsed" || chrome.activityRail === "hidden") {
      layout.setActivityCollapsed(true);
    }
    if (chrome.vaultSidebar === "hidden") {
      layout.setVaultSidebarCollapsed(true);
    }
    // Only apply when explicitly set — don't clobber a session expand with the default.
    const explicitNav = this.spec?.shellChrome?.desktop?.navStyle;
    if (explicitNav === "rail") {
      layout.setNavExpanded(true);
    } else if (explicitNav === "compact") {
      layout.setNavExpanded(false);
    }
  }

  /** Persist Peers into the daemon so rail + Canvas settings stay in sync after reload. */
  private async persistPeersMigrationIfNeeded(original: EnvironmentSpec): Promise<void> {
    if (this.peersMigrationPersisted || !this.spec) return;
    if (!peersNavMigrationNeeded(original, this.spec)) {
      this.peersMigrationPersisted = true;
      return;
    }
    this.peersMigrationPersisted = true;
    try {
      await this.saveSpec(this.spec);
    } catch {
      this.peersMigrationPersisted = false;
    }
  }

  setError(message: string) {
    this.streamError = message;
  }

  resetForReconnect() {
    this.spec = null;
    this.revision = 0;
    this.streamError = null;
    this.pendingProposal = null;
    this.feedStateByComponentId = new Map();
    this.pendingRuntimeProbes = new Map();
  }

  async refreshPending(profileId?: string): Promise<void> {
    try {
      const response = await getEnvironmentPending(profileId);
      this.pendingProposal = response.pending ?? null;
    } catch {
      this.pendingProposal = null;
    }
  }

  async refreshCanvasStatus(profileId?: string): Promise<void> {
    this.canvasStatusLoading = true;
    try {
      this.canvasStatus = await getEnvironmentStatus(profileId, undefined, {
        includeRuntime: true,
      });
      this.canvasStatusError = null;
    } catch (err) {
      this.canvasStatus = null;
      this.canvasStatusError =
        err instanceof Error ? err.message : "Could not load canvas status";
    } finally {
      this.canvasStatusLoading = false;
    }
  }

  canvasStatusForSurface(surfaceId: string) {
    return this.canvasStatus?.customSurfaces.find(
      (surface) => surface.surfaceId === surfaceId,
    );
  }

  async activatePreset(presetId: string, profileId?: string): Promise<void> {
    const { cloneEnvironmentSpec } = await import("$lib/utils/environmentCanvasOps");
    const spec = cloneEnvironmentSpec(this.spec ?? defaultEnvironmentSpec(profileId));
    activateLayoutPreset(spec, presetId);
    const response = await putEnvironmentSpec({ spec });
    this.applySpec(response.spec, response.revision);
  }

  async setSurfaceNavVisible(
    surfaceId: string,
    visible: boolean,
    profileId?: string,
  ): Promise<void> {
    const { setSurfaceNavVisible: applyNavVisibility } = await import(
      "$lib/utils/environmentLayout"
    );
    const spec = await this.cloneCurrentSpec(profileId);
    applyNavVisibility(spec, surfaceId, visible);
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
  }

  async setMobileDefaultHome(surfaceId: string, profileId?: string): Promise<void> {
    const { setMobileDefaultHome: applyMobileHome } = await import(
      "$lib/utils/environmentCanvasOps"
    );
    const spec = await this.cloneCurrentSpec(profileId);
    applyMobileHome(spec, surfaceId);
    await this.saveSpec(spec);
  }

  async patchShellChromeDesktop(
    patch: Partial<ShellChromeDesktop>,
    profileId?: string,
  ): Promise<void> {
    const { setDesktopShellChrome } = await import("$lib/utils/environmentCanvasOps");
    const spec = await this.cloneCurrentSpec(profileId);
    setDesktopShellChrome(spec, patch);
    await this.saveSpec(spec);
    if (patch.activityRail === "visible") {
      layout.setActivityCollapsed(false);
    } else if (patch.activityRail === "collapsed" || patch.activityRail === "hidden") {
      layout.setActivityCollapsed(true);
    }
    if (patch.vaultSidebar === "visible") {
      layout.setVaultSidebarCollapsed(false);
    } else if (patch.vaultSidebar === "hidden") {
      layout.setVaultSidebarCollapsed(true);
    }
    if (patch.navStyle === "rail") {
      layout.setNavExpanded(true);
    } else if (patch.navStyle === "compact") {
      layout.setNavExpanded(false);
    }
  }

  /** Seed desktop chrome from preferred mode when fields are still unset. */
  async seedDesktopChromeFromPreferredMode(
    mode: PreferredMode,
    profileId?: string,
  ): Promise<void> {
    const spec = await this.cloneCurrentSpec(profileId);
    if (!seedDesktopShellChromeFromPreferredMode(spec, mode)) return;
    await this.saveSpec(spec);
    this.syncDesktopChromeToLayout();
  }

  async applyPendingProposal(profileId?: string): Promise<void> {
    this.pendingBusy = true;
    try {
      const response = await applyEnvironmentPending(profileId);
      this.applySpec(response.spec, response.revision);
      this.pendingProposal = null;
    } finally {
      this.pendingBusy = false;
    }
  }

  async dismissPendingProposal(profileId?: string): Promise<void> {
    this.pendingBusy = true;
    try {
      await dismissEnvironmentPending(profileId);
      this.pendingProposal = null;
    } finally {
      this.pendingBusy = false;
    }
  }

  async saveSpec(spec: EnvironmentSpec): Promise<void> {
    const { cloneEnvironmentSpec } = await import("$lib/utils/environmentCanvasOps");
    const response = await putEnvironmentSpec({ spec: cloneEnvironmentSpec(spec) });
    this.applySpec(response.spec, response.revision);
  }

  async cloneCurrentSpec(profileId?: string): Promise<EnvironmentSpec> {
    const { cloneEnvironmentSpec } = await import("$lib/utils/environmentCanvasOps");
    return cloneEnvironmentSpec(this.spec ?? defaultEnvironmentSpec(profileId));
  }

  async removeCustomSurface(surfaceId: string, profileId?: string): Promise<void> {
    const spec = await this.cloneCurrentSpec(profileId);
    const surface = spec.surfaces.find((entry) => entry.id === surfaceId);
    if (!surface || surface.kind !== "custom") {
      throw new Error("Only custom views can be removed from Settings.");
    }
    const { removeCustomSurfaceFromSpec } = await import("$lib/utils/environmentCanvasOps");
    removeCustomSurfaceFromSpec(spec, surfaceId);
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
  }

  async removePresentationComponent(componentId: string, profileId?: string): Promise<void> {
    const spec = await this.cloneCurrentSpec(profileId);
    const { removeComponentFromSpec } = await import("$lib/utils/environmentCanvasOps");
    removeComponentFromSpec(spec, componentId);
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
  }

  async unlinkComponentsForArtifacts(
    artifactIds: string[],
    profileId?: string,
  ): Promise<string[]> {
    if (artifactIds.length === 0) return [];
    const spec = await this.cloneCurrentSpec(profileId);
    const { removeComponentsReferencingArtifacts } = await import(
      "$lib/utils/environmentCanvasOps"
    );
    const removed = removeComponentsReferencingArtifacts(spec, artifactIds);
    if (removed.length > 0) {
      await this.saveSpec(spec);
      await this.refreshCanvasStatus(profileId);
    }
    return removed;
  }

  async updatePresentationArtifactId(
    componentId: string,
    artifactId: string,
    profileId?: string,
  ): Promise<void> {
    const spec = await this.cloneCurrentSpec(profileId);
    const { updateComponentArtifactId } = await import("$lib/utils/environmentCanvasOps");
    updateComponentArtifactId(spec, componentId, artifactId);
    await this.saveSpec(spec);
  }

  async addCustomView(
    input: {
      id: string;
      label: string;
      icon: string;
      layout?: import("$lib/types/environment").SurfaceLayout;
      presetId?: string | null;
      afterSurfaceId?: string | null;
    },
    profileId?: string,
  ): Promise<string> {
    const spec = await this.cloneCurrentSpec(profileId);
    const { addCustomSurfaceToSpec } = await import("$lib/utils/environmentCanvasOps");
    addCustomSurfaceToSpec(spec, input);
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
    const { slugifyCanvasId } = await import("$lib/utils/environmentCanvasOps");
    return slugifyCanvasId(input.id);
  }

  async updateCustomView(
    input: { surfaceId: string; label?: string; icon?: string },
    profileId?: string,
  ): Promise<void> {
    const spec = await this.cloneCurrentSpec(profileId);
    const { updateCustomSurfaceInSpec } = await import("$lib/utils/environmentCanvasOps");
    updateCustomSurfaceInSpec(spec, input.surfaceId, {
      label: input.label,
      icon: input.icon,
    });
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
  }

  async addLayoutPreset(
    input: { label: string; id?: string | null },
    profileId?: string,
  ): Promise<string> {
    const spec = await this.cloneCurrentSpec(profileId);
    const { addLayoutPresetFromActive } = await import("$lib/utils/environmentLayout");
    const presetId = addLayoutPresetFromActive(spec, input);
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
    return presetId;
  }

  async removeLayoutPreset(presetId: string, profileId?: string): Promise<void> {
    const spec = await this.cloneCurrentSpec(profileId);
    const { removeLayoutPreset } = await import("$lib/utils/environmentLayout");
    removeLayoutPreset(spec, presetId);
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
  }

  async addPresentationFromArtifact(
    input: {
      surfaceId: string;
      artifactId: string;
      label: string;
      componentId?: string | null;
    },
    profileId?: string,
  ): Promise<string> {
    const spec = await this.cloneCurrentSpec(profileId);
    const { addPresentationComponentToSpec } = await import("$lib/utils/environmentCanvasOps");
    const component = addPresentationComponentToSpec(spec, input);
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
    return component.id;
  }

  async addMediaEmbedWidget(
    input: {
      surfaceId: string;
      provider: import("$lib/utils/mediaEmbed").MediaEmbedProvider;
      embedUrl: string;
      label: string;
      componentId?: string | null;
    },
    profileId?: string,
  ): Promise<string> {
    const spec = await this.cloneCurrentSpec(profileId);
    const { addMediaEmbedComponentToSpec } = await import("$lib/utils/environmentCanvasOps");
    const component = addMediaEmbedComponentToSpec(spec, input);
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
    return component.id;
  }

  async addMedousaViewFromNote(
    input: {
      surfaceId: string;
      notePath: string;
      label?: string | null;
      componentId?: string | null;
    },
    profileId?: string,
  ): Promise<string> {
    const spec = await this.cloneCurrentSpec(profileId);
    const { addMedousaViewComponentToSpec } = await import("$lib/utils/environmentCanvasOps");
    const component = addMedousaViewComponentToSpec(spec, input);
    await this.saveSpec(spec);
    await this.refreshCanvasStatus(profileId);
    return component.id;
  }
}

export const environment = new EnvironmentStore();

export async function startEnvironmentSync(profileId?: string): Promise<void> {
  await stopEnvironmentStream();
  await startEnvironmentStream(environment.revision || undefined, profileId);
}

export async function stopEnvironmentSync(): Promise<void> {
  await stopEnvironmentStream();
}
