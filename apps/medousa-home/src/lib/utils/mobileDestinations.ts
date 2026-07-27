import {
  MORE_DESTINATIONS,
  MORE_HUB_SECTIONS,
  type MobileTab,
  type MoreDestination,
} from "$lib/types/mobile";

export type MobileDestinationKind = "tab" | "more";

export type MobileDestinationItem = {
  id: string;
  label: string;
  hint?: string;
  kind: MobileDestinationKind;
  tab?: MobileTab;
  more?: Exclude<MoreDestination, "hub">;
};

/** Primary tabs shown at the top of the destinations menu. */
export const MOBILE_PRIMARY_DESTINATIONS: MobileDestinationItem[] = [
  { id: "tab-home", label: "Home", hint: "Glance & continue", kind: "tab", tab: "home" },
  { id: "tab-chat", label: "Chat", hint: "Sessions and replies", kind: "tab", tab: "chat" },
  { id: "tab-notes", label: "Notes", hint: "Vault library", kind: "tab", tab: "notes" },
  { id: "tab-web", label: "Web", hint: "Browser", kind: "tab", tab: "web" },
  {
    id: "more-calendar",
    label: "Calendar",
    hint: "Meetings, reminders & .ics",
    kind: "more",
    more: "calendar",
  },
];

export function moreDestinationItems(): MobileDestinationItem[] {
  return MORE_HUB_SECTIONS.flatMap((section) =>
    section.destinations
      .filter((id) => id !== "calendar" && id !== "settings")
      .map((id) => {
        const meta = MORE_DESTINATIONS.find((row) => row.id === id);
        return {
          id: `more-${id}`,
          label: meta?.label ?? id,
          hint: meta?.hint,
          kind: "more" as const,
          more: id,
        };
      }),
  );
}

/** Always rendered last in the destinations menu (after custom views). */
export function settingsDestinationItem(): MobileDestinationItem {
  const meta = MORE_DESTINATIONS.find((row) => row.id === "settings");
  return {
    id: "more-settings",
    label: meta?.label ?? "Preferences",
    hint: meta?.hint,
    kind: "more",
    more: "settings",
  };
}

export function mobileDestinationSections(): {
  title: string;
  items: MobileDestinationItem[];
}[] {
  return [
    { title: "Go to", items: MOBILE_PRIMARY_DESTINATIONS },
    { title: "More", items: moreDestinationItems() },
  ];
}
