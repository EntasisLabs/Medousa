import { isTauri } from "$lib/platform";
import { isMedousaMarkId, type MedousaMarkId } from "$lib/theme/medousaMarks";

export const MEDOUSA_MARK_STORAGE_KEY = "medousa-home-mark";
const MEDOUSA_MARK_EVENT = "settings://medousa-mark";

export function broadcastMedousaMark(mark: MedousaMarkId): void {
  if (!isTauri()) return;
  void import("@tauri-apps/api/event")
    .then(({ emit }) => emit(MEDOUSA_MARK_EVENT, mark))
    .catch(() => {
      // localStorage still persists the choice when the event bus is unavailable.
    });
}

/** Keep independent webviews on the same persisted companion identity. */
export async function listenForMedousaMark(
  apply: (mark: MedousaMarkId) => void,
): Promise<() => void> {
  const onStorage = (event: StorageEvent) => {
    if (event.key === MEDOUSA_MARK_STORAGE_KEY && isMedousaMarkId(event.newValue)) {
      apply(event.newValue);
    }
  };
  window.addEventListener("storage", onStorage);

  let unlisten = () => {};
  if (isTauri()) {
    try {
      const { listen } = await import("@tauri-apps/api/event");
      unlisten = await listen<string>(MEDOUSA_MARK_EVENT, (event) => {
        if (isMedousaMarkId(event.payload)) apply(event.payload);
      });
    } catch {
      // The storage listener remains as a browser-compatible fallback.
    }
  }

  return () => {
    window.removeEventListener("storage", onStorage);
    unlisten();
  };
}
