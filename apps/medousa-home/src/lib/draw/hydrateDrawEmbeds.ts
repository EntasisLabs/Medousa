import { mount, unmount } from "svelte";
import DrawSurface from "$lib/components/draw/DrawSurface.svelte";
import { parseDrawFenceBody } from "$lib/draw/drawDocument";

type MountedDraw = ReturnType<typeof mount>;
const mountedByRoot = new WeakMap<HTMLElement, MountedDraw[]>();

export function destroyDrawEmbeds(root: HTMLElement): void {
  const mounted = mountedByRoot.get(root) ?? [];
  for (const instance of mounted) void unmount(instance);
  mountedByRoot.delete(root);
}

export function hydrateDrawEmbeds(root: HTMLElement): void {
  destroyDrawEmbeds(root);
  const mounted: MountedDraw[] = [];
  for (const host of root.querySelectorAll<HTMLElement>("[data-draw-embed]")) {
    const source = host.querySelector<HTMLElement>(".medousa-draw-source");
    try {
      const document = parseDrawFenceBody(source?.textContent ?? "");
      host.replaceChildren();
      mounted.push(
        mount(DrawSurface, {
          target: host,
          props: { document, editable: false, variant: "embedded" },
        }),
      );
    } catch (error) {
      host.replaceChildren();
      const message = document.createElement("p");
      message.className = "medousa-draw-error";
      message.textContent = error instanceof Error ? error.message : "Drawing could not be loaded";
      host.append(message);
    }
  }
  if (mounted.length) mountedByRoot.set(root, mounted);
}
