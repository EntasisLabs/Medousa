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
import { defaultEnvironmentSpec } from "$lib/utils/environmentDefault";
import { mainComponentsForSurface, resolveLayoutRoot } from "$lib/utils/layoutPresentation";
import type { LayoutNode } from "$lib/types/environment";

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

  navSurfaces(): SurfaceDef[] {
    const spec = this.spec ?? defaultEnvironmentSpec();
    const preset = spec.layoutPresets?.find((entry) => entry.active);
    const orderedIds = preset?.surfaces ?? spec.surfaces.map((surface) => surface.id);
    const byId = new Map(spec.surfaces.map((surface) => [surface.id, surface]));
    const ordered = orderedIds
      .map((id) => byId.get(id))
      .filter((surface): surface is SurfaceDef => Boolean(surface));

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
      await this.refreshPending(profileId);
      this.streamError = null;
    } catch (err) {
      this.spec = defaultEnvironmentSpec(profileId);
      this.revision = 0;
      this.streamError =
        err instanceof Error ? err.message : "Could not load environment spec";
    } finally {
      this.loading = false;
    }
  }

  applyEvent(event: EnvironmentStreamEvent) {
    this.revision = event.revision;
    this.streamError = null;
    if (event.spec) {
      this.spec = event.spec;
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
    this.spec = spec;
    this.revision = revision;
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
}

export const environment = new EnvironmentStore();

export async function startEnvironmentSync(profileId?: string): Promise<void> {
  await stopEnvironmentStream();
  await startEnvironmentStream(environment.revision || undefined, profileId);
}

export async function stopEnvironmentSync(): Promise<void> {
  await stopEnvironmentStream();
}
