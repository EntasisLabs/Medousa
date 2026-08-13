import type { MobileTab, MoreDestination } from "$lib/types/mobile";
import type { LibraryView } from "$lib/stores/layout.svelte";
import type {
  AutomationsChromeMode,
  AutomationsSection,
} from "$lib/stores/automationsNav.svelte";
import type { MobileCodeChromeMode } from "$lib/utils/mobileCodeLanding";

export type { AutomationsChromeMode, MobileCodeChromeMode };

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
  | "automationsFilter"
  | "newAutomation"
  | "scriptTools"
  | "scriptSave"
  | "scriptRun"
  | "scriptCompile"
  | "flowAddStep"
  | "flowPlan"
  | "flowRun"
  | "flowSchedule"
  | "flowClose"
  | "agentsFilter"
  | "agentsImport"
  | "browserTabs"
  | "browserBack"
  | "browserForward"
  | "browserReload"
  | "activity"
  | "codeSearch"
  | "codeSave"
  | "codeFind"
  | "codeThread";

export type MobileChromeSurface =
  | "home"
  | "chat"
  | "notes"
  | "notes-reader"
  | "web"
  | "more"
  | "more-nested"
  | "automations"
  | "agents"
  | "code";

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
    if (moreDestination === "automations") return "automations";
    if (moreDestination === "workshop") return "agents";
    if (moreDestination === "code") return "code";
    return moreDestination !== "hub" ? "more-nested" : "more";
  }
  return "more";
}

export function mobileChromeLeading(
  surface: MobileChromeSurface,
): MobileChromeActionId | null {
  if (surface === "home") return null;
  return surface === "notes-reader" ||
    surface === "more-nested" ||
    surface === "automations" ||
    surface === "agents" ||
    surface === "code"
    ? "back"
    : "menu";
}

export function mobileChromeTrailing(
  surface: MobileChromeSurface,
  automationsSection: AutomationsSection = "scripts",
  automationsMode: AutomationsChromeMode = "browse",
  codeMode: MobileCodeChromeMode = "projects",
): MobileChromeActionId[] {
  switch (surface) {
    case "home":
      return ["menu"];
    case "chat":
      return ["sessions", "identity"];
    case "notes":
      return ["search", "notesFilter", "newNote"];
    case "notes-reader":
      return ["noteEdit", "noteChat", "noteMore"];
    case "web":
      return ["browserBack", "browserForward", "browserReload", "browserTabs"];
    case "automations":
      if (automationsMode === "flow-editor") {
        return ["flowAddStep", "flowPlan", "flowRun", "flowSchedule", "flowClose"];
      }
      if (automationsMode === "script-editor") {
        return [
          "automationsFilter",
          "scriptSave",
          "scriptRun",
          "scriptCompile",
          "scriptTools",
        ];
      }
      switch (automationsSection) {
        case "scripts":
          return ["search", "automationsFilter", "scriptTools"];
        case "flows":
        case "schedules":
          return ["search", "automationsFilter", "newAutomation"];
        case "history":
          return ["search", "automationsFilter"];
      }
      return ["search", "automationsFilter"];
    case "agents":
      return ["search", "agentsFilter", "agentsImport"];
    case "code":
      switch (codeMode) {
        case "projects":
          return ["codeSearch"];
        case "files":
          return ["codeSearch", "codeThread"];
        case "editor":
          return ["codeSave", "codeFind", "codeThread"];
        case "terminal":
        case "changes":
          return ["codeThread"];
      }
      return ["codeThread"];
    case "more":
    case "more-nested":
      return ["activity"];
  }
}
