import { invoke } from "@tauri-apps/api/core";

export interface DaemonWorkerConnection {
  id: string;
  label: string;
  workshopDeviceId: string;
  daemonUrl: string;
  pairingId: string;
  daemonPublicKey: string;
  irohTicket?: string;
  connectedAt: string;
}

export async function pairWorkerDaemon(
  pairingUrl: string,
  options: { workerUrl?: string; label?: string } = {},
): Promise<DaemonWorkerConnection> {
  return invoke<DaemonWorkerConnection>("runtime_pair_worker", {
    pairingUrl,
    workerUrl: options.workerUrl || null,
    label: options.label || null,
  });
}

export async function listWorkerDaemons(): Promise<DaemonWorkerConnection[]> {
  return invoke<DaemonWorkerConnection[]>("runtime_list_workers");
}

export async function removeWorkerDaemon(workerId: string): Promise<void> {
  await invoke("runtime_remove_worker", { workerId });
}

export async function selectWorkerDaemon(runtimeId: string | null): Promise<void> {
  await invoke("runtime_select_worker", { runtimeId });
}
