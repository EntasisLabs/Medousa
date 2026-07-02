import {
  applyEnvironmentPending,
  dismissEnvironmentPending,
  getEnvironmentPending,
  getEnvironmentSpec,
  putEnvironmentSpec,
  startEnvironmentStream,
  stopEnvironmentStream,
} from "$lib/daemon";
import type {
  ComponentDef,
  EnvironmentPendingProposal,
  EnvironmentSpec,
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

export class EnvironmentStore {
  spec = $state<EnvironmentSpec | null>(null);
  revision = $state(0);
  loading = $state(false);
  streamError = $state<string | null>(null);
  pendingProposal = $state<EnvironmentPendingProposal | null>(null);
  pendingBusy = $state(false);

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
  }

  async refreshPending(profileId?: string): Promise<void> {
    try {
      const response = await getEnvironmentPending(profileId);
      this.pendingProposal = response.pending ?? null;
    } catch {
      this.pendingProposal = null;
    }
  }

  async activatePreset(presetId: string, profileId?: string): Promise<void> {
    const spec = structuredClone(this.spec ?? defaultEnvironmentSpec(profileId));
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
}

export const environment = new EnvironmentStore();

export async function startEnvironmentSync(profileId?: string): Promise<void> {
  await stopEnvironmentStream();
  await startEnvironmentStream(environment.revision || undefined, profileId);
}

export async function stopEnvironmentSync(): Promise<void> {
  await stopEnvironmentStream();
}
