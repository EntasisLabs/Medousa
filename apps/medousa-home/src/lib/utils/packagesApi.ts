import { invoke } from "@tauri-apps/api/core";
import { isTauri } from "$lib/window";

export interface PackageStatusSummary {
  localBrainInstalled: boolean;
  installerAvailable: boolean;
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
    window.open("https://github.com/EntasisLabs/Medousa/releases/latest", "_blank");
    return;
  }
  await invoke("packages_open_installer");
}
