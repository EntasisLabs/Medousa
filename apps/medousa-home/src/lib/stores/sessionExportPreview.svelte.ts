/** Spotlight / session export → shared VaultExportPreviewModal payload. */

class SessionExportPreviewStore {
  open = $state(false);
  title = $state("");
  content = $state("");

  show(title: string, content: string) {
    this.title = title;
    this.content = content;
    this.open = true;
  }

  close() {
    this.open = false;
  }
}

export const sessionExportPreview = new SessionExportPreviewStore();
