const STORAGE_KEY = "medousa-active-agent";

function loadSelectedManuscriptId(): string | null {
  if (typeof sessionStorage === "undefined") return null;
  const raw = sessionStorage.getItem(STORAGE_KEY);
  if (!raw || raw.trim() === "") return null;
  return raw;
}

function persistSelectedManuscriptId(id: string | null) {
  if (typeof sessionStorage === "undefined") return;
  if (id === null) {
    sessionStorage.removeItem(STORAGE_KEY);
  } else {
    sessionStorage.setItem(STORAGE_KEY, id);
  }
}

/** Active chat agent — `null` means Default Medousa. */
export class ActiveAgentStore {
  selectedManuscriptId = $state<string | null>(loadSelectedManuscriptId());

  setActive(manuscriptId: string) {
    const id = manuscriptId.trim();
    if (!id) {
      this.clear();
      return;
    }
    this.selectedManuscriptId = id;
    persistSelectedManuscriptId(id);
  }

  clear() {
    this.selectedManuscriptId = null;
    persistSelectedManuscriptId(null);
  }
}

export const activeAgent = new ActiveAgentStore();
