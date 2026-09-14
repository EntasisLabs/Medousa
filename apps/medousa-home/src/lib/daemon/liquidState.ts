import { invoke } from "@tauri-apps/api/core";
import type {
  LiquidComponentStatePort,
  LiquidComponentStateRecord,
  LiquidComponentStateScope,
} from "$lib/liquid/ports";

type Response<T> = { state?: LiquidComponentStateRecord<T> | null };

function args(scope: LiquidComponentStateScope) {
  return {
    sessionId: scope.sessionId,
    messageId: scope.messageId,
    nodeId: scope.nodeId,
    instanceId: scope.instanceId,
  };
}

export const daemonLiquidComponentState: LiquidComponentStatePort = {
  async get<T>(scope: LiquidComponentStateScope) {
    const response = await invoke<Response<T>>("liquid_state_get", args(scope));
    return response.state ?? null;
  },
  async put<T>(scope: LiquidComponentStateScope, input: {
    schemaVersion: number;
    componentType: string;
    expectedRevision: number;
    state: T;
  }) {
    const response = await invoke<Response<T>>("liquid_state_put", {
      ...args(scope),
      request: {
        schema_version: input.schemaVersion,
        component_type: input.componentType,
        expected_revision: input.expectedRevision,
        state: input.state,
      },
    });
    if (!response.state) throw new Error("Liquid component state was not returned");
    return response.state;
  },
};
