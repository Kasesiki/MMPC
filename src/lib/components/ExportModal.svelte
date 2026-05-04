<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import type { Workspace } from "$lib/types";

  type ExportKind = "client" | "server" | "full";
  type ExportProgress = {
    stage: string;
    current: number;
    total: number;
    message?: string | null;
  };

  let {
    open = false,
    workspace = null,
    onclose = () => {},
  }: {
    open?: boolean;
    workspace?: Workspace | null;
    onclose?: () => void;
  } = $props();

  let exportKind = $state<ExportKind>("client");
  let includeJava = $state(false);
  let exporting = $state(false);
  let error = $state("");
  let success = $state("");
  let progress = $state<ExportProgress | null>(null);

  function formatLoader(ws: Workspace | null): string {
    if (!ws) return "vanilla";
    const loaderType = String(ws.loader_type || "vanilla").trim().toLowerCase();
    const loaderVersion = String(ws.loader_version || "").trim();
    if (loaderType === "vanilla") {
      return "Vanilla";
    }
    if (!loaderVersion) {
      return loaderType;
    }
    return `${loaderType} ${loaderVersion}`;
  }

  onMount(() => {
    let disposed = false;
    const offPromise = listen<ExportProgress>("export-progress", (event) => {
      if (disposed || !open) return;
      progress = event.payload;
    });
    return () => {
      disposed = true;
      offPromise.then((off) => off()).catch(() => {});
    };
  });

  $effect(() => {
    if (!open) {
      exporting = false;
      error = "";
      success = "";
      progress = null;
    }
  });

  async function runExport() {
    if (!workspace?.id || exporting) return;
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
          workspace_id: workspace.id,
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

{#if open}
  <div class="modal modal-open">
    <div class="modal-box max-w-3xl">
      <div class="mb-4 flex items-start justify-between gap-3">
        <div>
          <h3 class="text-xl font-bold">导出整合包</h3>
          <div class="mt-1 text-sm text-base-content/65">
            {#if workspace}
              {workspace.name} · MC {workspace.mc_version} · {formatLoader(workspace)}
            {:else}
              未绑定工作区
            {/if}
          </div>
        </div>
        <button
          class="btn btn-ghost btn-sm btn-square text-base-content/55 hover:text-base-content"
          aria-label="关闭"
          onclick={onclose}
        >
          <span class="text-lg leading-none">X</span>
        </button>
      </div>

      {#if error}
        <div class="alert alert-error mb-4"><span>{error}</span></div>
      {/if}
      {#if success}
        <div class="alert alert-success mb-4"><span>{success}</span></div>
      {/if}

      {#if exporting && progress}
        <div class="rounded-2xl border border-primary/20 bg-base-100 p-4 mb-4">
          <div class="flex items-start justify-between gap-3">
            <div>
              <div class="text-sm font-semibold text-primary">导出进度</div>
              <div class="mt-1 text-lg font-bold">{progress.stage}</div>
              {#if progress.message}
                <div class="mt-1 text-sm text-base-content/70">{progress.message}</div>
              {/if}
            </div>
            <div class="text-right">
              <div class="text-xl font-black">{progress.current}/{progress.total}</div>
            </div>
          </div>
          <progress
            class="progress progress-primary mt-3 w-full"
            value={Math.min(progress.current, progress.total || 1)}
            max={progress.total || 1}
          ></progress>
        </div>
      {/if}

      <div class="card border border-base-300 bg-base-200">
        <div class="card-body gap-5">
          <div>
            <div class="label">
              <span class="label-text">导出方式</span>
            </div>
            <div class="grid gap-3 md:grid-cols-3">
              <label class="card cursor-pointer border border-base-300 bg-base-100 p-4">
                <input type="radio" class="radio radio-primary mb-3" bind:group={exportKind} value="client" />
                <div class="font-semibold">客户端</div>
                <div class="text-sm text-base-content/65">自动打包为客户端包</div>
              </label>
              <label class="card cursor-pointer border border-base-300 bg-base-100 p-4">
                <input type="radio" class="radio radio-primary mb-3" bind:group={exportKind} value="server" />
                <div class="font-semibold">服务端</div>
                <div class="text-sm text-base-content/65">自动打包为服务端包</div>
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

          <div class="flex justify-end gap-2">
            <button class="btn btn-ghost" onclick={onclose}>关闭</button>
            <button class="btn btn-primary" onclick={runExport} disabled={!workspace?.id || exporting}>
              {exporting ? "导出中..." : "开始导出"}
            </button>
          </div>
        </div>
      </div>
    </div>
    <div class="modal-backdrop" role="button" tabindex="0" onclick={onclose} onkeydown={(e) => e.key === "Enter" && onclose()}></div>
  </div>
{/if}
