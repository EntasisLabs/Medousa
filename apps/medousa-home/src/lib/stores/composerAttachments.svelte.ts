export type ComposerAttachmentHost = "ask" | "chat";

class HostAttachments {
  skillIds = $state<string[]>([]);
  toolIds = $state<string[]>([]);

  attachSkill(id: string) {
    const trimmed = id.trim();
    if (!trimmed || this.skillIds.includes(trimmed)) return;
    this.skillIds = [...this.skillIds, trimmed];
  }

  detachSkill(id: string) {
    this.skillIds = this.skillIds.filter((entry) => entry !== id);
  }

  attachTool(id: string) {
    const trimmed = id.trim();
    if (!trimmed || this.toolIds.includes(trimmed)) return;
    this.toolIds = [...this.toolIds, trimmed];
  }

  detachTool(id: string) {
    this.toolIds = this.toolIds.filter((entry) => entry !== id);
  }

  toggleSkill(id: string) {
    if (this.skillIds.includes(id)) this.detachSkill(id);
    else this.attachSkill(id);
  }

  toggleTool(id: string) {
    if (this.toolIds.includes(id)) this.detachTool(id);
    else this.attachTool(id);
  }

  clear() {
    this.skillIds = [];
    this.toolIds = [];
  }

  get primarySkillId(): string | null {
    return this.skillIds[0] ?? null;
  }
}

class ComposerAttachmentsStore {
  ask = new HostAttachments();
  chat = new HostAttachments();

  forHost(host: ComposerAttachmentHost): HostAttachments {
    return host === "ask" ? this.ask : this.chat;
  }
}

export const composerAttachments = new ComposerAttachmentsStore();
