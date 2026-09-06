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

export type ComputerControlHolder =
  | "available"
  | "human"
  | "agent"
  | "bot"
  | "worker"
  | "peer"
  | "system";

export interface ComputerWorldControlState {
  world_id: string;
  driver_id: string;
  resource_id: string;
  session_id: string;
  ownership: ComputerDriverOwnership;
  holder: ComputerControlHolder;
  requester_has_control: boolean;
  control_generation: number;
  revision: number;
  lease_expires_at_ms?: number | null;
}

export interface ComputerWatchCapture {
  driver_id: string;
  resource_id: string;
  session_id: string;
  observation_generation: string;
  observation_revision: number;
  window_resource_id: string;
  mime: "image/png";
  image_width: number;
  image_height: number;
  byte_size: number;
  sha256: string;
  sensitive_regions_redacted: number;
  captured_at_ms: number;
  image_base64: string;
}

export interface ComputerWatchFrame {
  observation: {
    generation: string;
    revision: number;
    focused_window_resource_id: string;
  };
  capture: ComputerWatchCapture;
  control: ComputerWorldControlState;
}

export interface ComputerDriverReadiness {
  driver: ComputerDriverRegistration;
  resourceId?: string;
  preflight?: ComputerDriverPreflight;
  control?: ComputerWorldControlState;
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
): Promise<{
  resourceId: string;
  preflight: ComputerDriverPreflight;
  control: ComputerWorldControlState;
}> {
  const response = await daemonUnary<{
    ok: boolean;
    resource_id: string;
    preflight: ComputerDriverPreflight;
    control: ComputerWorldControlState;
  }>("computer.drivers.by_driver_id.preflight.get", { driver_id: driverId });
  return {
    resourceId: response.resource_id,
    preflight: response.preflight,
    control: response.control,
  };
}

export async function watchComputerDriver(
  driverId: string,
  sessionId: string,
  maxWidth = 960,
): Promise<ComputerWatchFrame> {
  return daemonUnary<ComputerWatchFrame>(
    "computer.drivers.by_driver_id.watch.post",
    { driver_id: driverId },
    { session_id: sessionId, max_width: maxWidth },
  );
}

export async function controlComputerDriver(
  driverId: string,
  sessionId: string,
  action: "take_control" | "return_to_medousa",
): Promise<ComputerWorldControlState> {
  const response = await daemonUnary<{ ok: boolean; control: ComputerWorldControlState }>(
    "computer.drivers.by_driver_id.control.post",
    { driver_id: driverId },
    { session_id: sessionId, action },
  );
  return response.control;
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
          control: result.control,
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
