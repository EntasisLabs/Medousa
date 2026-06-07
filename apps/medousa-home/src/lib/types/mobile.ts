export type MobileTab = "pulse" | "work" | "chat" | "you";

export type YouDestination =
  | "hub"
  | "library"
  | "skills"
  | "cron"
  | "messaging"
  | "settings"
  | "advanced"
  | "runtime";

export const MOBILE_TABS: { id: MobileTab; label: string }[] = [
  { id: "pulse", label: "Pulse" },
  { id: "work", label: "Work" },
  { id: "chat", label: "Chat" },
  { id: "you", label: "You" },
];

export const YOU_DESTINATIONS: {
  id: Exclude<YouDestination, "hub">;
  label: string;
  hint: string;
}[] = [
  { id: "library", label: "Notes", hint: "Vault and wikilinks" },
  { id: "skills", label: "Skills", hint: "Runnable manuscripts" },
  { id: "cron", label: "Schedule", hint: "Recurring prompts" },
  { id: "messaging", label: "Channels", hint: "Telegram, Discord, and more" },
  { id: "settings", label: "Workshop", hint: "Connection and preferences" },
  { id: "advanced", label: "Advanced", hint: "Model, API key, essentials" },
  { id: "runtime", label: "Workshop health", hint: "Queue, delivery, telemetry" },
];
