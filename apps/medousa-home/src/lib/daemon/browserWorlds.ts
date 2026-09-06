import { daemonUnary } from "./contractClient";

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

export async function listIsolatedBrowserWorlds(): Promise<IsolatedBrowserWorld[]> {
  const response = await daemonUnary<{ ok: boolean; worlds: IsolatedBrowserWorld[] }>(
    "browser.worlds.isolated.get",
  );
  return response.worlds ?? [];
}

export async function createIsolatedBrowserWorld(input?: {
  displayName?: string;
  profile?: BrowserWorldProfile;
  initialUrl?: string;
}): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.post",
    {},
    {
      display_name: input?.displayName ?? "Workshop browser",
      profile: input?.profile ?? { kind: "ephemeral" },
      initial_url: input?.initialUrl ?? "about:blank",
      headless: true,
    },
  );
  return response.world;
}

export async function getIsolatedBrowserWorld(worldId: string): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.by_world_id.get",
    { world_id: worldId },
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
): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.by_world_id.lifecycle.post",
    { world_id: worldId },
    { action },
  );
  return response.world;
}

export async function navigateIsolatedBrowserWorld(
  worldId: string,
  url: string,
): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.by_world_id.navigate.post",
    { world_id: worldId },
    { url },
  );
  return response.world;
}

export async function observeIsolatedBrowserWorld(
  worldId: string,
  sinceRevision?: number,
): Promise<BrowserObservation> {
  const response = await daemonUnary<{ ok: boolean; observation: BrowserObservation }>(
    "browser.worlds.isolated.by_world_id.observe.post",
    { world_id: worldId },
    { since_revision: sinceRevision ?? null, max_nodes: 64 },
  );
  return response.observation;
}

export async function screenshotIsolatedBrowserWorld(
  worldId: string,
  observation: BrowserObservation,
  maxWidth: number,
): Promise<BrowserScreenshot> {
  const response = await daemonUnary<{ ok: boolean; screenshot: BrowserScreenshot }>(
    "browser.worlds.isolated.by_world_id.screenshot.post",
    { world_id: worldId },
    {
      expected_document_id: observation.document_id,
      expected_observation_revision: observation.revision,
      max_width: maxWidth,
    },
  );
  return response.screenshot;
}

export async function inputIsolatedBrowserWorld(
  worldId: string,
  observation: BrowserObservation,
  input: BrowserHumanInput,
): Promise<IsolatedBrowserWorld> {
  const response = await daemonUnary<WorldResponse>(
    "browser.worlds.isolated.by_world_id.input.post",
    { world_id: worldId },
    {
      expected_document_id: observation.document_id,
      expected_observation_revision: observation.revision,
      ...input,
    },
  );
  return response.world;
}
