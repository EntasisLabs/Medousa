import { narrationChunks, narrationTextFromMarkdown } from "$lib/utils/narrationText";

const AUTO_NARRATE_KEY = "medousa-home-auto-narrate";
const MAX_REMEMBERED_TURNS = 100;

function loadAutoNarrate(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(AUTO_NARRATE_KEY) === "1";
}

export class NarrationStore {
  autoNarrate = $state(loadAutoNarrate());
  available = $state(false);
  activeMessageId = $state<string | null>(null);
  error = $state<string | null>(null);

  private initialized = false;
  private generation = 0;
  private queue: string[] = [];
  private narratedTurnIds = new Set<string>();

  initialize() {
    if (this.initialized) return;
    this.initialized = true;
    this.available =
      typeof window !== "undefined" &&
      "speechSynthesis" in window &&
      typeof SpeechSynthesisUtterance !== "undefined";
  }

  setAutoNarrate(enabled: boolean) {
    this.initialize();
    this.autoNarrate = enabled && this.available;
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(AUTO_NARRATE_KEY, this.autoNarrate ? "1" : "0");
    }
    if (!this.autoNarrate) this.stop();
  }

  toggleAutoNarrate() {
    this.setAutoNarrate(!this.autoNarrate);
  }

  toggleMessage(messageId: string, markdown: string) {
    if (this.activeMessageId === messageId) {
      this.stop();
      return;
    }
    this.speak(messageId, markdown);
  }

  maybeAutoNarrate(turnId: string, messageId: string, markdown: string) {
    if (!this.autoNarrate || this.narratedTurnIds.has(turnId)) return;
    if (typeof document !== "undefined" && document.visibilityState === "hidden") return;
    if (this.speak(messageId, markdown)) {
      this.narratedTurnIds.add(turnId);
      while (this.narratedTurnIds.size > MAX_REMEMBERED_TURNS) {
        const oldest = this.narratedTurnIds.values().next().value;
        if (typeof oldest !== "string") break;
        this.narratedTurnIds.delete(oldest);
      }
    }
  }

  speak(messageId: string, markdown: string): boolean {
    this.initialize();
    const speech = narrationTextFromMarkdown(markdown);
    if (!this.available || !speech) {
      this.error = this.available ? null : "Narration is unavailable on this device.";
      return false;
    }

    this.stop();
    this.error = null;
    this.activeMessageId = messageId;
    this.queue = narrationChunks(speech);
    const generation = this.generation;
    this.playNext(generation);
    return true;
  }

  stop() {
    this.generation += 1;
    this.queue = [];
    this.activeMessageId = null;
    if (typeof window !== "undefined" && "speechSynthesis" in window) {
      window.speechSynthesis.cancel();
    }
  }

  private playNext(generation: number) {
    if (generation !== this.generation) return;
    const chunk = this.queue.shift();
    if (!chunk) {
      this.activeMessageId = null;
      return;
    }

    const utterance = new SpeechSynthesisUtterance(chunk);
    utterance.lang =
      (typeof document !== "undefined" && document.documentElement.lang) ||
      (typeof navigator !== "undefined" ? navigator.language : "en-US") ||
      "en-US";
    utterance.rate = 1;
    utterance.onend = () => this.playNext(generation);
    utterance.onerror = (event) => {
      if (generation !== this.generation) return;
      if (event.error !== "canceled" && event.error !== "interrupted") {
        this.error = "Narration stopped unexpectedly.";
      }
      this.queue = [];
      this.activeMessageId = null;
    };
    window.speechSynthesis.speak(utterance);
  }
}

export const narration = new NarrationStore();
