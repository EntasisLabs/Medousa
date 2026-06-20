/** Phase G3d — user-authored note templates (localStorage). */

const STORAGE_KEY = "medousa-vault-user-templates";
const MAX_TEMPLATES = 24;

export interface VaultUserTemplate {
  id: string;
  name: string;
  content: string;
  spaceId?: string;
  createdAt: string;
}

function readRaw(): VaultUserTemplate[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as VaultUserTemplate[];
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (row) =>
        row &&
        typeof row.id === "string" &&
        typeof row.name === "string" &&
        typeof row.content === "string",
    );
  } catch {
    return [];
  }
}

function writeRaw(templates: VaultUserTemplate[]) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(templates.slice(0, MAX_TEMPLATES)));
}

export function listVaultUserTemplates(): VaultUserTemplate[] {
  return readRaw().sort((left, right) => right.createdAt.localeCompare(left.createdAt));
}

export function saveVaultUserTemplate(input: {
  name: string;
  content: string;
  spaceId?: string;
}): VaultUserTemplate | null {
  const name = input.name.trim();
  if (!name || !input.content.trim()) return null;
  const template: VaultUserTemplate = {
    id: `tpl-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`,
    name,
    content: input.content,
    spaceId: input.spaceId,
    createdAt: new Date().toISOString(),
  };
  writeRaw([template, ...readRaw()]);
  return template;
}

export function deleteVaultUserTemplate(id: string): boolean {
  const next = readRaw().filter((row) => row.id !== id);
  if (next.length === readRaw().length) return false;
  writeRaw(next);
  return true;
}

export function getVaultUserTemplate(id: string): VaultUserTemplate | null {
  return readRaw().find((row) => row.id === id) ?? null;
}
