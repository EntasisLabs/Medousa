import { isTauri } from "$lib/window";
import {
  guessMimeFromPath,
  type VaultAttachment,
} from "$lib/utils/vaultAttachments";

function fileNameFromPath(path: string): string {
  return path.split("/").pop()?.split("\\").pop() ?? path;
}

export async function pickAttachmentFiles(): Promise<VaultAttachment[]> {
  if (!isTauri()) {
    return pickAttachmentFilesWeb();
  }
  const { isCoLocatedWorkshop } = await import("$lib/utils/workshopLocality");
  if (!isCoLocatedWorkshop()) return [];
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

export async function pickSpreadsheetFiles(): Promise<VaultAttachment[]> {
  if (!isTauri()) {
    return pickAttachmentFilesWeb([".csv", ".tsv", ".xlsx", ".xls", ".xlsm"]);
  }
  const { isCoLocatedWorkshop } = await import("$lib/utils/workshopLocality");
  if (!isCoLocatedWorkshop()) return [];
  try {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      multiple: true,
      title: "Link spreadsheet to note",
      filters: [
        {
          name: "Spreadsheets",
          extensions: ["csv", "tsv", "xlsx", "xls", "xlsm"],
        },
      ],
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

function pickAttachmentFilesWeb(accept?: string[]): Promise<VaultAttachment[]> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.multiple = true;
    if (accept?.length) {
      input.accept = accept.join(",");
    }
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
  if (path.startsWith("http://") || path.startsWith("https://")) {
    window.open(path, "_blank", "noopener,noreferrer");
    return;
  }
  if (!isTauri()) return;
  const { isCoLocatedWorkshop } = await import("$lib/utils/workshopLocality");
  if (!isCoLocatedWorkshop()) return;
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(path);
}

export async function attachmentPreviewUrl(path: string): Promise<string | null> {
  const { localFilePreviewUrl } = await import("$lib/utils/vaultFilesystem");
  return localFilePreviewUrl(path);
}
