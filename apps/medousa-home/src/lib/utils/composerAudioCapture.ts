/** Single mic capture for composer voice input — MediaRecorder + level analyser. */

export interface ComposerAudioCaptureSession {
  /** Current mic level 0–1 for waveform UI. */
  getLevel(): number;
  stop(): Promise<{ blob: Blob; mimeType: string }>;
  abort(): void;
}

function mapGetUserMediaError(err: unknown): string {
  const name = err instanceof DOMException ? err.name : "";
  switch (name) {
    case "NotAllowedError":
    case "PermissionDeniedError":
      return "Microphone permission denied — allow access in Settings and try again.";
    case "NotFoundError":
    case "DevicesNotFoundError":
      return "No microphone found on this device.";
    case "NotReadableError":
      return "Microphone is in use by another app.";
    default:
      return err instanceof Error ? err.message : String(err);
  }
}

function pickRecorderMimeType(): string {
  if (typeof MediaRecorder === "undefined") return "";
  const candidates = [
    "audio/webm;codecs=opus",
    "audio/webm",
    "audio/mp4",
    "audio/ogg;codecs=opus",
  ];
  return candidates.find((type) => MediaRecorder.isTypeSupported(type)) ?? "";
}

export function composerMicSupported(): boolean {
  return (
    typeof navigator !== "undefined" &&
    !!navigator.mediaDevices?.getUserMedia &&
    typeof MediaRecorder !== "undefined"
  );
}

export async function startComposerAudioCapture(handlers?: {
  onError?: (message: string) => void;
}): Promise<ComposerAudioCaptureSession | null> {
  if (!composerMicSupported()) {
    handlers?.onError?.("Microphone capture is not available here.");
    return null;
  }

  let stream: MediaStream | null = null;
  let recorder: MediaRecorder | null = null;
  let audioContext: AudioContext | null = null;
  let analyser: AnalyserNode | null = null;
  let levelData: Uint8Array | null = null;
  let closed = false;
  const chunks: Blob[] = [];

  const teardownTracks = () => {
    for (const track of stream?.getTracks() ?? []) {
      track.stop();
    }
    stream = null;
  };

  const teardownAudio = () => {
    if (!audioContext) return;
    void audioContext.close().catch(() => {});
    audioContext = null;
    analyser = null;
    levelData = null;
  };

  try {
    stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  } catch (err) {
    handlers?.onError?.(mapGetUserMediaError(err));
    return null;
  }

  const mimeType = pickRecorderMimeType();
  try {
    recorder = mimeType
      ? new MediaRecorder(stream, { mimeType })
      : new MediaRecorder(stream);
  } catch (err) {
    teardownTracks();
    handlers?.onError?.(err instanceof Error ? err.message : String(err));
    return null;
  }

  const resolvedMime = recorder.mimeType || mimeType || "audio/webm";

  recorder.ondataavailable = (event) => {
    if (event.data.size > 0) chunks.push(event.data);
  };

  audioContext = new AudioContext();
  const source = audioContext.createMediaStreamSource(stream);
  analyser = audioContext.createAnalyser();
  analyser.fftSize = 256;
  source.connect(analyser);
  levelData = new Uint8Array(analyser.frequencyBinCount);

  recorder.start(250);

  return {
    getLevel() {
      if (closed || !analyser || !levelData) return 0;
      analyser.getByteFrequencyData(levelData);
      let sum = 0;
      for (let index = 0; index < levelData.length; index += 1) {
        sum += levelData[index] ?? 0;
      }
      return Math.min(1, sum / levelData.length / 255);
    },
    stop() {
      return new Promise((resolve, reject) => {
        if (closed) {
          reject(new Error("Recording already stopped."));
          return;
        }
        closed = true;

        if (!recorder || recorder.state === "inactive") {
          teardownTracks();
          teardownAudio();
          resolve({ blob: new Blob(chunks, { type: resolvedMime }), mimeType: resolvedMime });
          return;
        }

        recorder.onstop = () => {
          teardownTracks();
          teardownAudio();
          resolve({ blob: new Blob(chunks, { type: resolvedMime }), mimeType: resolvedMime });
        };
        recorder.onerror = () => {
          teardownTracks();
          teardownAudio();
          reject(new Error("Recording failed."));
        };

        try {
          recorder.stop();
        } catch (err) {
          teardownTracks();
          teardownAudio();
          reject(err instanceof Error ? err : new Error(String(err)));
        }
      });
    },
    abort() {
      if (closed) return;
      closed = true;
      try {
        recorder?.stop();
      } catch {
        // Already inactive.
      }
      teardownTracks();
      teardownAudio();
    },
  };
}
