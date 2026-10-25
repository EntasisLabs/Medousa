import { checkDaemonHealth } from "$lib/daemon/runtime";
import { activeWorkshopId } from "$lib/utils/workshopLocality";
import type { HostTurnContext } from "$lib/types/generated/daemon_api";
/** Old engines ignore unknown fields. Never let that silently retarget a vault read. */
export async function ensureReferenceSupport(context?: HostTurnContext | null) {
  if (!context?.references?.length) return;
  const scope = activeWorkshopId();
  const health = await checkDaemonHealth();
  if (scope !== activeWorkshopId()) throw new Error("The workshop changed. Send the request again in its original workshop.");
  if (!health.runtime?.advertised_capabilities.includes("turn.resource_references.v1")) {
    throw new Error("Update the workshop engine before sending note or project references.");
  }
}
