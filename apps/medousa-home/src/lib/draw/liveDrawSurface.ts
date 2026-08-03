import { mount, unmount } from "svelte";
import DrawSurface from "$lib/components/draw/DrawSurface.svelte";
import {
  parseDrawFenceBody,
  serializeDrawFence,
  type DrawDocument,
} from "$lib/draw/drawDocument";

function fenceBody(raw: string): string {
  const open = /^```draw[^\r\n`]*\r?\n/i.exec(raw);
  const close = raw.lastIndexOf("\n```");
  if (open && close >= open[0].length) return raw.slice(open[0].length, close);
  return raw.replace(/^```[^\n]*\n?/i, "").replace(/\n?```\s*$/, "");
}

export type DrawSurfaceHandles = {
  destroy: () => void;
  applyRaw: (raw: string) => void;
};

export function mountDrawSurface(
  host: HTMLElement,
  raw: string,
  onChange: (raw: string) => void,
): DrawSurfaceHandles {
  const target = document.createElement("div");
  target.className = "vault-live-draw";
  host.append(target);

  const instance = mount(DrawSurface, {
    target,
    props: {
      document: parseDrawFenceBody(fenceBody(raw)),
      editable: true,
      variant: "embedded",
      onchange: (next: DrawDocument) => onChange(serializeDrawFence(next)),
    },
  }) as unknown as { applyDocument?: (document: DrawDocument) => void };

  return {
    applyRaw(nextRaw) {
      try {
        instance.applyDocument?.(parseDrawFenceBody(fenceBody(nextRaw)));
      } catch {
        // Keep the last valid scene while raw source is repaired.
      }
    },
    destroy() {
      void unmount(instance);
      target.remove();
    },
  };
}
