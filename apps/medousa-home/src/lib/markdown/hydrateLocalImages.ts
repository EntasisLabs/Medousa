/** Hydrate local markdown image paths in vault preview containers. */

import { resolveLocalImagePreviewUrl } from "$lib/utils/vaultLocalImages";

export async function hydrateLocalImages(
  container: HTMLElement,
  sourcePath: string | null,
): Promise<void> {
  const images = container.querySelectorAll<HTMLImageElement>("img[data-local-image]");
  await Promise.all(
    [...images].map(async (img) => {
      const raw = img.getAttribute("data-local-image");
      if (!raw) return;

      const url = await resolveLocalImagePreviewUrl(raw, sourcePath);
      if (!url) {
        img.classList.add("markdown-local-image--missing");
        img.alt = img.alt || "Image unavailable";
        return;
      }

      img.src = url;
      img.decoding = "async";
      img.loading = "lazy";
      img.onerror = () => {
        img.classList.add("markdown-local-image--missing");
      };
    }),
  );
}
