/** Browser speech-to-text for chat composer (P5d v1 — Web Speech API). */

type SpeechRecognitionCtor = new () => SpeechRecognition;

function speechRecognitionCtor(): SpeechRecognitionCtor | null {
  if (typeof window === "undefined") return null;
  const w = window as Window & {
    SpeechRecognition?: SpeechRecognitionCtor;
    webkitSpeechRecognition?: SpeechRecognitionCtor;
  };
  return w.SpeechRecognition ?? w.webkitSpeechRecognition ?? null;
}

function isIosShell(): boolean {
  return typeof navigator !== "undefined" && /iPhone|iPad|iPod/i.test(navigator.userAgent);
}

function mapSpeechError(error: string): string {
  switch (error) {
    case "not-allowed":
    case "service-not-allowed":
      return "Microphone or speech permission denied — allow access in Settings and try again.";
    case "audio-capture":
      return "Could not access the microphone.";
    case "network":
      return "Speech recognition needs a network connection on this device.";
    case "no-speech":
      return "No speech detected — try again closer to the mic.";
    default:
      return error;
  }
}

export function composerSpeechSupported(): boolean {
  return speechRecognitionCtor() !== null;
}

export interface ComposerSpeechSession {
  stop(): void;
  abort(): void;
}

async function ensureMicrophoneAccess(): Promise<{ ok: true } | { ok: false; error: string }> {
  if (!navigator.mediaDevices?.getUserMedia) {
    return { ok: true };
  }

  try {
    const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
    for (const track of stream.getTracks()) {
      track.stop();
    }
    return { ok: true };
  } catch (err) {
    const name = err instanceof DOMException ? err.name : "Error";
    if (name === "NotAllowedError" || name === "PermissionDeniedError") {
      return {
        ok: false,
        error: mapSpeechError("not-allowed"),
      };
    }
    return {
      ok: false,
      error: err instanceof Error ? err.message : String(err),
    };
  }
}

export async function startComposerSpeech(handlers: {
  onFinal: (text: string) => void;
  onEnd?: () => void;
  onError?: (message: string) => void;
}): Promise<ComposerSpeechSession | null> {
  const Ctor = speechRecognitionCtor();
  if (!Ctor) return null;

  const mic = await ensureMicrophoneAccess();
  if (!mic.ok) {
    handlers.onError?.(mic.error);
    return null;
  }

  const recognition = new Ctor();
  recognition.lang = navigator.language || "en-US";
  recognition.continuous = !isIosShell();
  recognition.interimResults = isIosShell();

  const finals: string[] = [];

  recognition.onresult = (event: SpeechRecognitionEvent) => {
    for (let index = event.resultIndex; index < event.results.length; index += 1) {
      const result = event.results[index];
      const transcript = result[0]?.transcript ?? "";
      if (!transcript.trim()) continue;
      if (result.isFinal || isIosShell()) {
        finals.push(transcript);
      }
    }
  };

  recognition.onerror = (event: SpeechRecognitionErrorEvent) => {
    if (event.error === "aborted") return;
    if (event.error === "no-speech") {
      handlers.onError?.(mapSpeechError("no-speech"));
      return;
    }
    handlers.onError?.(mapSpeechError(event.error));
  };

  recognition.onend = () => {
    const text = finals.join(" ").replace(/\s+/g, " ").trim();
    if (text) handlers.onFinal(text);
    handlers.onEnd?.();
  };

  try {
    recognition.start();
  } catch (err) {
    handlers.onError?.(err instanceof Error ? err.message : String(err));
    return null;
  }

  return {
    stop: () => recognition.stop(),
    abort: () => recognition.abort(),
  };
}

export function appendComposerDraft(existing: string, spoken: string): string {
  const next = spoken.trim();
  if (!next) return existing;
  const base = existing.trimEnd();
  if (!base) return next;
  return `${base} ${next}`;
}
