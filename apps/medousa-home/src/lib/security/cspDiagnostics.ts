let installed = false;

export function classifyCspBlockedSource(blockedUri: string): string {
  const value = blockedUri.trim().toLowerCase();
  if (!value) return "none";
  if (value === "inline" || value === "eval") return value;
  if (value.startsWith("data:")) return "data";
  if (value.startsWith("blob:")) return "blob";
  try {
    const url = new URL(value);
    if (url.protocol === "http:" || url.protocol === "https:") return url.protocol.slice(0, -1);
    if (url.protocol === "tauri:" || url.protocol === "asset:") return "local-protocol";
    return "other-scheme";
  } catch {
    return "opaque";
  }
}

/** Capture policy regressions without logging a blocked URL, path, query, or payload. */
export function installCspViolationDiagnostics(): void {
  if (installed || typeof document === "undefined") return;
  installed = true;
  document.addEventListener("securitypolicyviolation", (event) => {
    console.warn("[medousa-security] CSP blocked content", {
      directive: event.effectiveDirective,
      sourceClass: classifyCspBlockedSource(event.blockedURI),
      disposition: event.disposition,
    });
  });
}
