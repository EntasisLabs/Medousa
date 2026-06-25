export type Screen = "configure" | "hub" | "progress" | "complete";
export type HubTab = "workloads" | "components" | "locations";
export type ConfigureMode = "express" | "existing" | "manual";

export interface ProfileSummary {
  id: string;
  displayName: string;
  description: string;
  packages: string[];
  sizeLabel: string;
}

export interface PackageSummary {
  id: string;
  displayName: string;
  category: string;
  depends: string[];
  binaries: string[];
  sizeLabel: string;
  sizeBytes: number;
  optional: boolean;
  selected: boolean;
  installed: boolean;
  updateAvailable: boolean;
  installedVersion: string | null;
  remoteVersion: string | null;
}

export interface BootstrapResponse {
  installRoot: string;
  dataDir: string;
  modelCacheDir: string;
  releaseManifestUrl: string;
  releaseBaseUrl: string | null;
  releaseChannel: string;
  profiles: ProfileSummary[];
  packages: PackageSummary[];
  modifyMode: boolean;
  installedVersion: string | null;
  remoteVersion: string | null;
  versionMismatch: boolean;
}

export interface SidebarNode {
  id: string;
  label: string;
  included: boolean;
  optional: boolean;
  children: SidebarNode[];
}

export interface ResolveSelectionResponse {
  expandedPackageIds: string[];
  totalBytes: number;
  sizeLabel: string;
  tree: SidebarNode[];
  warnings: string[];
}

export interface DownloadProgress {
  packageId: string;
  phase: string;
  percent: number;
  message: string;
}
