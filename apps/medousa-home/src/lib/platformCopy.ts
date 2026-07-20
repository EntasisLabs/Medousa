import {
  isTauriMacDesktop,
  isTauriMobilePlatform,
} from "$lib/platform";

/** Phone/tablet shell talking to a remote workshop daemon. */
export function isCompanionShell(): boolean {
  return isTauriMobilePlatform();
}

function isLikelyWindowsHost(): boolean {
  if (typeof navigator === "undefined") return false;
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData
      ?.platform ??
    navigator.platform ??
    "";
  return /Win/i.test(platform) || /Windows/i.test(navigator.userAgent);
}

/** OS-adaptive noun for the machine running Medousa (desktop host). */
export function hostComputerNoun(): string {
  if (isCompanionShell()) return "computer";
  if (isTauriMacDesktop()) return "Mac";
  if (isLikelyWindowsHost()) return "PC";
  return "computer";
}

export function hostComputerPhrase(): string {
  return `your ${hostComputerNoun()}`;
}

/** File manager name for reveal/pin copy. */
export function hostFileManagerNoun(): string {
  if (isTauriMacDesktop()) return "Finder";
  if (isLikelyWindowsHost()) return "File Explorer";
  return "file manager";
}

/** User-facing noun for the machine running the Medousa daemon. */
export function workshopHostNoun(): string {
  return "workshop";
}

/** "your workshop" — host that owns charter + daemon defaults. */
export function workshopHostPhrase(): string {
  return `your ${workshopHostNoun()}`;
}

/** Short badge label for companion → host routing (not a platform name). */
export function workshopHostBadge(): string {
  return "Workshop";
}

export function workshopConfigOnHostHint(): string {
  return `Configured on ${workshopHostPhrase()}. Change Models and Voice in Settings on the host, or edit tui_defaults.json.`;
}

export function workshopCharterOnHostHint(): string {
  return `Workshop charter lives on the host daemon. Change Memory and Voice in Settings on the host, or edit tui_defaults.json.`;
}

export function workshopModelOnHostHint(): string {
  return `Model is set on ${workshopHostPhrase()}`;
}

export function connectToWorkshopHint(): string {
  return `Connect to ${workshopHostPhrase()} to chat`;
}

export function workshopDaemonDefaultsLabel(): string {
  return "Workshop defaults";
}

export function companionTurnRoutingHint(): string {
  return `This device sends turns to ${workshopHostPhrase()} — model and routing live there.`;
}

export function mobileComposerRoutingHint(): string {
  return `Model and stance update ${workshopHostPhrase()} — your next message uses what you pick here.`;
}

export function workshopPairingFromHostHint(): string {
  return `Paste the pairing link from ${workshopHostPhrase()}, or switch to Enter address.`;
}

export function workshopQrScanHint(): string {
  return `Scan the QR on ${workshopHostPhrase()} — it should open Medousa and fill the pairing link here`;
}

export function workshopPairingStepsHint(): string {
  return `Pairing link: open your camera app, scan the QR on ${workshopHostPhrase()}, tap the banner, copy the link`;
}

export function workshopPairingManagedHint(): string {
  return `Pairing is managed on ${workshopHostPhrase()}. Open Medousa → Settings → Phone to show the QR code.`;
}

export function workshopRetentionReadHint(): string {
  return "Managed on the workshop host — open Settings → Rhythm there to change.";
}

export function workshopRetentionLocalHint(): string {
  return "Saved with workshop defaults on the host.";
}

export function workshopRuntimeReadHint(): string {
  return "Read from workshop daemon after connect";
}

export function workshopRuntimeRoutingHint(): string {
  return `Model and stage routing are configured on ${workshopHostPhrase()}. This device sends turns to the daemon — change Voice and Reach in host Settings, or edit tui_defaults.json.`;
}

export function workshopBasementConnectionLabel(mobile: boolean): string {
  return mobile ? "Workshop (local)" : "Local workshop";
}

export function workshopBasementRestartHint(): string {
  return `Restarts Medousa on your network so QR pairing and companion apps can reach ${hostComputerPhrase()}`;
}

export function localBrainOnDeviceHint(): string {
  return isCompanionShell()
    ? `Optional local Gemma engine on ${hostComputerPhrase()} — separate from cloud chat models.`
    : `Optional local Gemma engine on ${hostComputerPhrase()} — separate from cloud chat models.`;
}

export function vaultPinFolderHint(): string {
  return isCompanionShell()
    ? `Your real files live outside the vault too. Pin a folder from the host — Documents, Desktop, or a project root.`
    : `Your real files live outside the vault too. Pin a folder on ${hostComputerPhrase()} — Documents, Desktop, or a project root.`;
}

export function vaultRemoteFilesystemHint(): string {
  return `This workshop’s files live on the host ${hostComputerNoun()}. Pin folders and reveal in ${hostFileManagerNoun()} there.`;
}

export function revealInFileManagerLabel(): string {
  if (isTauriMacDesktop()) return "Reveal in Finder";
  if (isLikelyWindowsHost()) return "Show in File Explorer";
  return "Show in folder";
}
