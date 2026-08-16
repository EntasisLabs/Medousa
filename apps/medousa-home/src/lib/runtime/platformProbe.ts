import { shouldUseMobileShell } from "$lib/platform";
import type { ClientPlatform } from "./features/types";

/** Dependency-light platform choice — call before importing a shell graph. */
export function probeClientPlatform(): ClientPlatform {
  return shouldUseMobileShell() ? "mobile" : "desktop";
}
