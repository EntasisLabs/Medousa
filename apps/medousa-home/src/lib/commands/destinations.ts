import type { Surface } from "$lib/types/ui";

export const GO_DESTINATIONS: { surface: Surface; label: string; subtitle: string; keywords: string }[] = [
  { surface: "chat", label: "Chat", subtitle: "Talk with Medousa", keywords: "message compose conversation" },
  { surface: "create", label: "Create", subtitle: "Canvases and visual work", keywords: "create canvas design drawing" },
  { surface: "notes", label: "Notes", subtitle: "Vault notes and markdown", keywords: "vault notes documents library" },
  { surface: "files", label: "Files", subtitle: "Pinned folders on this machine", keywords: "files folders pin local" },
  { surface: "artifacts", label: "Artifacts", subtitle: "HTML decks and agent widgets", keywords: "artifacts presentations decks widgets" },
  { surface: "code", label: "Code", subtitle: "Projects, editing, understanding, and review", keywords: "code project repository editor forge test review source" },
  { surface: "reviews", label: "Reviews", subtitle: "Review prepared project changes", keywords: "forge code review changes approve feedback" },
  { surface: "web", label: "Browser", subtitle: "Built-in web workshop", keywords: "browser web surf" },
  { surface: "automations", label: "Automations", subtitle: "Saved prompts, scripts, and flows", keywords: "cron scripts grapheme" },
  { surface: "workshop", label: "Agents", subtitle: "Browse and edit specialists", keywords: "skills manuscripts workshop capabilities specialist agents" },
  { surface: "map", label: "Context", subtitle: "Browse saved session memory and sources", keywords: "memory locus context map moments" },
  { surface: "peers", label: "Peers", subtitle: "Nearby workshops and inbox", keywords: "peers nearby share trust lan inbox" },
  { surface: "profiles", label: "Profiles", subtitle: "People and identity", keywords: "profiles identity people" },
  { surface: "runtime", label: "Engine status", subtitle: "Jobs, delivery, health", keywords: "runtime engine daemon health" },
  { surface: "settings", label: "Settings", subtitle: "Preferences and connection", keywords: "settings preferences config" },
];
