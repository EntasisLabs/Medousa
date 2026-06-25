import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export interface PackageStatusSummary {
  localBrainInstalled: boolean;
  installerAvailable: boolean;
  installedPackages: string[];
  installedVersion: string | null;
  releaseBaseUrl: string | null;
  updateAvailable: boolean;
}

export async function fetchPackageStatus(): Promise<PackageStatusSummary | null> {
  if (!isTauri()) return null;
  try {
    return await invoke<PackageStatusSummary>("packages_status");
  } catch {
    return null;
  }
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
