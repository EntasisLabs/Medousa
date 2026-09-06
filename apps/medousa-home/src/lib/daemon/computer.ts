import { daemonUnary } from "./contractClient";

export type ComputerDriverOwnership = "owned" | "managed" | "attached";
export type ComputerPermissionKind = "accessibility" | "screen_capture" | "input_control";
export type ComputerPermissionStatus =
  | "granted"
  | "denied"
  | "not_determined"
  | "restricted"
  | "unsupported";

export interface ComputerDriverRegistration {
  driver_id: string;
  kind: string;
  surface: string;
  ownership: ComputerDriverOwnership;
  transport: string;
  capabilities: string[];
  display_name?: string | null;
}

export interface ComputerPermissionReport {
  permission: ComputerPermissionKind;
  status: ComputerPermissionStatus;
  can_request: boolean;
  guidance?: string | null;
}

export interface ComputerDriverPreflight {
  protocol_version: number;
  driver_id: string;
  platform: string;
  session_id: string;
  permissions: ComputerPermissionReport[];
  checked_at_ms: number;
}

export interface ComputerDriverReadiness {
  driver: ComputerDriverRegistration;
  resourceId?: string;
  preflight?: ComputerDriverPreflight;
  error?: string;
}

export async function listComputerDrivers(): Promise<ComputerDriverRegistration[]> {
  const response = await daemonUnary<{ ok: boolean; drivers: ComputerDriverRegistration[] }>(
    "computer.drivers.get",
  );
  return response.drivers ?? [];
}

export async function preflightComputerDriver(
  driverId: string,
): Promise<{ resourceId: string; preflight: ComputerDriverPreflight }> {
  const response = await daemonUnary<{
    ok: boolean;
    resource_id: string;
    preflight: ComputerDriverPreflight;
  }>("computer.drivers.by_driver_id.preflight.get", { driver_id: driverId });
  return {
    resourceId: response.resource_id,
    preflight: response.preflight,
  };
}

/**
 * Readiness is intentionally observational. Opening Runtime Controls never
 * asks the operating system for permission or starts a computer action.
 */
export async function loadComputerDriverReadiness(): Promise<ComputerDriverReadiness[]> {
  const drivers = await listComputerDrivers();
  return Promise.all(
    drivers.map(async (driver) => {
      try {
        const result = await preflightComputerDriver(driver.driver_id);
        return {
          driver,
          resourceId: result.resourceId,
          preflight: result.preflight,
        };
      } catch (error) {
        return {
          driver,
          error: error instanceof Error ? error.message : String(error),
        };
      }
    }),
  );
}
