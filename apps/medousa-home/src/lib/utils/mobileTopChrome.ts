import type { MobileTab, MoreDestination } from "$lib/types/mobile";
import type { LibraryView } from "$lib/stores/layout.svelte";

export type MobileChromeActionId =
  | "menu"
  | "back"
  | "workshop"
  | "sessions"
  | "identity"
  | "search"
  | "notesFilter"
  | "newNote"
  | "noteEdit"
  | "noteChat"
  | "noteMore"
  | "browserTabs"
  | "browserBack"
  | "browserForward"
  | "browserReload"
  | "activity";

export type MobileChromeSurface =
  | "home"
  | "chat"
  | "notes"
  | "notes-reader"
  | "web"
  | "more"
  | "more-nested";

export function resolveMobileChromeSurface(
  tab: MobileTab,
  libraryView: LibraryView,
  moreDestination: MoreDestination = "hub",
): MobileChromeSurface {
  if (tab === "notes" && libraryView === "reader") return "notes-reader";
  if (tab === "home") return "home";
  if (tab === "chat") return "chat";
  if (tab === "notes") return "notes";
  if (tab === "web") return "web";
  if (tab === "more") {
    return moreDestination !== "hub" ? "more-nested" : "more";
  }
  return "more";
}

export function mobileChromeLeading(
  surface: MobileChromeSurface,
): MobileChromeActionId {
  return surface === "notes-reader" || surface === "more-nested" ? "back" : "menu";
}

export function mobileChromeTrailing(
  surface: MobileChromeSurface,
): MobileChromeActionId[] {
  switch (surface) {
    case "home":
      return ["workshop"];
    case "chat":
      return ["sessions", "identity"];
    case "notes":
      return ["search", "notesFilter", "newNote"];
    case "notes-reader":
      return ["noteEdit", "noteChat", "noteMore"];
    case "web":
      return ["browserBack", "browserForward", "browserReload", "browserTabs"];
    case "more":
    case "more-nested":
      return ["activity"];
  }
}
