/** Registered by the focused VaultEditor — flush TipTap/CM before remount/leave. */

export type VaultLeaveFlushFn = () => void | Promise<void>;

let leaveFlushHandler: VaultLeaveFlushFn | null = null;

export function registerVaultLeaveFlush(handler: VaultLeaveFlushFn | null) {
  leaveFlushHandler = handler;
}

export async function invokeVaultLeaveFlush(): Promise<void> {
  await leaveFlushHandler?.();
}
