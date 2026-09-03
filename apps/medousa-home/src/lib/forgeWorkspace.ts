/** Workspace placement shared by Forge projections and project creation UI. */

export type ForgeWorkspaceMode = "isolated" | "attached_checkout";

export type ForgeEnvironment = {
  kind?: "git_worktree" | "attached_checkout" | string;
  worktree: string;
  branch: string;
  baseline_oid: string;
  attached_index_oid?: string | null;
  generation: number;
};

export function usesAttachedCheckout(item: {
  workspace_mode?: ForgeWorkspaceMode;
  environment?: Pick<ForgeEnvironment, "kind"> | null;
} | null | undefined): boolean {
  return item?.workspace_mode === "attached_checkout"
    || item?.environment?.kind === "attached_checkout";
}

export function unsupportedAttachedCheckout(): Error {
  return new Error(
    "This workshop does not support current-checkout Coder projects yet. Update medousa_daemon and try again.",
  );
}

export function assertWorkspaceMode(
  item: Parameters<typeof usesAttachedCheckout>[0],
  requested?: ForgeWorkspaceMode,
): void {
  if (requested === "attached_checkout" && !usesAttachedCheckout(item)) {
    throw unsupportedAttachedCheckout();
  }
}
