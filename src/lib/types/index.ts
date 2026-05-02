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

export interface WorkspaceMod {
  project_id: string;
  version_id: string;
  mod_name: string;
  mod_version: string;
  mc_version: string;
  file_name: string;
  title?: string;
}

export interface ModrinthVersionSummary {
  version_id: string;
  version_number: string;
  game_versions: string[];
  loaders: string[];
  file_name: string;
  download_url: string;
  size: number;
  sha1?: string | null;
}

export interface ModrinthSearchResult {
  project_id: string;
  slug: string;
  title: string;
  description: string;
  downloads: number;
  icon_url?: string | null;
  latest_version?: ModrinthVersionSummary | null;
}

export interface PackConfig {
  id?: string;
  name: string;
  description: string;
  mc_version: McVersion;
  loader_type?: string;
  loader_version?: string | null;
  mods: WorkspaceMod[];
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
  | { state: "launching"; stage?: string; current?: number; total?: number }
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

export interface AppSettings {
  download_pool_size: number;
  theme: "dark" | "cupcake";
}

export interface LoaderVersionOption {
  version: string;
  stable: boolean;
}
