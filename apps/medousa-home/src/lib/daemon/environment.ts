import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { StreamErrorPayload } from "./client";

export async function getEnvironmentStatus(
  profileId?: string,
  surfaceId?: string,
  options?: { includeRuntime?: boolean },
): Promise<import("$lib/types/environment").EnvironmentStatusResponse> {
  return invoke("environment_get_status", {
    profileId,
    surfaceId,
    includeRuntime: options?.includeRuntime ?? null,
  });
}

export async function getEnvironmentSpec(
  profileId?: string,
): Promise<import("$lib/types/environment").EnvironmentSpecResponse> {
  return invoke("environment_get_spec", { profileId });
}

export async function putEnvironmentSpec(
  request: import("$lib/types/environment").EnvironmentSpecPutRequest,
): Promise<import("$lib/types/environment").EnvironmentSpecResponse> {
  return invoke("environment_put_spec", { request });
}

export async function getEnvironmentPending(
  profileId?: string,
): Promise<import("$lib/types/environment").EnvironmentPendingResponse> {
  return invoke("environment_get_pending", { profileId });
}

export async function applyEnvironmentPending(
  profileId?: string,
): Promise<import("$lib/types/environment").EnvironmentSpecResponse> {
  return invoke("environment_apply_pending", { profileId });
}

export async function dismissEnvironmentPending(
  profileId?: string,
): Promise<void> {
  return invoke("environment_dismiss_pending", { profileId });
}

export async function startEnvironmentStream(
  sinceRevision?: number,
  profileId?: string,
): Promise<void> {
  return invoke("environment_stream_start", { sinceRevision, profileId });
}

export async function stopEnvironmentStream(): Promise<void> {
  return invoke("environment_stream_stop");
}

export async function fetchFeedTail(
  feedId: string,
  limit?: number,
  profileId?: string,
): Promise<import("$lib/types/environment").FeedTailResponse> {
  return invoke("feed_tail", {
    feedId,
    limit: limit ?? null,
    profileId: profileId ?? null,
  });
}

export async function fetchFeedLatestGood(
  feedId: string,
  profileId?: string,
): Promise<import("$lib/types/environment").FeedLatestGoodResponse> {
  return invoke("feed_latest_good", {
    feedId,
    profileId: profileId ?? null,
  });
}

export async function componentStoreGet(
  componentId: string,
  options?: { key?: string; profileId?: string },
): Promise<import("$lib/types/environment").ComponentStoreGetResponse> {
  return invoke("component_store_get", {
    componentId,
    key: options?.key ?? null,
    profileId: options?.profileId ?? null,
  });
}

export async function componentStoreSet(
  componentId: string,
  key: string,
  value: unknown,
  profileId?: string,
): Promise<import("$lib/types/environment").ComponentStoreSetResponse> {
  return invoke("component_store_set", {
    componentId,
    key,
    value,
    profileId: profileId ?? null,
  });
}

export async function componentStoreDelete(
  componentId: string,
  key: string,
  profileId?: string,
): Promise<import("$lib/types/environment").ComponentStoreDeleteResponse> {
  return invoke("component_store_delete", {
    componentId,
    key,
    profileId: profileId ?? null,
  });
}

export async function componentStoreListKeys(
  componentId: string,
  profileId?: string,
): Promise<import("$lib/types/environment").ComponentStoreListResponse> {
  return invoke("component_store_list_keys", {
    componentId,
    profileId: profileId ?? null,
  });
}

export type ComponentRuntimeEventInput = {
  level: string;
  message: string;
  stack?: string;
  source?: string;
  sessionId?: string;
};

export async function componentRuntimeAppendEvents(
  componentId: string,
  events: ComponentRuntimeEventInput[],
  options?: { profileId?: string; sessionId?: string },
): Promise<{ ok: boolean; accepted: number }> {
  return invoke("component_runtime_append_events", {
    componentId,
    request: {
      events,
      profileId: options?.profileId ?? null,
      sessionId: options?.sessionId ?? null,
    },
  });
}

export async function componentRuntimeCompleteProbe(
  componentId: string,
  probeId: string,
  result: {
    probeId: string;
    componentId: string;
    storeReady: boolean;
    storeRoundTripOk: boolean;
    errors: string[];
    profileId?: string;
  },
): Promise<{ ok: boolean }> {
  return invoke("component_runtime_complete_probe", {
    componentId,
    probeId,
    result: {
      ...result,
      profileId: result.profileId ?? null,
    },
  });
}

export function onEnvironmentEvent<T>(
  handler: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>("environment://event", (event) => {
    handler(event.payload);
  });
}

export function onEnvironmentError(
  handler: (error: StreamErrorPayload) => void,
): Promise<UnlistenFn> {
  return listen<StreamErrorPayload>("environment://error", (event) => {
    handler(event.payload);
  });
}
