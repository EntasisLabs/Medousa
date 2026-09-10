import { imageFilesFromDataTransfer } from "$lib/utils/vaultImagePaste";

/** Capture File objects before the event's DataTransfer becomes inaccessible. */
export function handleChatImagePaste(
  event: ClipboardEvent,
  options: { blocked: boolean; attach: (files: File[]) => void },
): void {
  const files = imageFilesFromDataTransfer(event.clipboardData);
  if (!files.length) return; // Let the textarea paste text normally, at its caret.
  if (options.blocked) {
    event.preventDefault();
    return;
  }
  // Mixed text + images retain the browser's native text/selection behavior.
  if (!event.clipboardData?.getData("text/plain")) event.preventDefault();
  event.stopPropagation();
  options.attach(files);
}

export function canReadClipboardImages(): boolean {
  return typeof navigator !== "undefined" && typeof navigator.clipboard?.read === "function";
}

/** Must be called directly from the user's click, without an intervening await. */
export async function readClipboardImages(maxImages: number): Promise<File[]> {
  if (!canReadClipboardImages()) {
    throw new Error("Use Paste in the message field, or attach the image with +.");
  }
  let items: ClipboardItem[];
  try {
    items = await navigator.clipboard.read();
  } catch {
    throw new Error("Couldn’t read the clipboard. Choose Paste if prompted, or attach the image with +.");
  }
  const files: File[] = [];
  for (const item of items) {
    // A single clipboard image can offer several representations. Attach it once.
    const type = item.types.includes("image/png")
      ? "image/png"
      : item.types.find((type) => type.startsWith("image/"));
    if (!type) continue;
    if (files.length >= maxImages) {
      throw new Error(`Only ${maxImages} more image${maxImages === 1 ? "" : "s"} can be attached to this message.`);
    }
    const blob = await item.getType(type);
    if (!blob.size) throw new Error("The copied image is empty. Copy it again or attach it with +.");
    const extension = type.split("/")[1].replace(/[^a-z0-9]/gi, "") || "png";
    files.push(new File([blob], `Pasted image ${files.length + 1}.${extension}`, { type }));
  }
  if (!files.length) throw new Error("No image on the clipboard. Copy an image first, or attach one with +.");
  return files;
}
