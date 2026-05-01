const fs = require("fs");

function writeFiles() {
  // OverviewTab
  fs.writeFileSync("src/lib/components/OverviewTab.svelte", `<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { launchStatus } from "$lib/stores/workspace";
  import type { Workspace, PackConfig, LaunchStatus } from "$lib/types";

  let { workspace, fullConfig, ondownloadmc, downloading }: {
    workspace: Workspace;
    fullConfig: PackConfig;
    ondownloadmc: () => void;
    downloading: boolean;
  } = $props();

  let status = $state<LaunchStatus>({ state: "idle" });
  $effect(() => { status = $launchStatus; });

  async function handleLaunch() {
    launchStatus.set({ state: "launching" });
    try {
      await invoke("launch_game", { workspaceId: workspace.id, playerName: "Player", javaPath: null });
    } catch (e: any) {
      launchStatus.set({ state: "error", message: String(e) });
    }
  }

  function handleStop() { launchStatus.set({ state: "idle" }); }
</script>

<div class="flex flex-col gap-6">
  <div class="card bg-base-200 border border-base-300">
    <div class="card-body items-center text-center py-10">
      {#if !fullConfig?.mc_version}
        <p class="text-base-content/50">配置加载中...</p>
      {:else}
        {#if status.state === "running"}
          <button class="btn btn-circle btn-lg btn-error" onclick={handleStop} aria-label="停止">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12"/></svg>
          </button>
          <p class="text-lg font-medium mt-2">游戏运行中 (PID: {status.pid})</p>
        {:else if status.state === "launching"}
          <span class="loading loading-spinner loading-lg text-primary"></span>
          <p class="text-lg font-medium mt-2">正在启动...</p>
        {:else if status.state === "error"}
          <p class="text-error">启动失败</p>
          <div class="alert alert-error mt-4 max-w-md"><span>{status.message}</span></div>
          <button class="btn btn-primary mt-4" onclick={handleLaunch}>重试</button>
        {:else}
          <button class="btn btn-circle btn-lg btn-success" onclick={handleLaunch} aria-label="启动">
            <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8 ml-1" viewBox="0 0 24 24" fill="currentColor"><polygon points="5,3 19,12 5,21"/></svg>
          </button>
          <p class="text-lg font-medium mt-2">启动游戏</p>
        {/if}
      {/if}
    </div>
  </div>

  <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
    <div class="stat bg-base-200 border border-base-300 rounded-box p-4">
      <div class="stat-title">MC 版本</div>
      <div class="stat-value text-lg">{fullConfig?.mc_version || "-"}</div>
    </div>
    <div class="stat bg-base-200 border border-base-300 rounded-box p-4">
      <div class="stat-title">模组</div>
      <div class="stat-value text-lg">{fullConfig?.mods?.length || 0}</div>
    </div>
    <div class="stat bg-base-200 border border-base-300 rounded-box p-4">
      <div class="stat-title">内存</div>
      <div class="stat-value text-lg">{fullConfig?.min_memory_mb || "?"}M / {fullConfig?.max_memory_mb || "?"}M</div>
    </div>
    <div class="stat bg-base-200 border border-base-300 rounded-box p-4">
      <div class="stat-title">MC 数据</div>
      <div class="stat-value text-lg">
        {#if fullConfig?.mc_version}
          <button class="btn btn-ghost btn-xs" onclick={ondownloadmc} disabled={downloading}>
            {downloading ? "下载中" : "下载"}
          </button>
        {:else}-{/if}
      </div>
    </div>
  </div>
</div>`);

  console.log("OverviewTab written");
}

writeFiles();
