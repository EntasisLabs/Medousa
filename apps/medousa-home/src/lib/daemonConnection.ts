import { getDaemonUrl, setDaemonUrl } from "$lib/daemon";
import { isTauriMobilePlatform } from "$lib/platform";

const DAEMON_PORT = 7419;

export function isLoopbackDaemonUrl(url: string): boolean {
  try {
    const { hostname } = new URL(url);
    return hostname === "localhost" || hostname === "127.0.0.1" || hostname === "::1";
  } catch {
    return false;
  }
}

/** During `tauri ios dev`, Vite is served from the Mac LAN IP — use the same host for the daemon. */
export function inferDevDaemonUrl(): string | null {
  if (typeof window === "undefined") return null;
  const { hostname, protocol } = window.location;
  if (protocol !== "http:" && protocol !== "https:") return null;
  if (isLoopbackDaemonUrl(`http://${hostname}:${DAEMON_PORT}`)) return null;
  return `http://${hostname}:${DAEMON_PORT}`;
}

/**
 * On phone/tablet, replace loopback defaults with the dev-server host or a previously saved URL.
 */
export async function ensureMobileDaemonUrl(): Promise<string> {
  const current = (await getDaemonUrl()).trim();
  if (!isTauriMobilePlatform()) return current;

  if (!isLoopbackDaemonUrl(current)) return current;

  const inferred = inferDevDaemonUrl();
  if (!inferred) return current;

  await setDaemonUrl(inferred);
  return inferred;
}
