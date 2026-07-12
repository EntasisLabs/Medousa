import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { isTauri } from "$lib/window";

export interface PackageStatusSummary {
  localBrainInstalled: boolean;
  installerAvailable: boolean;
  installedPackages: string[];
  installedVersion: string | null;
  releaseBaseUrl: string | null;
  updateAvailable: boolean;
}

export interface HomePackageRow {
  id: string;
  displayName: string;
  hint: string;
  categoryLabel: string;
  installed: boolean;
  installedVersion: string | null;
  availableVersion: string | null;
  updateAvailable: boolean;
  sizeBytes: number;
  optional: boolean;
}

export interface HomePackagesCatalog {
  packages: HomePackageRow[];
  installerAvailable: boolean;
  releaseVersion: string | null;
  releaseBaseUrl: string | null;
}

export interface PackageProgressEvent {
  packageId: string;
  displayName: string;
  phase: string;
  phaseLabel: string;
  percent: number;
  message: string;
}

export async function fetchPackageStatus(): Promise<PackageStatusSummary | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<PackageStatusSummary>("packages_status");
  } catch {
    return null;
  }
}

export async function fetchPackagesCatalog(): Promise<HomePackagesCatalog | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<HomePackagesCatalog>("packages_catalog");
  } catch {
    return null;
  }
}

export async function installPackage(packageId: string): Promise<void> {
  await invoke("packages_install", { packageId });
}

export async function removePackage(packageId: string): Promise<void> {
  await invoke("packages_remove", { packageId });
}

export async function openPackageInstaller(): Promise<void> {
  if (!isTauri()) {
    const base = import.meta.env.VITE_MEDOUSA_RELEASE_BASE_URL as string | undefined;
    if (base) {
      window.open(`${base.replace(/\/$/, "")}/stable/installer-bootstrap.json`, "_blank");
    }
    return;
  }
  await invoke("packages_open_installer");
}

export async function listenPackageProgress(
  onProgress: (event: PackageProgressEvent) => void,
): Promise<UnlistenFn> {
  return listen<PackageProgressEvent>("packages-progress", (event) => {
    onProgress(event.payload);
  });
}

export function formatPackageBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = value >= 10 || unit === 0 ? 0 : 1;
  return `${value.toFixed(digits)} ${units[unit]}`;
}
