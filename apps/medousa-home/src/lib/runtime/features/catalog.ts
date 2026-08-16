import type { FeatureDescriptor, FeatureId } from "./types";

/** Metadata only — this module must not import stores, components, or loaders. */
export const FEATURE_CATALOG: readonly FeatureDescriptor[] = [
  {
    id: "shell-desktop",
    destinations: ["chat", "library", "work", "web", "settings"],
    clientPlatforms: ["desktop"],
    requiredCapabilities: [],
    preload: "never",
  },
  {
    id: "shell-mobile",
    destinations: ["home", "chat", "notes", "web", "more"],
    clientPlatforms: ["mobile"],
    requiredCapabilities: [],
    preload: "never",
  },
  {
    id: "vault-browse",
    destinations: ["library", "notes"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: ["vault"],
    preload: "intent",
  },
  {
    id: "vault-edit",
    destinations: ["library", "notes"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: ["vault"],
    preload: "intent",
  },
  {
    id: "code-work",
    destinations: ["code", "work"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "browser",
    destinations: ["web"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "settings",
    destinations: ["settings"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "spotlight",
    destinations: [],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "wizard",
    destinations: [],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "export-import",
    destinations: [],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: ["vault"],
    preload: "never",
  },
  {
    id: "rich-renderers",
    destinations: [],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "terminal",
    destinations: ["terminal"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "calendar",
    destinations: ["calendar"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "map",
    destinations: ["map", "context"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "profiles",
    destinations: ["profiles"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "peers",
    destinations: ["peers"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "messaging",
    destinations: ["messaging"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
  {
    id: "runtime",
    destinations: ["runtime"],
    clientPlatforms: ["desktop", "mobile"],
    requiredCapabilities: [],
    preload: "intent",
  },
];

export function featureDescriptor(id: FeatureId): FeatureDescriptor {
  const match = FEATURE_CATALOG.find((entry) => entry.id === id);
  if (!match) throw new Error(`unknown feature ${id}`);
  return match;
}
