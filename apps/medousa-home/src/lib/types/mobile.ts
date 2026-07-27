export type MobileTab = "home" | "chat" | "notes" | "web" | "more";

export type MoreDestination =
  | "hub"
  | "profiles"
  | "map"
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
  { id: "profiles", label: "You", hint: "Who you are — teach her facts" },
  { id: "map", label: "Map", hint: "Sessions, moments, and notes linked" },
  { id: "workshop", label: "Agents", hint: "Specialist agents you can run" },
  {
    id: "automations",
    label: "Automations",
    hint: "Scripts, flows, schedules & history",
  },
  { id: "calendar", label: "Calendar", hint: "Meetings, reminders & .ics" },
  { id: "messaging", label: "Channels", hint: "Telegram, Discord, Slack & more" },
  { id: "peers", label: "Peers", hint: "Nearby workshops & inbox" },
  { id: "settings", label: "Preferences", hint: "Models, voice, rhythm & reach" },
  { id: "runtime", label: "Workshop", hint: "Live pulse, jobs & delivery" },
];

/** Destinations listed on the More hub home. */
export const MORE_HUB_SECTIONS: {
  title: string;
  subtitle: string;
  destinations: Exclude<MoreDestination, "hub">[];
}[] = [
  {
    title: "Stay in touch",
    subtitle: "Memory, agents, scripts, and channels",
    destinations: [
      "profiles",
      "map",
      "workshop",
      "automations",
      "calendar",
      "messaging",
      "peers",
    ],
  },
  {
    title: "Preferences",
    subtitle: "Tuning and workshop pulse",
    destinations: ["settings", "runtime"],
  },
];
