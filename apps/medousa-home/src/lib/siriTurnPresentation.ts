const siriOwnedTurns = new Map<string, number>();

export function registerSiriOwnedTurn(turnId: string, waitMs: number): void {
  const id = turnId.trim();
  if (!id) return;
  siriOwnedTurns.set(id, Date.now() + Math.max(0, waitMs));
}

export function shouldSuppressSiriTurnNotification(
  turnId: string,
  now = Date.now(),
): boolean {
  const id = turnId.trim();
  const deadline = siriOwnedTurns.get(id);
  if (deadline == null) return false;
  siriOwnedTurns.delete(id);
  return now <= deadline;
}

export function clearSiriOwnedTurnsForTests(): void {
  siriOwnedTurns.clear();
}
