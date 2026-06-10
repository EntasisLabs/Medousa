import { isTauri } from "$lib/window";
import {
  guessMimeFromPath,
  type VaultAttachment,
} from "$lib/utils/vaultAttachments";

function fileNameFromPath(path: string): string {
  return path.split("/").pop()?.split("\\").pop() ?? path;
}

export async function pickAttachmentFiles(): Promise<VaultAttachment[]> {
  if (isTauri()) {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: true,
        title: "Link file to note",
      });
      if (!selected) return [];
      const paths = Array.isArray(selected) ? selected : [selected];
      return paths.map((path) => ({
        path,
        label: fileNameFromPath(path),
        mime: guessMimeFromPath(path),
      }));
    } catch {
      return [];
    }
  }

  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = true;
    input.style.display = "none";
    input.addEventListener("change", () => {
      const files = [...(input.files ?? [])];
      input.remove();
      resolve(
        files.map((file) => ({
          path: file.name,
          label: file.name,
          mime: file.type || guessMimeFromPath(file.name),
        })),
      );
    });
    input.addEventListener("cancel", () => {
      input.remove();
      resolve([]);
    });
    document.body.appendChild(input);
    input.click();
  });
}

export async function openAttachmentPath(path: string): Promise<void> {
  if (!path.trim()) return;
  if (isTauri()) {
    const { openPath } = await import("@tauri-apps/plugin-opener");
    await openPath(path);
    return;
  }
  if (path.startsWith("http://") || path.startsWith("https://")) {
    window.open(path, "_blank", "noopener,noreferrer");
  }
}

export async function attachmentPreviewUrl(path: string): Promise<string | null> {
  if (!isTauri() || !path.trim()) return null;
  try {
    const { convertFileSrc } = await import("@tauri-apps/api/core");
    return convertFileSrc(path);
  } catch {
    return null;
  }
}
