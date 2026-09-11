import { ensureNotificationPermission } from "$lib/notifications";

function idFor(seed: string): number {
  let hash = 0;
  for (let index = 0; index < seed.length; index += 1) hash = (hash * 31 + seed.charCodeAt(index)) | 0;
  return Math.abs(hash) || 1;
}

export async function scheduleTimerNotification(input: {
  seed: string;
  title: string;
  deadlineAt: string;
}): Promise<number | undefined> {
  if (!(await ensureNotificationPermission())) return undefined;
  const deadline = new Date(input.deadlineAt);
  if (!Number.isFinite(deadline.getTime()) || deadline.getTime() <= Date.now()) return undefined;
  const { Schedule, sendNotification } = await import("@tauri-apps/plugin-notification");
  const id = idFor(input.seed);
  sendNotification({
    id,
    title: "Medousa timer",
    body: `${input.title} is ready`,
    schedule: Schedule.at(deadline, false, true),
    extra: { kind: "liquid-timer", seed: input.seed },
  });
  return id;
}

export async function cancelTimerNotification(id?: number): Promise<void> {
  if (!id) return;
  try {
    const { cancel } = await import("@tauri-apps/plugin-notification");
    await cancel([id]);
  } catch {
    // Browser preview or unavailable native plugin: deadline reconciliation still completes.
  }
}
