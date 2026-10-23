import type { CreativeApi } from "./creativeApi";
/** View-owned URLs: release on unmount, bounded even for scenes with huge images. */
export function creativeAssets(
  api: Pick<CreativeApi, "asset">,
  id: string,
  maxBytes = 32 * 1024 * 1024,
) {
  const urls = new Map<string, { url: string; bytes: number; blob: Blob }>();
  const pending = new Map<string, Promise<string>>();
  let total = 0,
    disposed = false;
  return {
    async get(assetId: string): Promise<string> {
      if (disposed) throw new Error("Canvas closed");
      const existing = urls.get(assetId);
      if (existing) return existing.url;
      const inflight = pending.get(assetId);
      if (inflight) return inflight;
      const request = (async () => {
        const data = await api.asset(id, assetId);
        if (disposed) throw new Error("Canvas closed");
        if (
          !["image/png", "image/jpeg", "image/webp"].includes(data.asset.mime)
        )
          throw new Error("Unsupported canvas image");
        if (total + data.asset.byte_size > maxBytes || urls.size >= 24)
          throw new Error(
            "Canvas image preview limit reached. Export or remove unused image layers to load more.",
          );
        const raw = atob(data.data_base64),
          bytes = Uint8Array.from(raw, (c) => c.charCodeAt(0));
        if (bytes.length !== data.asset.byte_size)
          throw new Error("Incomplete canvas image");
        const blob = new Blob([bytes], { type: data.asset.mime }),
          url = URL.createObjectURL(blob);
        total += bytes.length;
        urls.set(assetId, { url, bytes: bytes.length, blob });
        return url;
      })();
      pending.set(assetId, request);
      try {
        return await request;
      } finally {
        pending.delete(assetId);
      }
    },
    blob(assetId: string) {
      return urls.get(assetId)?.blob;
    },
    retainOnly(ids: Set<string>) {
      for (const [id, value] of urls)
        if (!ids.has(id)) {
          URL.revokeObjectURL(value.url);
          total -= value.bytes;
          urls.delete(id);
        }
    },
    dispose() {
      disposed = true;
      for (const value of urls.values()) URL.revokeObjectURL(value.url);
      urls.clear();
      pending.clear();
      total = 0;
    },
  };
}
export type CreativeAssets = ReturnType<typeof creativeAssets>;
