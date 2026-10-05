import { invoke } from "@tauri-apps/api/core";
export interface SshTarget {
  target_id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  agent_access: boolean;
}
export interface SshConfig {
  name: string;
  host: string;
  port: number;
  username: string;
  identity_file: string | null;
  agent_access: boolean;
}
export interface HostInspection { host_keys: string[]; fingerprints: string[] }
export async function sshTargets(): Promise<SshTarget[]> {
  return (await invoke<{ targets: SshTarget[] }>("ssh_targets")).targets;
}
export function sshAction<T>(action: "inspect" | "save" | "test" | "terminal", input: unknown): Promise<T> {
  return invoke<T>("ssh_action", { action, input });
}
export function sshSetAccess(targetId: string, enabled: boolean): Promise<void> {
  return invoke("ssh_set_access", { targetId, enabled });
}
export function sshRemove(targetId: string): Promise<void> {
  return invoke("ssh_remove", { targetId });
}
