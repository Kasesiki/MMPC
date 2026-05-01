import { writable, derived } from "svelte/store";
import { invoke } from "@tauri-apps/api/core";
import type { Workspace, LaunchStatus } from "$lib/types";

export const workspaces = writable<Workspace[]>([]);
export const activeWorkspaceId = writable<string | null>(null);
export const activeWorkspace = derived(
  [workspaces, activeWorkspaceId],
  ([$workspaces, $id]) => $workspaces.find((w) => w.id === $id) ?? null,
);
export const launchStatus = writable<LaunchStatus>({ state: "idle" });

export async function loadWorkspaces(): Promise<void> {
  try {
    const list: Workspace[] = await invoke("list_workspaces");
    workspaces.set(list);
  } catch (e) {
    console.error("加载工作区失败", e);
  }
}

export async function createWorkspaceOnDisk(
  name: string,
  mcVersion: string,
  description: string,
): Promise<Workspace | null> {
  try {
    const ws: Workspace = await invoke("create_workspace", {
      name,
      mcVersion,
      description,
    });
    workspaces.update((list) => [ws, ...list]);
    return ws;
  } catch (e) {
    console.error("创建工作区失败", e);
    return null;
  }
}

export async function deleteWorkspaceOnDisk(id: string): Promise<void> {
  try {
    await invoke("delete_workspace", { id });
    workspaces.update((list) => list.filter((w) => w.id !== id));
  } catch (e) {
    console.error("删除工作区失败", e);
  }
}

export async function savePackConfig(
  id: string,
  config: Record<string, unknown>,
): Promise<void> {
  try {
    await invoke("save_pack_config", { id, config });
  } catch (e) {
    console.error("保存配置失败", e);
  }
}
