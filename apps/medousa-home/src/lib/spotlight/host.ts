import type { SpotlightLens } from "$lib/commands/collectCommands";
import type { WorkshopCommand } from "$lib/commands/types";
import type { CommandSpotlightMode } from "$lib/stores/commandSpotlight.svelte";
import type { ShellDesktopLayout } from "$lib/types/shellTabs";
import type { ToolHistorySliceRef } from "$lib/types/toolHistory";
import type { ActivityTarget } from "./activity";

export interface SpotlightQuery {
  query: string;
  mode: CommandSpotlightMode;
  lens: SpotlightLens;
  scopeId: string;
}
export type SpotlightCommandDescriptor = Omit<WorkshopCommand, "run"> & { token: string };
export interface SpotlightSnapshot {
  notice?: string | null;
  groups: Array<{ id: string; label: string; commands: SpotlightCommandDescriptor[] }>;
  scopes: Array<{ id: string; label: string }>;
  layout: ShellDesktopLayout | null;
}
/** Presentation stays shared; a host supplies destinations and real editor context. */
export interface SpotlightHost {
  collect(query: SpotlightQuery): Promise<SpotlightSnapshot>;
  execute(command: SpotlightCommandDescriptor, args?: string): Promise<void>;
  pin(slot: number): Promise<void>;
  openConversation(sessionId: string): Promise<void>;
  openActivity(target: ActivityTarget, title: string): Promise<void>;
  newAsk(): Promise<void>;
  manageConnection(): Promise<void>;
  createFlow(refs: ToolHistorySliceRef[], name: string): Promise<void>;
  openSsh(targetId: string): Promise<void>;
}

export type SpotlightHostRequest =
  | { kind: "collect"; query: SpotlightQuery }
  | { kind: "execute"; token: string; args?: string }
  | { kind: "pin"; slot: number }
  | { kind: "activity"; target: ActivityTarget; title: string }
  | { kind: "new-ask" }
  | { kind: "connection" }
  | { kind: "ssh"; targetId: string }
  | { kind: "create-flow"; refs: ToolHistorySliceRef[]; name: string };
