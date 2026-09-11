export interface LiquidComponentStateScope {
  sessionId: string;
  messageId: string;
  nodeId: string;
  instanceId: string;
}

export interface LiquidComponentStateRecord<T = unknown> {
  schema_version: number;
  session_id: string;
  message_id: string;
  node_id: string;
  instance_id: string;
  component_type: string;
  revision: number;
  state: T;
  updated_at_utc: string;
}

export interface LiquidComponentStatePort {
  get<T>(scope: LiquidComponentStateScope): Promise<LiquidComponentStateRecord<T> | null>;
  put<T>(
    scope: LiquidComponentStateScope,
    input: {
      schemaVersion: number;
      componentType: string;
      expectedRevision: number;
      state: T;
    },
  ): Promise<LiquidComponentStateRecord<T>>;
}
