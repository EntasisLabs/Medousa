/** Parse `medousa://pair/1.0?...` and `medousa://pair/2.0?...` QR payloads. */

export interface ParsedPairQr {
  advertiseAddress: string;
  daemonUrl: string;
  deviceId: string;
  qrToken: string;
  signature: string;
  peerName: string;
  protocolVersion: "1.0" | "2.0";
  irohTicket?: string;
  workshopEndpointId?: string;
}

export function advertiseAddressToDaemonUrl(address: string): string {
  const trimmed = address.trim();
  if (trimmed.startsWith("http://") || trimmed.startsWith("https://")) {
    return trimmed.replace(/\/+$/, "");
  }
  return `http://${trimmed.replace(/\/+$/, "")}`;
}

function parseProtocolVersion(pathname: string): "1.0" | "2.0" | null {
  const normalized = pathname.replace(/^\/+/, "").trim();
  if (normalized === "1.0") return "1.0";
  if (normalized === "2.0") return "2.0";
  return null;
}

export function parsePairQrUrl(raw: string): ParsedPairQr | null {
  const trimmed = raw.trim();
  if (!trimmed.startsWith("medousa:")) return null;

  try {
    const url = new URL(trimmed);
    if (url.hostname.toLowerCase() !== "pair") return null;
    const protocolVersion = parseProtocolVersion(url.pathname);
    if (!protocolVersion) return null;

    const advertiseAddress = url.searchParams.get("a");
    const deviceId = url.searchParams.get("d");
    const qrToken = url.searchParams.get("t");
    const signature = url.searchParams.get("s");
    const peerName = url.searchParams.get("n") ?? "Medousa";
    const irohTicket = url.searchParams.get("k") ?? undefined;
    const workshopEndpointId = url.searchParams.get("e") ?? undefined;
    if (!advertiseAddress || !deviceId || !qrToken || !signature) return null;
    if (protocolVersion === "2.0" && !irohTicket) return null;

    return {
      advertiseAddress,
      daemonUrl: advertiseAddressToDaemonUrl(advertiseAddress),
      deviceId,
      qrToken,
      signature,
      peerName: decodeURIComponent(peerName),
      protocolVersion,
      irohTicket,
      workshopEndpointId,
    };
  } catch {
    return null;
  }
}
