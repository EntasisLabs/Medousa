export type MobileTab = "pulse" | "work" | "chat" | "you";

export type YouDestination =
  | "hub"
  | "profiles"
  | "library"
  | "context"
  | "workshop"
  | "automations"
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
  { id: "profiles", label: "Profiles", hint: "Who you are — teach her facts" },
  { id: "library", label: "Notes", hint: "Your vault" },
  { id: "context", label: "Context", hint: "What she remembers about you" },
  { id: "workshop", label: "Capabilities", hint: "Specialists & connections" },
  { id: "automations", label: "Automations", hint: "Scripts, flows, schedules & history" },
  { id: "messaging", label: "Channels", hint: "Telegram, Discord, Slack & more" },
  { id: "settings", label: "Preferences", hint: "Models, voice, rhythm & reach" },
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
    destinations: ["profiles", "library", "context", "workshop", "messaging"],
  },
  {
    title: "Capabilities",
    subtitle: "Automations, preferences, and tuning",
    destinations: ["automations", "settings", "runtime"],
  },
];
