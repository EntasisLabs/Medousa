export type InstallerStep = "welcome" | "manage" | "progress" | "complete";

export interface ProfileSummary {
  id: string;
  displayName: string;
  description: string;
  icon: string;
  section: string;
  packages: string[];
  sizeLabel: string;
}

export interface PackageSummary {
  id: string;
  displayName: string;
  category: string;
  categoryLabel: string;
  icon: string;
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
  releaseBaseUrl: string | null;
  releaseChannel: string;
  installerVersion: string;
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
  displayName: string;
  phase: string;
  phaseLabel: string;
  percent: number;
  message: string;
}
