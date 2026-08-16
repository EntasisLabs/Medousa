/** Window chrome for the human browser — no store imports. */

export type HumanBrowserSurface = "embed" | "popout";

export function isPopoutBrowserChrome(): boolean {
  return (
    typeof window !== "undefined" &&
    window.location.pathname.includes("/popout/browser-chrome")
  );
}
