import { chat } from "$lib/stores/chat.svelte";
import { connection } from "$lib/stores/connection.svelte";
import { layout } from "$lib/stores/layout.svelte";
import { runtime } from "$lib/stores/runtime.svelte";
import { settingsNav } from "$lib/stores/settingsNav.svelte";
import { vault } from "$lib/stores/vault.svelte";
import { workspace } from "$lib/stores/workspace.svelte";
import type { SettingsSectionId } from "$lib/types/settings";
import type { RuntimeTab } from "$lib/types/runtime";
import type { Surface } from "$lib/types/ui";

export type CommandSection =
  | "suggested"
  | "go"
  | "open"
  | "ask"
  | "tune"
  | "advanced";

export type CommandRisk = "safe" | "attention";

export interface CommandSpotlightCallbacks {
  close: () => void;
  focusChat: () => void;
}

export interface WorkshopCommandContext {
  layout: typeof layout;
  chat: typeof chat;
  workspace: typeof workspace;
  vault: typeof vault;
  runtime: typeof runtime;
  connection: typeof connection;
  settingsNav: typeof settingsNav;
  callbacks: CommandSpotlightCallbacks;
  navigate: (surface: Surface) => void;
  openRuntimeTab: (tab: RuntimeTab) => void;
  openSettingsSection: (section: SettingsSectionId) => void;
  notice: (message: string) => void;
  error: (message: string) => void;
}

export interface WorkshopCommand {
  id: string;
  section: CommandSection;
  label: string;
  subtitle?: string;
  hint?: string;
  keywords?: string;
  aliases?: string[];
  risk?: CommandRisk;
  advanced?: boolean;
  /** When set, selecting opens a second-step prompt instead of running immediately. */
  prompt?: {
    placeholder: string;
    submitLabel?: string;
  };
  run: (ctx: WorkshopCommandContext, args?: string) => void | Promise<void>;
}

export interface GroupedCommands {
  section: CommandSection;
  label: string;
  commands: WorkshopCommand[];
}

export const SECTION_LABELS: Record<CommandSection, string> = {
  suggested: "Suggested",
  go: "Go to",
  open: "Open",
  ask: "Ask Medousa",
  tune: "Tune",
  advanced: "Advanced",
};

export const SECTION_ORDER: CommandSection[] = [
  "suggested",
  "go",
  "open",
  "ask",
  "tune",
  "advanced",
];
