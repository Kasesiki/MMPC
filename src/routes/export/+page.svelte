<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import type { Workspace } from "$lib/types";

  type ExportKind = "client" | "server" | "full";
  type ExportProgress = {
    stage: string;
    current: number;
    total: number;
    message?: string | null;
  };

  function formatLoader(workspace: Workspace): string {
    const loaderType = String(workspace.loader_type || "vanilla").trim().toLowerCase();
    const loaderVersion = String(workspace.loader_version || "").trim();
    if (loaderType === "vanilla") {
      return "Vanilla";
    }
    if (!loaderVersion) {
      return loaderType;
    }
    return `${loaderType} ${loaderVersion}`;
  }

  let workspaces = $state<Workspace[]>([]);
  let selectedWorkspaceId = $state("");
  let exportKind = $state<ExportKind>("client");
  let includeJava = $state(false);
  let exporting = $state(false);
  let error = $state("");
  let success = $state("");
  let progress = $state<ExportProgress | null>(null);

  onMount(() => {
    let disposed = false;
    const offPromise = listen<ExportProgress>("export-progress", (event) => {
      if (disposed) return;
      progress = event.payload;
    });
    return () => {
      disposed = true;
      offPromise.then((off) => off()).catch(() => {});
    };
  });

  $effect(() => {
    invoke<Workspace[]>("list_workspaces")
      .then((list) => {
        workspaces = list;
        if (!selectedWorkspaceId && list.length > 0) {
          selectedWorkspaceId = list[0].id;
        }
      })
      .catch((e) => {
        error = String(e);
      });
  });

  async function runExport() {
    if (!selectedWorkspaceId || exporting) return;
    exporting = true;
    error = "";
    success = "";
    progress = {
      stage: "等待导出开始",
      current: 0,
      total: 1,
      message: null,
    };
    try {
      const result = await invoke<{ export_dir: string }>("export_workspace", {
        request: {
          workspace_id: selectedWorkspaceId,
          export_kind: exportKind,
          include_java: includeJava,
        },
      });
      success = `导出完成：${result.export_dir}`;
    } catch (e: any) {
      error = String(e);
    } finally {
      exporting = false;
      if (error) {
        progress = null;
      }
    }
  }
</script>

<div class="mx-auto flex max-w-3xl flex-col gap-6">
  <div class="flex items-center justify-between gap-3">
    <div>
      <h1 class="text-2xl font-bold">导出整合包</h1>
      <p class="text-sm text-base-content/60">按用途导出客户端、服务端或完整运行目录</p>
    </div>
    <button class="btn btn-ghost btn-sm" onclick={() => goto("/")}>返回</button>
  </div>

  {#if error}
    <div class="alert alert-error"><span>{error}</span></div>
  {/if}
  {#if success}
    <div class="alert alert-success"><span>{success}</span></div>
  {/if}
  {#if exporting && progress}
    <div class="rounded-3xl border border-primary/20 bg-base-100 p-5 shadow-sm">
      <div class="flex items-start justify-between gap-4">
        <div>
          <div class="text-sm font-semibold text-primary">导出进度</div>
          <div class="mt-1 text-lg font-bold">{progress.stage}</div>
          {#if progress.message}
            <div class="mt-1 text-sm text-base-content/70">{progress.message}</div>
          {/if}
        </div>
        <div class="text-right">
          <div class="text-2xl font-black">{progress.current}/{progress.total}</div>
          <div class="text-xs uppercase tracking-[0.2em] text-base-content/45">progress</div>
        </div>
      </div>
      <progress
        class="progress progress-primary mt-4 w-full"
        value={Math.min(progress.current, progress.total || 1)}
        max={progress.total || 1}
      ></progress>
    </div>
  {/if}

  <div class="card border border-base-300 bg-base-200">
    <div class="card-body gap-5">
      <div>
        <label class="label" for="export-workspace">
          <span class="label-text">工作区</span>
        </label>
        <select id="export-workspace" class="select select-bordered w-full" bind:value={selectedWorkspaceId}>
          {#each workspaces as ws}
            <option value={ws.id}>{ws.name} · MC {ws.mc_version} · {formatLoader(ws)}</option>
          {/each}
        </select>
      </div>

      <div>
        <div class="label">
          <span class="label-text">导出方式</span>
        </div>
        <div class="grid gap-3 md:grid-cols-3">
          <label class="card cursor-pointer border border-base-300 bg-base-100 p-4">
            <input type="radio" class="radio radio-primary mb-3" bind:group={exportKind} value="client" />
            <div class="font-semibold">客户端</div>
            <div class="text-sm text-base-content/65">MC 本体 + 仅客户端 / 双端可用模组</div>
          </label>
          <label class="card cursor-pointer border border-base-300 bg-base-100 p-4">
            <input type="radio" class="radio radio-primary mb-3" bind:group={exportKind} value="server" />
            <div class="font-semibold">服务端</div>
            <div class="text-sm text-base-content/65">MC 本体 + 仅服务端 / 双端可用模组</div>
          </label>
          <label class="card cursor-pointer border border-base-300 bg-base-100 p-4">
            <input type="radio" class="radio radio-primary mb-3" bind:group={exportKind} value="full" />
            <div class="font-semibold">全量</div>
            <div class="text-sm text-base-content/65">导出全部数据</div>
          </label>
        </div>
      </div>

      <label class="label cursor-pointer justify-start gap-3">
        <input type="checkbox" class="checkbox checkbox-primary" bind:checked={includeJava} />
        <span class="label-text">同时导出工作区使用的 Java</span>
      </label>

      <div class="flex justify-end">
        <button class="btn btn-primary" onclick={runExport} disabled={!selectedWorkspaceId || exporting}>
          {exporting ? "导出中..." : "开始导出"}
        </button>
      </div>
    </div>
  </div>
</div>
