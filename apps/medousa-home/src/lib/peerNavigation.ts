/** One-shot navigation target when opening Peers from a notification tap. */
let pendingPeerWorkshopId: string | null = null;

export function setPendingPeerNavigation(workshopId: string): void {
  const trimmed = workshopId.trim();
  pendingPeerWorkshopId = trimmed || null;
}

export function consumePendingPeerNavigation(): string | null {
  const id = pendingPeerWorkshopId;
  pendingPeerWorkshopId = null;
  return id;
}
