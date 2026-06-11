export type MobileTab = "pulse" | "work" | "chat" | "you";

export type YouDestination =
  | "hub"
  | "library"
  | "context"
  | "skills"
  | "cron"
  | "messaging"
  | "settings"
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
  { id: "context", label: "Context", hint: "What she remembers about you" },
  { id: "skills", label: "Skills", hint: "Run in chat" },
  { id: "cron", label: "Schedule", hint: "Recurring prompts" },
  { id: "messaging", label: "Channels", hint: "Telegram, Discord, Slack & more" },
  { id: "settings", label: "Preferences", hint: "Charter — memory, voice, reach" },
  { id: "runtime", label: "Workshop", hint: "Live pulse, jobs & delivery" },
];

export const YOU_HUB_SECTIONS: {
  title: string;
  subtitle: string;
  destinations: Exclude<YouDestination, "hub">[];
}[] = [
  {
    title: "Stay in touch",
    subtitle: "Notes, memory, skills, and channels",
    destinations: ["library", "context", "skills", "messaging"],
  },
  {
    title: "Workshop",
    subtitle: "Schedule, preferences, and tuning",
    destinations: ["cron", "settings", "runtime"],
  },
];
