<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { JavaRuntime } from "$lib/types";

  let list = $state<JavaRuntime[]>([]);
  let loading = $state(true);
  let adding = $state(false);
  let deletingId = $state<string | null>(null);

  let showAdd = $state(false);
  let formName = $state("");
  let formPath = $state("");
  let formVersion = $state("");
  let formMajor = $state<number | null>(null);
  let detecting = $state(false);
  let formErr = $state("");

  async function loadList() {
    loading = true;
    try {
      list = await invoke<JavaRuntime[]>("list_java_runtimes");
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadList();
  });

  async function handleDetect() {
    formErr = "";
    if (!formPath.trim()) return;
    detecting = true;
    try {
      const r = await invoke<{ version_text: string; major_version?: number }>("detect_java_runtime", {
        path: formPath.trim()
      });
      formVersion = r.version_text;
      formMajor = r.major_version ?? null;
      if (!formName.trim()) {
        formName = `Java ${formMajor ?? "Unknown"}`;
      }
    } catch (e: any) {
      formErr = String(e);
    } finally {
      detecting = false;
    }
  }

  async function handleAdd() {
    formErr = "";
    if (!formName.trim() || !formPath.trim() || adding) return;
    adding = true;
    try {
      await invoke("add_java_runtime", { name: formName.trim(), path: formPath.trim() });
      showAdd = false;
      formName = "";
      formPath = "";
      formVersion = "";
      formMajor = null;
      await loadList();
    } catch (e: any) {
      formErr = String(e);
    } finally {
      adding = false;
    }
  }

  async function handleDelete(id: string) {
    if (!confirm("确认删除该 Java 配置？")) return;
    deletingId = id;
    try {
      await invoke("delete_java_runtime", { id });
      await loadList();
    } finally {
      deletingId = null;
    }
  }

  function fmt(iso: string) {
    return new Date(iso).toLocaleDateString("zh-CN", {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit"
    });
  }
</script>

<div class="min-h-screen p-6">
  <div class="flex items-center justify-between mb-8">
    <div>
      <h1 class="text-3xl font-bold">Java 管理</h1>
      <p class="text-base-content/60 text-sm mt-1">管理可用于启动工作区的 Java 运行时</p>
    </div>
    <button class="btn btn-primary" onclick={() => (showAdd = true)}>+ 添加 Java</button>
  </div>

  {#if loading}
    <div class="flex justify-center py-24"><span class="loading loading-spinner loading-lg"></span></div>
  {:else if list.length === 0}
    <div class="flex flex-col items-center py-24 text-base-content/40">
      <p class="text-lg">还没有 Java 配置</p>
      <p class="text-sm mt-1">点击上方按钮添加 Java 路径</p>
    </div>
  {:else}
    <div class="overflow-x-auto">
      <table class="table table-zebra">
        <thead>
          <tr>
            <th>名称</th>
            <th>版本</th>
            <th class="max-md:hidden">路径</th>
            <th>添加时间</th>
            <th></th>
          </tr>
        </thead>
        <tbody>
          {#each list as j}
            <tr>
              <td class="font-medium">{j.name}</td>
              <td>
                <span class="badge badge-outline">
                  {j.major_version ? `Java ${j.major_version}` : "未知版本"}
                </span>
                <div class="text-xs text-base-content/60 mt-1">{j.version_text}</div>
              </td>
              <td class="text-sm text-base-content/60 max-md:hidden max-w-lg truncate">{j.path}</td>
              <td class="text-sm text-base-content/50">{fmt(j.created_at)}</td>
              <td>
                <button
                  class="btn btn-ghost btn-xs text-error"
                  disabled={deletingId === j.id}
                  onclick={() => handleDelete(j.id)}
                >
                  {deletingId === j.id ? "删除中..." : "删除"}
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

{#if showAdd}
  <div class="modal modal-open">
    <div class="modal-box">
      <h3 class="font-bold text-lg mb-4">添加 Java 运行时</h3>
      <div class="form-control gap-4">
        <div>
          <label class="label" for="j-path"><span class="label-text">Java 可执行文件路径</span></label>
          <div class="join w-full">
            <input id="j-path" class="input input-bordered join-item w-full" bind:value={formPath} placeholder="/path/to/java 或 java.exe" />
            <button class="btn join-item" onclick={handleDetect} disabled={detecting || !formPath.trim()}>
              {detecting ? "检测中..." : "检测"}
            </button>
          </div>
        </div>
        <div>
          <label class="label" for="j-name"><span class="label-text">显示名称</span></label>
          <input id="j-name" class="input input-bordered w-full" bind:value={formName} placeholder="例如：Temurin 21" />
        </div>
        {#if formVersion}
          <div class="alert alert-info text-sm">
            <span>检测结果：{formVersion}{formMajor ? `（Java ${formMajor}）` : ""}</span>
          </div>
        {/if}
        {#if formErr}
          <div class="alert alert-error text-sm"><span>{formErr}</span></div>
        {/if}
      </div>
      <div class="modal-action">
        <button class="btn btn-ghost" onclick={() => (showAdd = false)}>取消</button>
        <button class="btn btn-primary" onclick={handleAdd} disabled={adding || !formName.trim() || !formPath.trim()}>
          {adding ? "保存中..." : "保存"}
        </button>
      </div>
    </div>
    <div class="modal-backdrop" role="button" tabindex="0" onclick={() => (showAdd = false)} onkeydown={(e) => e.key === "Enter" && (showAdd = false)}></div>
  </div>
{/if}
