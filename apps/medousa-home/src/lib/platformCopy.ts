import { isTauriMobilePlatform } from "$lib/platform";

/** Phone/tablet shell talking to a remote workshop daemon. */
export function isCompanionShell(): boolean {
  return isTauriMobilePlatform();
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
  return `Scan the QR on ${workshopHostPhrase()} with your camera app, then paste the medousa:// link here`;
}

export function workshopPairingStepsHint(): string {
  return `Pairing link: open your camera app, scan the QR on ${workshopHostPhrase()}, tap the banner, copy the link`;
}

export function workshopPairingManagedHint(): string {
  return `Pairing is managed on ${workshopHostPhrase()}. Open Medousa → Settings → Phone to show the QR code.`;
}

export function workshopRetentionReadHint(): string {
  return "Read from the workshop daemon after connect. Edit on the host in Settings → Rhythm or in tui_defaults.json.";
}

export function workshopRetentionLocalHint(): string {
  return "on the host.";
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
  return "Restarts Medousa on your network so QR pairing and companion apps can reach this machine";
}

export function workshopDefaultsMirrorHint(): string {
  return "Read-only snapshot from the host — day-to-day charter is in Settings → Memory & Voice.";
}

export function localBrainOnDeviceHint(): string {
  return isCompanionShell()
    ? "Optional local Gemma engine on the host — separate from cloud chat models."
    : "Optional local Gemma engine on this machine — separate from cloud chat models.";
}

export function vaultPinFolderHint(): string {
  return isCompanionShell()
    ? "Your real files live outside the vault too. Pin a folder from the host — Documents, Desktop, or a project root."
    : "Your real files live outside the vault too. Pin a folder — Documents, Desktop, or a project root.";
}
