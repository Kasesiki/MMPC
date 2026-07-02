<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { invoke } from "@tauri-apps/api/core";
  import type { JavaRuntime, PackConfig, Workspace } from "$lib/types";

  let list = $state<JavaRuntime[]>([]);
  let loading = $state(true);
  let adding = $state(false);
  let deletingId = $state<string | null>(null);
  let workspaceId = $state("");
  let workspaceName = $state("");
  let workspaceConfig = $state<PackConfig | null>(null);
  let workspaceLoading = $state(true);
  let workspaceSaving = $state(false);
  let workspaceErr = $state("");
  let workspaceSuccess = $state("");
  let selectedJavaId = $state("");

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

  async function loadWorkspaceTarget(id: string) {
    workspaceLoading = true;
    workspaceErr = "";
    workspaceSuccess = "";

    try {
      const [cfg, workspaces] = await Promise.all([
        invoke<PackConfig>("get_pack_config", { id }),
        invoke<Workspace[]>("list_workspaces"),
      ]);
      workspaceConfig = cfg;
      workspaceName = workspaces.find((item) => item.id === id)?.name ?? id;
      selectedJavaId = cfg.java_runtime_id ?? "";
    } catch (e: any) {
      workspaceErr = String(e);
      workspaceConfig = null;
      workspaceName = "";
      selectedJavaId = "";
    } finally {
      workspaceLoading = false;
    }
  }

  onMount(() => {
    workspaceId = page.url.searchParams.get("workspaceId")?.trim() ?? "";
    if (!workspaceId) {
      goto("/");
      return;
    }

    void loadList();
    void loadWorkspaceTarget(workspaceId);
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
      if (selectedJavaId === id) {
        selectedJavaId = "";
      }
      await loadList();
    } finally {
      deletingId = null;
    }
  }

  async function handleSaveWorkspaceJava() {
    if (!workspaceId || !workspaceConfig || workspaceSaving) return;
    workspaceSaving = true;
    workspaceErr = "";
    workspaceSuccess = "";
    try {
      const nextConfig: PackConfig = {
        ...workspaceConfig,
        java_runtime_id: selectedJavaId || null,
      };
      await invoke("save_pack_config", { id: workspaceId, config: nextConfig });
      workspaceConfig = nextConfig;
      workspaceSuccess = "工作区 Java 运行时已更新";
    } catch (e: any) {
      workspaceErr = String(e);
    } finally {
      workspaceSaving = false;
    }
  }

  function closePanel() {
    if (!workspaceId) {
      goto("/");
      return;
    }
    goto(`/workspace/${workspaceId}`);
  }

  function currentWorkspaceJavaLabel() {
    if (!selectedJavaId) return "默认（系统 Java）";
    const found = list.find((item) => item.id === selectedJavaId);
    if (!found) return "已删除的 Java 运行时";
    return found.major_version ? `${found.name} (Java ${found.major_version})` : `${found.name} (${found.version_text})`;
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

<div
  class="overlay"
  role="button"
  tabindex="0"
  aria-label="关闭 Java 管理"
  onclick={closePanel}
  onkeydown={(e) => (e.key === "Enter" || e.key === "Escape") && closePanel()}
>
  <div
    class="panel java-panel"
    role="dialog"
    aria-modal="true"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
    onkeydown={(e) => e.stopPropagation()}
  >
    <div class="panel__header">
      <div>
        <h2 class="panel__title">Java 管理</h2>
        <div class="panel__subtitle">
          {workspaceName ? `为 ${workspaceName} 选择启动 Java，并管理本地 Java 运行时` : "加载工作区 Java 配置中"}
        </div>
      </div>
      <button class="icon-button" onclick={closePanel} aria-label="关闭">×</button>
    </div>

    <div class="panel__body java-panel__body">
      <section class="java-panel__section">
        <div class="java-panel__section-header">
          <div>
            <div class="panel-heading">Workspace Java</div>
            <div class="inline-message">为当前工作区选择启动时使用的 Java 版本。</div>
          </div>
          <div class="java-panel__current">{currentWorkspaceJavaLabel()}</div>
        </div>

        {#if workspaceLoading}
          <div class="empty-state">正在加载工作区配置...</div>
        {:else}
          <div class="field">
            <label for="workspace-java-select">Java Runtime</label>
            <select id="workspace-java-select" bind:value={selectedJavaId} disabled={loading || list.length === 0}>
              <option value="">默认（系统 Java）</option>
              {#each list as j}
                <option value={j.id}>{j.name} - {j.major_version ? `Java ${j.major_version}` : j.version_text}</option>
              {/each}
            </select>
          </div>
        {/if}

        {#if workspaceErr}
          <div class="inline-message" style="color: var(--vscode-danger);">{workspaceErr}</div>
        {/if}
        {#if workspaceSuccess}
          <div class="inline-message" style="color: var(--vscode-success);">{workspaceSuccess}</div>
        {/if}
      </section>

      <section class="java-panel__section">
        <div class="java-panel__section-header">
          <div>
            <div class="panel-heading">Installed Runtimes</div>
            <div class="inline-message">添加、查看和删除当前可用的 Java 运行时。</div>
          </div>
          <button class="primary-button" onclick={() => (showAdd = true)}>添加 Java</button>
        </div>

        {#if loading}
          <div class="empty-state">正在加载 Java 列表...</div>
        {:else if list.length === 0}
          <div class="empty-state">还没有 Java 配置，先添加一个运行时吧。</div>
        {:else}
          <div class="java-runtime-list">
            {#each list as j}
              <div
                class:java-runtime-card={true}
                class:java-runtime-card--active={selectedJavaId === j.id}
                role="button"
                tabindex="0"
                onclick={() => (selectedJavaId = j.id)}
                onkeydown={(e) => (e.key === "Enter" || e.key === " ") && (selectedJavaId = j.id)}
              >
                <div class="java-runtime-card__main">
                  <div class="java-runtime-card__title">
                    <span>{j.name}</span>
                    <span class="java-runtime-card__badge">
                      {j.major_version ? `Java ${j.major_version}` : "未知版本"}
                    </span>
                  </div>
                  <div class="java-runtime-card__meta">{j.version_text}</div>
                  <div class="java-runtime-card__path">{j.path}</div>
                </div>
                <div class="java-runtime-card__actions">
                  <div class="java-runtime-card__date">{fmt(j.created_at)}</div>
                  <button
                    class="secondary-button"
                    type="button"
                    disabled={deletingId === j.id}
                    onclick={(e) => {
                      e.stopPropagation();
                      void handleDelete(j.id);
                    }}
                  >
                    {deletingId === j.id ? "删除中..." : "删除"}
                  </button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </section>
    </div>

    <div class="panel__footer">
      <button class="secondary-button" onclick={closePanel}>关闭</button>
      <button class="primary-button" onclick={handleSaveWorkspaceJava} disabled={workspaceSaving || workspaceLoading}>
        {workspaceSaving ? "保存中..." : "保存工作区设置"}
      </button>
    </div>
  </div>
</div>

{#if showAdd}
  <div class="overlay">
    <div class="panel panel--narrow" role="dialog" aria-modal="true">
      <div class="panel__header">
        <div>
          <h3 class="panel__title">添加 Java 运行时</h3>
          <div class="panel__subtitle">录入一个可执行的 Java 路径，用于当前工作区选择。</div>
        </div>
        <button class="icon-button" onclick={() => (showAdd = false)} aria-label="关闭">×</button>
      </div>

      <div class="panel__body">
        <div class="field-grid">
          <div class="field">
            <label for="j-path">Java 可执行文件路径</label>
            <div class="button-row">
              <input id="j-path" bind:value={formPath} placeholder="/path/to/java 或 java.exe" />
              <button class="secondary-button" onclick={handleDetect} disabled={detecting || !formPath.trim()}>
                {detecting ? "检测中..." : "检测"}
              </button>
            </div>
          </div>

          <div class="field">
            <label for="j-name">显示名称</label>
            <input id="j-name" bind:value={formName} placeholder="例如：Temurin 21" />
          </div>

          {#if formVersion}
            <div class="inline-message">检测结果：{formVersion}{formMajor ? `（Java ${formMajor}）` : ""}</div>
          {/if}
          {#if formErr}
            <div class="inline-message" style="color: var(--vscode-danger);">{formErr}</div>
          {/if}
        </div>
      </div>

      <div class="panel__footer">
        <button class="secondary-button" onclick={() => (showAdd = false)}>取消</button>
        <button class="primary-button" onclick={handleAdd} disabled={adding || !formName.trim() || !formPath.trim()}>
          {adding ? "保存中..." : "保存"}
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .java-panel {
    width: min(980px, 100%);
  }

  .java-panel__body {
    display: grid;
    gap: 18px;
  }

  .java-panel__section {
    display: grid;
    gap: 14px;
    padding: 16px;
    border: 1px solid var(--vscode-border);
    background: rgba(255, 255, 255, 0.02);
  }

  .java-panel__section-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  .java-panel__current {
    color: var(--vscode-text-muted);
    font-size: 13px;
    text-align: right;
    max-width: 280px;
  }

  .java-runtime-list {
    display: grid;
    gap: 10px;
    max-height: 360px;
    overflow: auto;
  }

  .java-runtime-card {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    width: 100%;
    padding: 14px 16px;
    border: 1px solid var(--vscode-border-strong);
    background: var(--vscode-bg-panel);
    color: var(--vscode-text);
    text-align: left;
    transition: border-color 140ms ease, background 140ms ease;
  }

  .java-runtime-card:hover {
    border-color: var(--vscode-accent);
    background: var(--vscode-bg-hover);
  }

  .java-runtime-card--active {
    border-color: var(--vscode-accent);
    background: var(--vscode-accent-soft);
  }

  .java-runtime-card__main {
    display: grid;
    gap: 6px;
    min-width: 0;
  }

  .java-runtime-card__title {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    font-weight: 600;
  }

  .java-runtime-card__badge {
    color: var(--vscode-warning);
    font-size: 12px;
  }

  .java-runtime-card__meta,
  .java-runtime-card__date {
    color: var(--vscode-text-muted);
    font-size: 12px;
  }

  .java-runtime-card__path {
    color: var(--vscode-text-muted);
    font-size: 12px;
    word-break: break-all;
  }

  .java-runtime-card__actions {
    display: grid;
    gap: 10px;
    justify-items: end;
    flex-shrink: 0;
  }

  @media (max-width: 720px) {
    .java-panel__section-header,
    .java-runtime-card {
      grid-template-columns: 1fr;
      display: grid;
    }

    .java-panel__current,
    .java-runtime-card__actions {
      text-align: left;
      justify-items: start;
      max-width: none;
    }
  }
</style>
