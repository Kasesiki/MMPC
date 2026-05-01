<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { launchStatus } from "$lib/stores/workspace";

let {
  workspace,
  ondownloadmc,
  downloading = false,
  onconfigjava,
  javaLabel = "默认"
} = $props<any>();

let status = $state<any>({ state: 'idle' });
$effect(() => { status = $launchStatus; });

async function handleLaunch() {
  launchStatus.set({ state: 'launching' });
  try {
    const pid: number = await invoke('launch_game', {
      workspaceId: workspace.id,
      playerName: 'Player',
      javaPath: null,
    });
    launchStatus.set({ state: 'running', pid });
  } catch (e: any) {
    launchStatus.set({ state: 'error', message: e });
  }
}
</script>

<div class="flex flex-col gap-6">
  <div class="card bg-base-200 border border-base-300">
    <div class="card-body items-center text-center py-10">
      {#if status.state === 'running'}
        <button class="btn btn-circle btn-lg btn-error" onclick={() => launchStatus.set({ state: 'idle' })} aria-label="停止">
          <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8" viewBox="0 0 24 24" fill="currentColor"><rect x="6" y="6" width="12" height="12"/></svg>
        </button>
        <p class="text-lg font-medium mt-2">运行中 (PID: {status.pid})</p>
      {:else if status.state === 'launching'}
        <span class="loading loading-spinner loading-lg text-primary"></span>
        <p class="text-lg font-medium mt-2">启动中...</p>
      {:else}
        <button class="btn btn-circle btn-lg btn-success" onclick={handleLaunch} aria-label="启动" disabled={downloading}>
          <svg xmlns="http://www.w3.org/2000/svg" class="h-8 w-8 ml-1" viewBox="0 0 24 24" fill="currentColor"><polygon points="5,3 19,12 5,21"/></svg>
        </button>
        <p class="text-lg font-medium mt-2">启动游戏</p>
      {/if}
      {#if status.state === 'error'}
        <div class="alert alert-error mt-4 max-w-md"><span>{status.message}</span></div>
      {/if}
      <div class="mt-4">
        <button class="btn btn-outline btn-sm" onclick={ondownloadmc} disabled={downloading}>
          {downloading ? "下载中..." : "下载/修复 MC 依赖"}
        </button>
      </div>
      <div class="mt-2">
        <button class="btn btn-outline btn-sm" onclick={onconfigjava}>
          配置 Java（当前：{javaLabel}）
        </button>
      </div>
    </div>
  </div>

  <div class="grid grid-cols-2 lg:grid-cols-4 gap-4">
    <div class="stat bg-base-200 border border-base-300 rounded-box p-4">
      <div class="stat-title">MC 版本</div>
      <div class="stat-value text-lg">{workspace.mc_version}</div>
    </div>
    <div class="stat bg-base-200 border border-base-300 rounded-box p-4">
      <div class="stat-title">模组</div>
      <div class="stat-value text-lg">{workspace.mod_count}</div>
    </div>
    <div class="stat bg-base-200 border border-base-300 rounded-box p-4">
      <div class="stat-title">内存</div>
      <div class="stat-value text-lg">1024M / 4096M</div>
    </div>
    <div class="stat bg-base-200 border border-base-300 rounded-box p-4">
      <div class="stat-title">JVM 参数</div>
      <div class="stat-value text-lg">默认</div>
    </div>
  </div>
</div>
