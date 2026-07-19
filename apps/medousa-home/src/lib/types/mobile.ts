export type MobileTab = "home" | "chat" | "notes" | "web" | "more";

export type MoreDestination =
  | "hub"
  | "profiles"
  | "context"
  | "workshop"
  | "automations"
  | "calendar"
  | "messaging"
  | "peers"
  | "settings"
  | "runtime";

export const MOBILE_TABS: { id: MobileTab; label: string }[] = [
  { id: "home", label: "Home" },
  { id: "chat", label: "Chat" },
  { id: "notes", label: "Notes" },
  { id: "web", label: "Web" },
  { id: "more", label: "More" },
];

export const MORE_DESTINATIONS: {
  id: Exclude<MoreDestination, "hub">;
  label: string;
  hint: string;
}[] = [
  { id: "profiles", label: "Profiles", hint: "Who you are — teach her facts" },
  { id: "context", label: "Context", hint: "What she remembers about you" },
  { id: "workshop", label: "Agents", hint: "Specialist agents you can run" },
  { id: "calendar", label: "Calendar", hint: "Meetings, reminders & .ics" },
  { id: "messaging", label: "Channels", hint: "Telegram, Discord, Slack & more" },
  { id: "peers", label: "Peers", hint: "Nearby workshops & inbox" },
  { id: "settings", label: "Preferences", hint: "Models, voice, rhythm & reach" },
  { id: "runtime", label: "Workshop", hint: "Live pulse, jobs & delivery" },
];

/** Destinations listed on the More hub home (automations is deep-link only). */
export const MORE_HUB_SECTIONS: {
  title: string;
  subtitle: string;
  destinations: Exclude<MoreDestination, "hub" | "automations">[];
}[] = [
  {
    title: "Stay in touch",
    subtitle: "Memory, agents, and channels",
    destinations: ["profiles", "context", "workshop", "calendar", "messaging", "peers"],
  },
  {
    title: "Preferences",
    subtitle: "Tuning and workshop pulse",
    destinations: ["settings", "runtime"],
  },
];
