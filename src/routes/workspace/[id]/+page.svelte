<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { workspaces, activeWorkspaceId } from "$lib/stores/workspace";
  import OverviewTab from "$lib/components/OverviewTab.svelte";
  import ModsTab from "$lib/components/ModsTab.svelte";
  import ConfigTab from "$lib/components/ConfigTab.svelte";
  import type { Workspace, PackConfig } from "$lib/types";

  let { params } = $props();
  let ws = $state<any>(null);
  let fullCfg = $state<PackConfig | null>(null);
  let activeTab = $state<string>("overview");
  let dlStage = $state<string>("");
  let dlPct = $state<number>(0);
  let downloading = $state(false);
  let gameLogs = $state<string[]>([]);

  onMount(() => {
    const id = params.id;
    activeWorkspaceId.set(id);
    const unsub = workspaces.subscribe(list => { ws = list.find(w => w.id === id) ?? null; });

    // Load full config
    invoke("get_pack_config", { id }).then((cfg: any) => { fullCfg = cfg; }).catch(() => {});

    // Listen for download progress
    const unlisten = listen<any>("download-progress", (e) => {
      dlStage = e.payload.stage || "";
      dlPct = e.payload.progress || 0;
      if (dlPct >= 100) { setTimeout(() => { downloading = false; }, 1000); }
    });
    const unlistenGame = listen<any>("game-status", (e) => {
      const state = e.payload?.state;
      const message = e.payload?.message;
      if (state === "log" && message) {
        gameLogs = [String(message), ...gameLogs].slice(0, 40);
      } else if (state === "stderr" && message) {
        gameLogs = [`[stderr] ${String(message)}`, ...gameLogs].slice(0, 40);
      } else if (state === "stopped") {
        gameLogs = ["[info] 游戏进程已结束", ...gameLogs].slice(0, 40);
      }
    });
    return () => { unsub(); unlisten.then(f => f()); unlistenGame.then(f => f()); };
  });

  async function handleDownloadMc() {
    if (!ws || downloading) return;
    downloading = true;
    dlStage = "准备下载...";
    dlPct = 0;
    try {
      await invoke("download_mc_version", { workspaceId: ws.id, mcVersion: ws.mc_version });
    } catch (e: any) {
      dlStage = "下载失败: " + e;
    }
  }

  function goBack() { activeWorkspaceId.set(null); goto("/"); }
</script>

<div class="p-4 lg:p-6">
  <div class="flex items-center gap-3 mb-4">
    <button class="btn btn-ghost btn-sm btn-circle" onclick={goBack} aria-label="返回">
      <svg xmlns="http://www.w3.org/2000/svg" class="h-5 w-5" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M19 12H5M12 19l-7-7 7-7"/></svg>
    </button>
    <div>
      <h1 class="text-2xl font-bold">{ws?.name || "工作区"}</h1>
      <p class="text-sm text-base-content/60">{ws?.description || ""}</p>
    </div>
  </div>

  {#if downloading}
    <div class="alert alert-info mb-4">
      <span class="loading loading-spinner loading-xs"></span>
      <span>{dlStage} ({dlPct}%)</span>
      <progress class="progress progress-primary w-1/3" value={dlPct} max="100"></progress>
    </div>
  {/if}

  {#if gameLogs.length > 0}
    <div class="card bg-base-200 border border-base-300 mb-4">
      <div class="card-body py-3">
        <h3 class="font-semibold">启动日志</h3>
        <pre class="text-xs whitespace-pre-wrap max-h-48 overflow-auto">{gameLogs.join("\n")}</pre>
      </div>
    </div>
  {/if}

  {#if ws && fullCfg}
    <div role="tablist" class="tabs tabs-bordered mb-6">
      <button role="tab" class="tab tab-lg {activeTab === 'overview' ? 'tab-active' : ''}" onclick={() => activeTab = 'overview'}>概览</button>
      <button role="tab" class="tab tab-lg {activeTab === 'mods' ? 'tab-active' : ''}" onclick={() => activeTab = 'mods'}>模组</button>
      <button role="tab" class="tab tab-lg {activeTab === 'config' ? 'tab-active' : ''}" onclick={() => activeTab = 'config'}>配置</button>
    </div>
    {#if activeTab === 'overview'}
      <OverviewTab workspace={ws} fullConfig={fullCfg} ondownloadmc={handleDownloadMc} downloading={downloading} />
    {:else if activeTab === 'mods'}
      <ModsTab workspace={ws} />
    {:else if activeTab === 'config'}
      <ConfigTab workspace={ws} />
    {/if}
  {:else if ws && !fullCfg}
    <div class="flex justify-center py-12"><span class="loading loading-spinner loading-md"></span><span class="ml-2">加载配置中...</span></div>
  {:else}
    <div class="alert alert-warning">工作区未找到</div>
  {/if}
</div>
