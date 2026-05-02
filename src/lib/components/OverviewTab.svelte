<script lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { launchStatus } from "$lib/stores/workspace";

let {
  workspace,
  downloading = false,
  onconfigjava,
  javaLabel = "默认"
} = $props<any>();

let status = $state<any>({ state: 'idle' });
$effect(() => { status = $launchStatus; });

async function handleLaunch() {
  launchStatus.set({ state: 'launching', stage: '准备启动', current: 0, total: 0 });
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

async function handleStop() {
  if (status.state !== 'running') return;
  try {
    await invoke('stop_game', { pid: status.pid });
    launchStatus.set({ state: 'idle' });
  } catch (e: any) {
    launchStatus.set({ state: 'error', message: String(e) });
  }
}

function launchProgressText() {
  if (status.state !== 'launching') return '';
  const stage = status.stage || '启动中';
  const current = Number(status.current ?? 0);
  const total = Number(status.total ?? 0);
  if (total > 0) {
    return `${stage} ${current}/${total}`;
  }
  return stage;
}
</script>

<div class="flex flex-col gap-3">
      {#if status.state === 'running'}
        <button class="btn btn-error" onclick={handleStop} aria-label="关闭游戏">
          关闭游戏
        </button>
        <p class="text-lg font-medium mt-2">运行中 (PID: {status.pid})</p>
      {:else if status.state === 'launching'}
        <button class="btn" disabled>
          <span class="loading loading-spinner"></span>
          {launchProgressText()}
        </button>
        <p class="text-sm text-base-content/70">{launchProgressText()}</p>
      {:else}
        <button class="btn btn-outline" onclick={handleLaunch} aria-label="启动" disabled={downloading}>
          启动游戏
        </button>
        <button class="btn btn-outline btn-sm" onclick={onconfigjava}>
          配置 Java（当前：{javaLabel}）
        </button>
      {/if}
      {#if status.state === 'error'}
        <div class="alert alert-error mt-4 max-w-md"><span>{status.message}</span></div>
      {/if}
      <div class="mt-4">

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
