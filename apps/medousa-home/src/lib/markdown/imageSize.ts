/** Obsidian-style image size tokens: `400` or `400x240`. */

export type ImageDisplaySize = {
  width: number;
  height?: number;
};

const SIZE_TOKEN = /^(\d{1,4})(?:x(\d{1,4}))?$/i;

export function parseImageSizeToken(raw: string): ImageDisplaySize | null {
  const match = SIZE_TOKEN.exec(raw.trim());
  if (!match) return null;
  const width = Number(match[1]);
  if (!Number.isFinite(width) || width <= 0) return null;
  if (match[2] == null) return { width };
  const height = Number(match[2]);
  if (!Number.isFinite(height) || height <= 0) return null;
  return { width, height };
}

/** Strip trailing `|size` from a markdown image href. */
export function splitImageHrefSize(href: string): {
  href: string;
  size: ImageDisplaySize | null;
} {
  const trimmed = href.trim();
  const pipe = trimmed.lastIndexOf("|");
  if (pipe <= 0) return { href: trimmed, size: null };
  const size = parseImageSizeToken(trimmed.slice(pipe + 1));
  if (!size) return { href: trimmed, size: null };
  return { href: trimmed.slice(0, pipe).trim(), size };
}

/** Strip trailing `|size` from alt text (`![caption|400](…)`). */
export function splitImageAltSize(alt: string): {
  alt: string;
  size: ImageDisplaySize | null;
} {
  const pipe = alt.lastIndexOf("|");
  if (pipe < 0) return { alt, size: null };
  const size = parseImageSizeToken(alt.slice(pipe + 1));
  if (!size) return { alt, size: null };
  return { alt: alt.slice(0, pipe).trim(), size };
}

export function imageSizeStyle(size: ImageDisplaySize): string {
  const height =
    size.height != null ? `${size.height}px` : "auto";
  return `width:${size.width}px;height:${height};max-width:100%`;
}
