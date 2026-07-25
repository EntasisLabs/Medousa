import { contextThreads } from "$lib/stores/contextThreads.svelte";

export type MapAvecMins = {
  stability: number;
  friction: number;
  logic: number;
  autonomy: number;
};

export const DEFAULT_MAP_AVEC_MINS: MapAvecMins = {
  stability: 0,
  friction: 0,
  logic: 0,
  autonomy: 0,
};

/** Desktop shell bridge for the Map surface (search, selection, AVEC filter). */
export class ContextShellStore {
  search = $state("");
  selectedMapNodeId = $state<string | null>(null);
  /** Rail → canvas: bump nonce + session id to force-expand moments. */
  mapExpandSessionId = $state<string | null>(null);
  mapExpandNonce = $state(0);
  /** Per-dimension minimums (0 = no constraint). */
  mapAvecMins = $state<MapAvecMins>({ ...DEFAULT_MAP_AVEC_MINS });

  selectMapNode(id: string | null) {
    this.selectedMapNodeId = id;
  }

  requestExpandMapSession(sessionId: string) {
    const id = sessionId.trim();
    if (!id) return;
    this.mapExpandSessionId = id;
    this.mapExpandNonce += 1;
  }

  setMapAvecMin(key: keyof MapAvecMins, value: number) {
    const clamped = Math.min(1, Math.max(0, value));
    this.mapAvecMins = { ...this.mapAvecMins, [key]: clamped };
  }

  resetMapAvecMins() {
    this.mapAvecMins = { ...DEFAULT_MAP_AVEC_MINS };
  }

  get mapAvecFilterActive(): boolean {
    const mins = this.mapAvecMins;
    return (
      mins.stability > 0 ||
      mins.friction > 0 ||
      mins.logic > 0 ||
      mins.autonomy > 0
    );
  }

  /** Focus a locus moment on the map and load its detail. */
  focusMapMoment(syncKey: string, sessionId?: string) {
    const key = syncKey.trim();
    if (!key) return;
    this.selectedMapNodeId = `thread:${key}`;
    void contextThreads.loadDetail(key);
    if (sessionId?.trim()) {
      this.requestExpandMapSession(sessionId.trim());
    }
  }
}

export const contextShell = new ContextShellStore();
