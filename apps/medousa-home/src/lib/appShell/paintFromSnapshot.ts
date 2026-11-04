/**
 * Paint shellTabs host fields from a ShellCore snapshot.
 * Core owns desktop/tab/pane identity after hydrate; localStorage is a
 * persistence sink, not live authority.
 */

import type {
  ShellDesktop as AppShellDesktop,
  ShellDesktopLayout as AppShellLayout,
  ShellSnapshot,
} from "$lib/types/generated/app_api";
import type { ShellDesktop, ShellDesktopLayout } from "$lib/types/shellTabs";

export function toHostShellDesktops(desktops: AppShellDesktop[]): ShellDesktop[] {
  return desktops as unknown as ShellDesktop[];
}

export function toHostShellLayout(layout: AppShellLayout): ShellDesktopLayout {
  return layout as unknown as ShellDesktopLayout;
}

export function shellSnapshotMatchesScope(
  snapshot: ShellSnapshot,
  workspaceScopeId: string,
): boolean {
  if (!snapshot.workspaceScopeId) return true;
  return snapshot.workspaceScopeId === workspaceScopeId;
}
