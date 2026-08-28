import {
  MORE_DESTINATIONS,
  MORE_HUB_SECTIONS,
  type MobileTab,
  type MoreDestination,
} from "$lib/types/mobile";
import type { EnvironmentSpec } from "$lib/types/environment";
import { activePresetSurfaceIds } from "$lib/utils/environmentLayout";

export type MobileDestinationKind = "tab" | "more";

export type MobileDestinationItem = {
  id: string;
  label: string;
  hint?: string;
  kind: MobileDestinationKind;
  tab?: MobileTab;
  more?: Exclude<MoreDestination, "hub">;
  /** Shared environment surface controlling membership in the active layout. */
  surfaceId?: string;
  /** Utility doors that remain available even when absent from a preset. */
  pinned?: boolean;
};

/** Primary tabs shown at the top of the destinations menu. */
export const MOBILE_PRIMARY_DESTINATIONS: MobileDestinationItem[] = [
  {
    id: "tab-home",
    label: "Home",
    hint: "Glance & continue",
    kind: "tab",
    tab: "home",
    surfaceId: "home",
    pinned: true,
  },
  {
    id: "tab-chat",
    label: "Chat",
    hint: "Sessions and replies",
    kind: "tab",
    tab: "chat",
    surfaceId: "chat",
  },
  {
    id: "tab-notes",
    label: "Notes",
    hint: "Vault library",
    kind: "tab",
    tab: "notes",
    surfaceId: "notes",
  },
  {
    id: "more-code",
    label: "Code",
    hint: "Projects, files, and agents",
    kind: "more",
    more: "code",
    surfaceId: "code",
  },
  {
    id: "tab-web",
    label: "Web",
    hint: "Browser",
    kind: "tab",
    tab: "web",
    surfaceId: "web",
  },
  {
    id: "more-calendar",
    label: "Calendar",
    hint: "Meetings, reminders & .ics",
    kind: "more",
    more: "calendar",
    surfaceId: "calendar",
  },
];

const MORE_SURFACE_IDS: Partial<
  Record<Exclude<MoreDestination, "hub">, string>
> = {
  map: "map",
  workshop: "workshop",
  automations: "automations",
  code: "code",
  calendar: "calendar",
  messaging: "messaging",
  peers: "peers",
  settings: "settings",
  runtime: "runtime",
};

const PINNED_MORE_DESTINATIONS = new Set<Exclude<MoreDestination, "hub">>([
  "profiles",
  "workshop",
  "settings",
  "runtime",
]);

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
          surfaceId: MORE_SURFACE_IDS[id],
          pinned: PINNED_MORE_DESTINATIONS.has(id),
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
    surfaceId: "settings",
    pinned: true,
  };
}

function surfaceOrder(spec: EnvironmentSpec): Map<string, number> {
  return new Map(activePresetSurfaceIds(spec).map((id, index) => [id, index]));
}

function sortByActiveLayout(
  items: MobileDestinationItem[],
  order: Map<string, number>,
): MobileDestinationItem[] {
  return items
    .map((item, catalogIndex) => ({ item, catalogIndex }))
    .sort((left, right) => {
      const leftIndex = left.item.surfaceId
        ? (order.get(left.item.surfaceId) ?? Number.MAX_SAFE_INTEGER)
        : Number.MAX_SAFE_INTEGER;
      const rightIndex = right.item.surfaceId
        ? (order.get(right.item.surfaceId) ?? Number.MAX_SAFE_INTEGER)
        : Number.MAX_SAFE_INTEGER;
      return leftIndex - rightIndex || left.catalogIndex - right.catalogIndex;
    })
    .map(({ item }) => item);
}

function visibleItems(
  items: MobileDestinationItem[],
  spec: EnvironmentSpec | null | undefined,
  includeHidden: boolean,
): MobileDestinationItem[] {
  if (!spec || includeHidden) return [...items];
  const visibleIds = new Set(activePresetSurfaceIds(spec));
  return items.filter(
    (item) => item.pinned || !item.surfaceId || visibleIds.has(item.surfaceId),
  );
}

export type MobileDestinationSection = {
  title: string;
  items: MobileDestinationItem[];
};

/**
 * Mobile projection of the active layout preset. The catalog stays mobile-safe;
 * shared preset membership and order decide which of those doors appear.
 */
export function mobileDestinationSections(
  spec?: EnvironmentSpec | null,
  options: { includeHidden?: boolean } = {},
): MobileDestinationSection[] {
  const includeHidden = options.includeHidden ?? false;
  const order = spec ? surfaceOrder(spec) : new Map<string, number>();
  const primary = visibleItems(MOBILE_PRIMARY_DESTINATIONS, spec, includeHidden);
  const home = primary.filter((item) => item.id === "tab-home");
  const primaryMovable = sortByActiveLayout(
    primary.filter((item) => item.id !== "tab-home"),
    order,
  );

  const more = visibleItems(moreDestinationItems(), spec, includeHidden);
  const you = more.filter((item) => item.more === "profiles");
  const runtime = more.filter((item) => item.more === "runtime");
  const moreMovable = sortByActiveLayout(
    more.filter((item) => item.more !== "profiles" && item.more !== "runtime"),
    order,
  );

  return [
    { title: "Go to", items: [...home, ...primaryMovable] },
    { title: "More", items: [...you, ...moreMovable, ...runtime] },
  ];
}

/** Built-in mobile doors the active layout may show or hide. */
export function mobileEditableDestinationItems(
  spec: EnvironmentSpec,
): MobileDestinationItem[] {
  const definedSurfaceIds = new Set(spec.surfaces.map((surface) => surface.id));
  const catalog = [
    ...MOBILE_PRIMARY_DESTINATIONS,
    ...moreDestinationItems(),
    settingsDestinationItem(),
  ];
  return sortByActiveLayout(
    catalog.filter(
      (item) =>
        !item.pinned &&
        Boolean(item.surfaceId) &&
        definedSurfaceIds.has(item.surfaceId!),
    ),
    surfaceOrder(spec),
  );
}
