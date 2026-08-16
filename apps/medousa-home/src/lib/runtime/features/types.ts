export type FeatureId =
  | "shell-desktop"
  | "shell-mobile"
  | "vault-browse"
  | "vault-edit"
  | "code-work"
  | "browser"
  | "settings"
  | "spotlight"
  | "wizard"
  | "export-import"
  | "rich-renderers"
  | "terminal"
  | "calendar"
  | "map"
  | "profiles"
  | "peers"
  | "messaging"
  | "runtime";

export type ClientPlatform = "desktop" | "mobile";

export type FeaturePreload = "never" | "intent" | "post-interaction-idle";

export type DisposeReason = "cancelled" | "start-failed" | "replaced" | "teardown" | "evicted";

export interface FeatureDescriptor {
  id: FeatureId;
  destinations: readonly string[];
  clientPlatforms: readonly ClientPlatform[];
  requiredCapabilities: readonly string[];
  preload: FeaturePreload;
}

export interface FeatureContext {
  platform: ClientPlatform;
  signal: AbortSignal;
  /** Register a partial instance so the loader can dispose if start throws. */
  track(instance: FeatureInstance): void;
}

export interface FeatureInstance {
  dispose(reason: DisposeReason): Promise<void> | void;
}

export interface FeatureModule {
  start(context: FeatureContext): Promise<FeatureInstance>;
}

export type FeatureModuleLoader = () => Promise<FeatureModule>;
