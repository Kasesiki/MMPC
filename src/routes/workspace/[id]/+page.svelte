<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { listReleaseVersions, savePackConfig } from "$lib/stores/workspace";
  import { workspaces, activeWorkspaceId, launchStatus } from "$lib/stores/workspace";
  import OverviewTab from "$lib/components/OverviewTab.svelte";
  import ModsTab from "$lib/components/ModsTab.svelte";
  import ConfigTab from "$lib/components/ConfigTab.svelte";
  import type { Workspace, PackConfig, JavaRuntime } from "$lib/types";

  const fallbackReleaseVersions = ["1.21", "1.20.6", "1.20.4", "1.20.1", "1.19.4", "1.18.2", "1.16.5", "1.12.2"];

  let { params } = $props();
  let ws = $state<any>(null);
  let fullCfg = $state<PackConfig | null>(null);
  let activeTab = $state<string>("overview");
  let dlCurrent = $state<number>(0);
  let dlTotal = $state<number>(0);
  let downloading = $state(false);
  let gameLogs = $state<string[]>([]);
  let javaList = $state<JavaRuntime[]>([]);
  let showJavaModal = $state(false);
  let javaSaving = $state(false);
  let selectedJavaId = $state<string>("");
  let javaErr = $state("");
  let releaseVersions = $state<string[]>([...fallbackReleaseVersions]);
  let configSaving = $state(false);
  let configError = $state("");

  onMount(() => {
    const id = params.id;
    activeWorkspaceId.set(id);
    const unsub = workspaces.subscribe(list => { ws = list.find(w => w.id === id) ?? null; });

    // Load full config
    invoke("get_pack_config", { id }).then((cfg: any) => { fullCfg = cfg; }).catch(() => {});
    invoke<JavaRuntime[]>("list_java_runtimes").then((list) => { javaList = list; }).catch(() => {});
    listReleaseVersions().then((versions) => {
      if (versions.length > 0) {
        releaseVersions = versions;
      }
    }).catch(() => {});

    // Listen for download progress
    const unlisten = listen<any>("download-progress", (e) => {
      dlCurrent = Number(e.payload?.current ?? 0);
      dlTotal = Number(e.payload?.total ?? 0);
    });
    const unlistenGame = listen<any>("game-status", (e) => {
      const state = e.payload?.state;
      const message = e.payload?.message;
      if (state === "log" && message) {
        gameLogs = [String(message), ...gameLogs].slice(0, 40);
      } else if (state === "stderr" && message) {
        gameLogs = [`[stderr] ${String(message)}`, ...gameLogs].slice(0, 40);
      } else if (state === "stopped") {
        launchStatus.set({ state: "idle" });
        gameLogs = ["[info] 游戏进程已结束", ...gameLogs].slice(0, 40);
      }
    });
    return () => { unsub(); unlisten.then(f => f()); unlistenGame.then(f => f()); };
  });

  async function handleDownloadMc() {
    if (!ws || downloading) return;
    downloading = true;
    dlCurrent = 0;
    dlTotal = 0;
    try {
      await invoke("download_mc_version", { workspaceId: ws.id, mcVersion: ws.mc_version });
    } catch (e: any) {
      gameLogs = [`[error] 下载失败: ${String(e)}`, ...gameLogs].slice(0, 40);
    } finally {
      downloading = false;
    }
  }

  function currentJavaLabel() {
    if (!fullCfg?.java_runtime_id) return "默认";
    const found = javaList.find((j) => j.id === fullCfg?.java_runtime_id);
    if (!found) return "已删除";
    const major = found.major_version ? `Java ${found.major_version}` : found.version_text;
    return `${found.name} (${major})`;
  }

  function openJavaModal() {
    if (!fullCfg) return;
    selectedJavaId = fullCfg.java_runtime_id || "";
    javaErr = "";
    showJavaModal = true;
  }

  async function saveJavaSelection() {
    if (!ws || !fullCfg || javaSaving) return;
    javaSaving = true;
    javaErr = "";
    try {
      const nextCfg = {
        ...fullCfg,
        java_runtime_id: selectedJavaId || null
      };
      await invoke("save_pack_config", { id: ws.id, config: nextCfg });
      fullCfg = nextCfg as any;
      showJavaModal = false;
    } catch (e: any) {
      javaErr = String(e);
    } finally {
      javaSaving = false;
    }
  }

  async function saveWorkspaceConfig(nextCfg: PackConfig) {
    if (!ws || configSaving) return;
    configSaving = true;
    configError = "";
    try {
      await savePackConfig(ws.id, nextCfg as unknown as Record<string, unknown>);
      fullCfg = nextCfg;
      await invoke("get_pack_config", { id: ws.id }).then((cfg: any) => { fullCfg = cfg; });
      await invoke<Workspace[]>("list_workspaces").then((list) => {
        workspaces.set(list);
      });
    } catch (e: any) {
      configError = String(e);
    } finally {
      configSaving = false;
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
      <span>下载进度：{dlCurrent}/{dlTotal}</span>
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

  {#if configError}
    <div class="alert alert-error mb-4">
      <span>{configError}</span>
    </div>
  {/if}

  {#if ws && fullCfg}
    <div role="tablist" class="tabs tabs-bordered mb-6">
      <button role="tab" class="tab tab-lg {activeTab === 'overview' ? 'tab-active' : ''}" onclick={() => activeTab = 'overview'}>概览</button>
      <button role="tab" class="tab tab-lg {activeTab === 'mods' ? 'tab-active' : ''}" onclick={() => activeTab = 'mods'}>模组</button>
      <button role="tab" class="tab tab-lg {activeTab === 'config' ? 'tab-active' : ''}" onclick={() => activeTab = 'config'}>配置</button>
    </div>
    {#if activeTab === 'overview'}
      <OverviewTab
        workspace={ws}
        fullConfig={fullCfg}
        onconfigjava={openJavaModal}
        javaLabel={currentJavaLabel()}
        downloading={downloading}
      />
    {:else if activeTab === 'mods'}
      <ModsTab workspace={ws} />
    {:else if activeTab === 'config'}
      <ConfigTab
        workspace={ws}
        config={fullCfg}
        releaseVersions={releaseVersions}
        onsave={saveWorkspaceConfig}
      />
    {/if}
  {:else if ws && !fullCfg}
    <div class="flex justify-center py-12"><span class="loading loading-spinner loading-md"></span><span class="ml-2">加载配置中...</span></div>
  {:else}
    <div class="alert alert-warning">工作区未找到</div>
  {/if}
</div>

{#if showJavaModal}
  <div class="modal modal-open">
    <div class="modal-box">
      <h3 class="font-bold text-lg mb-4">配置工作区 Java</h3>
      <div class="form-control gap-3">
        <label class="label" for="java-select"><span class="label-text">选择启动时使用的 Java</span></label>
        <select id="java-select" class="select select-bordered w-full" bind:value={selectedJavaId}>
          <option value="">默认（系统 java）</option>
          {#each javaList as j}
            <option value={j.id}>
              {j.name} - {j.major_version ? `Java ${j.major_version}` : j.version_text}
            </option>
          {/each}
        </select>
        {#if javaList.length === 0}
          <div class="alert alert-warning text-sm mt-2">
            <span>还没有可用 Java，请先到“Java 管理”页面添加。</span>
          </div>
        {/if}
        {#if javaErr}
          <div class="alert alert-error text-sm"><span>{javaErr}</span></div>
        {/if}
      </div>
      <div class="modal-action">
        <button class="btn btn-ghost" onclick={() => showJavaModal = false}>取消</button>
        <button class="btn btn-primary" onclick={saveJavaSelection} disabled={javaSaving}>
          {javaSaving ? "保存中..." : "保存"}
        </button>
      </div>
    </div>
    <div class="modal-backdrop" role="button" tabindex="0" onclick={() => showJavaModal = false} onkeydown={(e) => e.key === 'Enter' && (showJavaModal = false)}></div>
  </div>
{/if}
