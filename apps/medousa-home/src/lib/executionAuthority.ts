/**
 * Client-side transport pin for the active Coder undertaking.
 *
 * This is deliberately independent from the mutable active-workshop setting:
 * once a project is bound, reconnecting any of its surfaces must keep using
 * the daemon that authored the undertaking.
 */
let coderExecutionRuntimeId: string | null = null;

export function setCoderExecutionTransport(runtimeId?: string | null): void {
  coderExecutionRuntimeId = runtimeId?.trim() || null;
}

export function getCoderExecutionTransport(): string | null {
  return coderExecutionRuntimeId;
}
