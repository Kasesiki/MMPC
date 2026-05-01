import { i as derived, w as writable } from "./exports.js";
import "@tauri-apps/api/core";
const workspaces = writable([]);
const activeWorkspaceId = writable(null);
derived(
  [workspaces, activeWorkspaceId],
  ([$workspaces, $id]) => $workspaces.find((w) => w.id === $id) ?? null
);
