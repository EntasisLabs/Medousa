import { daemonUnary } from "./contractClient";
import { getDaemonUrl } from "./client";
import {
  openDaemonEventStream,
  type DaemonEventConnection,
  type DaemonStreamFailure,
} from "./daemonEventStream";
import { operationPath } from "./opPath";
import type { BrowserPresentationFrame } from "$lib/types/generated/daemon_api";

export type BrowserWorldRunState = "starting" | "running" | "paused" | "stopped" | "failed";
export type BrowserWorldControl = "agent" | "user" | "awaiting_operator";

export type BrowserWorldProfile =
  | { kind: "ephemeral" }
  | { kind: "persistent"; profile_id: string };

export interface BrowserWorldDriver {
  driver_id: string;
  display_name?: string | null;
  ownership: "owned" | "managed" | "attached";
}

export interface IsolatedBrowserWorld {
  world_id: string;
  owner_profile_id: string;
  authority_id: string;
  driver: BrowserWorldDriver;
  tab_group_id: string;
  tab_id?: string | null;
  profile: BrowserWorldProfile;
  run_state: BrowserWorldRunState;
  control: BrowserWorldControl;
  control_epoch: number;
  view_attached: boolean;
  headless: boolean;
  url: string;
  title: string;
  failure?: string | null;
  created_at_ms: number;
  updated_at_ms: number;
}

export interface BrowserObservationViewport {
  width: number;
  height: number;
  scroll_x: number;
  scroll_y: number;
  device_scale_factor: number;
}

export interface BrowserObservation {
  tab_id: string;
  url: string;
  title: string;
  document_id: string;
  revision: number;
  full: boolean;
  viewport: BrowserObservationViewport;
  captured_at_ms: number;
}

export interface BrowserScreenshot {
  tab_id: string;
  url: string;
  title: string;
  document_id: string;
  observation_revision: number;
  viewport: BrowserObservationViewport;
  coordinate_frame: "css_viewport" | string;
  mime: string;
  image_width: number;
  image_height: number;
  byte_size: number;
  sha256: string;
  sensitive_regions_redacted: number;
  captured_at_ms: number;
  image_base64: string;
}

export type BrowserHumanInput =
  | { action: "click"; x: number; y: number; button?: "left" | "middle" | "right" }
  | {
      action: "scroll";
      x?: number;
      y?: number;
      delta_x: number;
      delta_y: number;
    }
  | { action: "text"; text: string }
  | { action: "key"; key: string; modifiers?: string[] };

type WorldResponse = { ok: boolean; world: IsolatedBrowserWorld };

export async function listIsolatedBrowserWorlds(
  executionRuntimeId?: string | null,
): Promise<IsolatedBrowserWorld[]> {
  const response = await daemonUnary<{ ok: boolean; worlds: IsolatedBrowserWorld[] }>(
    "browser.worlds.isolated.get",
    {},
    undefined,
    executionRuntimeId,
  );
  return response.worlds ?? [];
}

export async function createIsolatedBrowserWorld(input?: {
  displayName?: string;
  profile?: BrowserWorldProfile;
  initialUrl?: string;
}, executionRuntimeId?: string | null): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.post",
    {},
    {
      display_name: input?.displayName ?? "Workshop browser",
      profile: input?.profile ?? { kind: "ephemeral" },
      initial_url: input?.initialUrl ?? "about:blank",
      headless: true,
    },
    executionRuntimeId,
  );
  return response.world;
}

export async function getIsolatedBrowserWorld(
  worldId: string,
  executionRuntimeId?: string | null,
): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.by_world_id.get",
    { world_id: worldId },
    undefined,
    executionRuntimeId,
  );
  return response.world;
}

export async function setIsolatedBrowserWorldLifecycle(
  worldId: string,
  action:
    | "pause"
    | "resume"
    | "takeover"
    | "return_to_agent"
    | "attach_view"
    | "detach_view"
    | "stop",
  executionRuntimeId?: string | null,
): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.by_world_id.lifecycle.post",
    { world_id: worldId },
    { action },
    executionRuntimeId,
  );
  return response.world;
}

export async function navigateIsolatedBrowserWorld(
  worldId: string,
  url: string,
  executionRuntimeId?: string | null,
): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.by_world_id.navigate.post",
    { world_id: worldId },
    { url },
    executionRuntimeId,
  );
  return response.world;
}

export async function observeIsolatedBrowserWorld(
  worldId: string,
  sinceRevision?: number,
  executionRuntimeId?: string | null,
): Promise<BrowserObservation> {
  const response = await daemonUnary<{ ok: boolean; observation: BrowserObservation }>(
    "browser.worlds.isolated.by_world_id.observe.post",
    { world_id: worldId },
    { since_revision: sinceRevision ?? null, max_nodes: 64 },
    executionRuntimeId,
  );
  return response.observation;
}

export async function screenshotIsolatedBrowserWorld(
  worldId: string,
  observation: BrowserObservation,
  maxWidth: number,
  executionRuntimeId?: string | null,
): Promise<BrowserScreenshot> {
  const response = await daemonUnary<{ ok: boolean; screenshot: BrowserScreenshot }>(
    "browser.worlds.isolated.by_world_id.screenshot.post",
    { world_id: worldId },
    {
      expected_document_id: observation.document_id,
      expected_observation_revision: observation.revision,
      max_width: maxWidth,
    },
    executionRuntimeId,
  );
  return response.screenshot;
}

export interface BrowserPresentationStreamOptions {
  worldId: string;
  sinceRevision?: number;
  maxWidth: number;
  intervalMs?: number;
  executionRuntimeId?: string | null;
  onFrame(frame: BrowserPresentationFrame): void;
  onOpen?(): void;
  onError(error: DaemonStreamFailure): void;
}

/**
 * Watch an isolated browser through one destination-owned stream. Every event
 * pairs a semantic action fence with the exact redacted pixel artifact.
 */
export async function openIsolatedBrowserPresentation(
  options: BrowserPresentationStreamOptions,
): Promise<DaemonEventConnection> {
  const pathParams = { world_id: options.worldId };
  const query = {
    ...(options.sinceRevision !== undefined
      ? { since_revision: String(options.sinceRevision) }
      : {}),
    max_width: String(Math.max(320, Math.min(1280, Math.round(options.maxWidth || 960)))),
    interval_ms: String(Math.max(100, Math.min(2500, Math.round(options.intervalMs ?? 250)))),
  };
  let connection: DaemonEventConnection | null = null;
  let invalidFrame = false;
  const opened = await openDaemonEventStream<BrowserPresentationFrame>({
    operation: "browser.worlds.isolated.by_world_id.presentation.get",
    pathParams,
    query,
    executionRuntimeId: options.executionRuntimeId,
    browserEvent: "browser-frame",
    browserUrl: async () => {
      const base = (await getDaemonUrl()).replace(/\/$/, "");
      const path = operationPath(
        "browser.worlds.isolated.by_world_id.presentation.get",
        pathParams,
      );
      return `${base}${path}?${new URLSearchParams(query).toString()}`;
    },
    onOpen: options.onOpen,
    onError: options.onError,
    onEvent: (frame) => {
      const sameFence =
        frame.world_id === options.worldId &&
        frame.observation.document_id === frame.screenshot.document_id &&
        frame.observation.revision === frame.screenshot.observation_revision;
      if (!sameFence) {
        invalidFrame = true;
        connection?.close();
        options.onError({
          message: "Browser presentation returned a mismatched observation fence",
          recoverable: false,
          transport: "presentation",
          stage: "validate",
        });
        return;
      }
      options.onFrame(frame);
    },
  });
  connection = opened;
  if (invalidFrame) opened.close();
  return opened;
}

export async function inputIsolatedBrowserWorld(
  worldId: string,
  observation: BrowserObservation,
  input: BrowserHumanInput,
  executionRuntimeId?: string | null,
): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.by_world_id.input.post",
    { world_id: worldId },
    {
      expected_document_id: observation.document_id,
      expected_observation_revision: observation.revision,
      ...input,
    },
    executionRuntimeId,
  );
  return response.world;
}
