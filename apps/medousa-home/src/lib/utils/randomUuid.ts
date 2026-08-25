type WebCrypto = {
  randomUUID?: () => string;
  getRandomValues?: (bytes: Uint8Array) => Uint8Array;
};

/** UUID v4 that also works in WKWebView versions without crypto.randomUUID. */
export function randomUuid(
  source: WebCrypto | undefined = globalThis.crypto,
): string {
  if (typeof source?.randomUUID === "function") {
    return source.randomUUID();
  }

  const bytes = new Uint8Array(16);
  if (typeof source?.getRandomValues === "function") {
    source.getRandomValues(bytes);
  } else {
    for (let index = 0; index < bytes.length; index += 1) {
      bytes[index] = Math.floor(Math.random() * 256);
    }
  }

  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0"));
  return [
    hex.slice(0, 4),
    hex.slice(4, 6),
    hex.slice(6, 8),
    hex.slice(8, 10),
    hex.slice(10),
  ]
    .map((part) => part.join(""))
    .join("-");
}
