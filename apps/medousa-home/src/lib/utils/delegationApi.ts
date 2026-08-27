import { invoke } from "@tauri-apps/api/core";

export interface DelegationTarget {
  routeRef: string;
  peerDeviceId: string;
  label?: string | null;
}

export interface DelegationBinding {
  target: DelegationTarget;
  createdAt: string;
  updatedAt: string;
}

export function loadDelegationBinding(): Promise<DelegationBinding | null> {
  return invoke<DelegationBinding | null>("embedded_delegation_binding");
}

export function setDelegationBinding(workshopId: string): Promise<DelegationBinding> {
  return invoke<DelegationBinding>("embedded_set_delegation_binding", { workshopId });
}

export function clearDelegationBinding(): Promise<boolean> {
  return invoke<boolean>("embedded_clear_delegation_binding");
}
