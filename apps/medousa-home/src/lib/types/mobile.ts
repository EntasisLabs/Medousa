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
  { id: "library", label: "Notes", hint: "Your vault" },
  { id: "skills", label: "Skills", hint: "Run in chat" },
  { id: "cron", label: "Schedule", hint: "Recurring prompts" },
  { id: "messaging", label: "Channels", hint: "Telegram, Discord, more" },
  { id: "settings", label: "Workshop", hint: "Connection & preferences" },
  { id: "advanced", label: "Advanced", hint: "Model & API key" },
  { id: "runtime", label: "Health", hint: "Queue & delivery" },
];

export const YOU_HUB_SECTIONS: {
  title: string;
  subtitle: string;
  destinations: Exclude<YouDestination, "hub">[];
}[] = [
  {
    title: "Stay in touch",
    subtitle: "Notes, skills, and channels",
    destinations: ["library", "skills", "messaging"],
  },
  {
    title: "Workshop",
    subtitle: "Schedule, settings, and health",
    destinations: ["cron", "settings", "advanced", "runtime"],
  },
];
