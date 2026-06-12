/** Parse `medousa://pair/1.0?...` QR payloads from the desktop pairing screen. */

export interface ParsedPairQr {
  advertiseAddress: string;
  daemonUrl: string;
  deviceId: string;
  qrToken: string;
  signature: string;
  peerName: string;
}

export function advertiseAddressToDaemonUrl(address: string): string {
  const trimmed = address.trim();
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
    return trimmed.replace(/\/+$/, "");
  }
  return `http://${trimmed.replace(/\/+$/, "")}`;
}

export function parsePairQrUrl(raw: string): ParsedPairQr | null {
  const trimmed = raw.trim();
  if (!trimmed.startsWith("medousa:")) return null;

  try {
    const url = new URL(trimmed);
    if (url.hostname.toLowerCase() !== "pair") return null;

    const advertiseAddress = url.searchParams.get("a");
    const deviceId = url.searchParams.get("d");
    const qrToken = url.searchParams.get("t");
    const signature = url.searchParams.get("s");
    const peerName = url.searchParams.get("n") ?? "Medousa";
    if (!advertiseAddress || !deviceId || !qrToken || !signature) return null;

    return {
      advertiseAddress,
      daemonUrl: advertiseAddressToDaemonUrl(advertiseAddress),
      deviceId,
      qrToken,
      signature,
      peerName: decodeURIComponent(peerName),
    };
  } catch {
    return null;
  }
}
