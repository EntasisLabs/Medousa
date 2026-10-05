import { forgeFetch } from "$lib/forge";
import { operationPath } from "$lib/daemon";

export function getUndertakingImage(workId: string, path: string): Promise<{ path: string; mime: string; bytes_base64: string }> {
  const query = new URLSearchParams({ path, image: "true" });
  return forgeFetch(operationPath("forge.items.by_work_id.source.get", { work_id: workId }) + "?" + query);
}

export type CodeMarkdownMode = "source" | "markdown" | "preview" | "split";

/** Resolve relative links on the workshop, never against the Home process. */
export function projectMarkdownTarget(sourcePath: string, href: string): { path: string; hash: string } | null {
  if (!href || /^[a-z][a-z\d+.-]*:/i.test(href) || href.startsWith("//") || href.includes("\\")) return null;
  const hashAt = href.indexOf("#");
  const raw = (hashAt < 0 ? href : href.slice(0, hashAt)).split("?")[0];
  let decoded: string;
  try { decoded = decodeURIComponent(raw); } catch { return null; }
  if (decoded.includes("\\") || decoded.includes("\0")) return null;
  const parts = raw.startsWith("/") ? [] : sourcePath.split("/").slice(0, -1);
  if (!decoded) return { path: sourcePath, hash: hashAt < 0 ? "" : href.slice(hashAt + 1) };
  for (const part of decoded.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (!parts.length) return null;
      parts.pop();
    } else parts.push(part);
  }
  const path = parts.join("/");
  if (!path || parts.includes(".git")) return null;
  return { path, hash: hashAt < 0 ? "" : href.slice(hashAt + 1) };
}

/** Component-owned URLs and requests: closing a document releases all image bytes. */
export class ProjectMarkdownImages {
  private urls = new Map<string, Promise<string>>();
  private owned = new Set<string>();
  private disposed = false;
  constructor(private workId: string, private sourcePath: string) {}

  resolve(raw: string): Promise<string> {
    if (this.disposed) return Promise.reject(new Error("Document closed"));
    if (/^(https?:|data:image\/|blob:)/i.test(raw)) return Promise.resolve(raw);
    const target = projectMarkdownTarget(this.sourcePath, raw);
    if (!target) return Promise.reject(new Error("Image path is outside this project"));
    let request = this.urls.get(target.path);
    if (!request) {
      request = getUndertakingImage(this.workId, target.path).then((image) => {
        if (this.disposed) throw new Error("Document closed");
        const bytes = Uint8Array.from(atob(image.bytes_base64), (char) => char.charCodeAt(0));
        const url = URL.createObjectURL(new Blob([bytes], { type: image.mime }));
        this.owned.add(url);
        return url;
      });
      this.urls.set(target.path, request);
    }
    return request;
  }

  dispose() {
    this.disposed = true;
    for (const url of this.owned) URL.revokeObjectURL(url);
    this.owned.clear();
    this.urls.clear();
  }
}
