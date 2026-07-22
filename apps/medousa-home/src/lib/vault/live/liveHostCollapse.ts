import { LIVE_ICON_CHEVRON_DOWN, LIVE_ICON_CHEVRON_UP } from "./liveIcons";

export type HostCollapseOptions = {
  label?: string;
  /** When true, start collapsed. */
  defaultCollapsed?: boolean;
};

/**
 * Add a session-local collapse toggle to a Live host (table / medousa-view).
 * Returns a disposer. Missing host → no-op.
 */
export function attachHostCollapse(
  host: HTMLElement | null | undefined,
  options: HostCollapseOptions = {},
): () => void {
  if (!host || !(host instanceof HTMLElement)) return () => {};
  if (host.dataset.liveCollapseBound === "1") return () => {};

  host.dataset.liveCollapseBound = "1";
  host.classList.add("vault-live-collapsible-host");

  const bar = document.createElement("div");
  bar.className = "vault-live-host-collapse-bar";

  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "vault-live-host-collapse-btn";
  const label = options.label?.trim() || "Section";
  btn.setAttribute("aria-label", `Collapse ${label}`);

  const title = document.createElement("span");
  title.className = "vault-live-host-collapse-label";
  title.textContent = label;

  bar.append(btn, title);

  let collapsed = Boolean(options.defaultCollapsed);
  const sync = () => {
    host.classList.toggle("vault-live-collapsible-host--collapsed", collapsed);
    btn.setAttribute("aria-expanded", collapsed ? "false" : "true");
    btn.setAttribute("aria-label", collapsed ? `Expand ${label}` : `Collapse ${label}`);
    btn.innerHTML = collapsed ? LIVE_ICON_CHEVRON_DOWN : LIVE_ICON_CHEVRON_UP;
  };
  sync();

  const onClick = (event: Event) => {
    event.preventDefault();
    event.stopPropagation();
    collapsed = !collapsed;
    sync();
  };
  btn.addEventListener("click", onClick);

  host.prepend(bar);

  return () => {
    btn.removeEventListener("click", onClick);
    bar.remove();
    host.classList.remove(
      "vault-live-collapsible-host",
      "vault-live-collapsible-host--collapsed",
    );
    delete host.dataset.liveCollapseBound;
  };
}
