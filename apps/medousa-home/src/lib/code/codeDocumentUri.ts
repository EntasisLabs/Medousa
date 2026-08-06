/** Keep URI identity stable without resolving through the Home device's disk. */
export function canonicalCodeDocumentUri(uri: string): string {
  try {
    const parsed = new URL(uri);
    parsed.protocol = parsed.protocol.toLowerCase();
    parsed.hostname = parsed.hostname.toLowerCase();
    if (parsed.protocol === "file:") {
      if (parsed.hostname === "localhost") parsed.hostname = "";
      parsed.hash = "";
      parsed.search = "";
    }
    return parsed.href;
  } catch {
    return uri;
  }
}

/** Convert a workshop-owned absolute path to an LSP file URI. */
export function pathToFileUri(path: string): string {
  const normalized = path.replaceAll("\\", "/");
  if (normalized.startsWith("//")) {
    const hostEnd = normalized.indexOf("/", 2);
    const host = hostEnd < 0 ? normalized.slice(2) : normalized.slice(2, hostEnd);
    const pathname = hostEnd < 0 ? "/" : normalized.slice(hostEnd);
    const uri = new URL(`file://${host}`);
    uri.pathname = pathname;
    return uri.href;
  }
  const prefixed = normalized.startsWith("/") ? normalized : `/${normalized}`;
  const uri = new URL("file://");
  uri.pathname = prefixed;
  return uri.href;
}

function normalizeWorkshopPath(path: string): string {
  const normalized = path.replaceAll("\\", "/");
  return normalized.length > 1 ? normalized.replace(/\/+$/, "") : normalized;
}

/**
 * Resolve a server-returned file URI to a project-relative workshop path.
 * Returns null for another scheme, another root, or an ambiguous encoded path.
 */
export function workspaceRelativePathFromUri(
  uri: string,
  workspaceRoot: string,
): string | null {
  try {
    const parsed = new URL(uri);
    if (parsed.protocol.toLowerCase() !== "file:") return null;
    if (/%2f|%5c/i.test(parsed.pathname)) return null;
    let absolutePath = decodeURIComponent(parsed.pathname).replaceAll("\\", "/");
    if (absolutePath.includes("\0")) return null;
    if (parsed.hostname && parsed.hostname !== "localhost") {
      absolutePath = `//${parsed.hostname}${absolutePath}`;
    }

    let root = normalizeWorkshopPath(workspaceRoot);
    absolutePath = normalizeWorkshopPath(absolutePath);
    if (/^[A-Za-z]:\//.test(root) && /^\/[A-Za-z]:\//.test(absolutePath)) {
      absolutePath = absolutePath.slice(1);
    }

    const caseInsensitive = /^[A-Za-z]:\//.test(root) || root.startsWith("//");
    const comparedRoot = caseInsensitive ? root.toLowerCase() : root;
    const comparedPath = caseInsensitive ? absolutePath.toLowerCase() : absolutePath;
    if (!comparedPath.startsWith(`${comparedRoot}/`)) return null;

    const relative = absolutePath.slice(root.length + 1);
    if (
      !relative ||
      relative.startsWith("/") ||
      relative.split("/").some((segment) => !segment || segment === "." || segment === "..")
    ) {
      return null;
    }
    return relative;
  } catch {
    return null;
  }
}

/** Accept a server-selected language root only when it stays in the project. */
export function validatedCodeLanguageRootUri(
  candidateUri: string,
  workspaceRoot: string,
): string | null {
  const candidate = canonicalCodeDocumentUri(candidateUri);
  const project = canonicalCodeDocumentUri(pathToFileUri(workspaceRoot));
  if (candidate === project) return candidate;
  return workspaceRelativePathFromUri(candidate, workspaceRoot) ? candidate : null;
}
