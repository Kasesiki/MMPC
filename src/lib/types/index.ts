export type McVersion = string;

export interface Mod {
  id: string;
  name: string;
  version: string;
  mc_version: McVersion;
  description: string;
  url?: string;
  file_size?: number;
}

export interface PackConfig {
  id?: string;
  name: string;
  description: string;
  mc_version: McVersion;
  mods: string[];
  jvm_args: string[];
  java_runtime_id?: string | null;
  min_memory_mb: number;
  max_memory_mb: number;
  window_width: number;
  window_height: number;
  created_at?: string;
  last_opened?: string;
}

export interface Workspace {
  id: string;
  name: string;
  mc_version: string;
  description: string;
  mod_count: number;
  config: PackConfig;
  path: string;
  last_opened: string;
  created_at: string;
}

export type LaunchStatus =
  | { state: "idle" }
  | { state: "launching" }
  | { state: "running"; pid: number }
  | { state: "error"; message: string };

export interface JavaRuntime {
  id: string;
  name: string;
  path: string;
  version_text: string;
  major_version?: number | null;
  created_at: string;
}
