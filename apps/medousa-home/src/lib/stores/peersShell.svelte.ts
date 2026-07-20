import type { PeerConversationRow } from "$lib/utils/peerHomePreview";

/** Desktop shell sidebar ↔ PeersPanel bridge (list lives in shared rail). */
export class PeersShellStore {
  selectedPeerId = $state<string | null>(null);
  peopleQuery = $state("");
  rows = $state<PeerConversationRow[]>([]);
  nearbyUntrustedCount = $state(0);
  hasPeers = $state(false);
  showPeopleSearch = $state(false);
  onSelectPeer: ((id: string) => void) | null = null;
  onAddPeer: (() => void) | null = null;

  selectPeer(id: string) {
    this.selectedPeerId = id;
    this.onSelectPeer?.(id);
  }

  requestAddPeer() {
    this.onAddPeer?.();
  }

  publish(input: {
    rows: PeerConversationRow[];
    nearbyUntrustedCount: number;
    hasPeers: boolean;
    showPeopleSearch: boolean;
    selectedPeerId: string | null;
  }) {
    this.rows = input.rows;
    this.nearbyUntrustedCount = input.nearbyUntrustedCount;
    this.hasPeers = input.hasPeers;
    this.showPeopleSearch = input.showPeopleSearch;
    this.selectedPeerId = input.selectedPeerId;
  }
}

export const peersShell = new PeersShellStore();
