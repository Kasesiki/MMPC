<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import {
    createWorkspaceOnDisk,
    loadWorkspaces,
    listFabricLoaderVersions,
    listForgeLoaderVersions,
    listNeoForgeLoaderVersions,
    listReleaseVersions,
  } from "$lib/stores/workspace";
  import { workspaces } from "$lib/stores/workspace";
  import type { LoaderVersionOption, Workspace } from "$lib/types";

  const fallbackReleaseVersions = ["1.21", "1.20.6", "1.20.4", "1.20.1", "1.19.4", "1.18.2", "1.16.5", "1.12.2"];

  let wsList = $state<Workspace[]>([]);
  let loading = $state(true);
  let showWorkspacePicker = $state(false);
  let showCreatePanel = $state(false);
  let newName = $state("");
  let newMcVersion = $state("1.21");
  let newLoaderType = $state("vanilla");
  let newLoaderVersion = $state("");
  let newDesc = $state("");
  let creating = $state(false);
  let releaseVersions = $state<string[]>([...fallbackReleaseVersions]);
  let loaderVersions = $state<LoaderVersionOption[]>([]);
  let loadingLoaderVersions = $state(false);

  onMount(async () => {
    const versions = await listReleaseVersions();
    if (versions.length > 0) {
      releaseVersions = versions;
      if (!versions.includes(newMcVersion)) {
        newMcVersion = versions[0];
      }
    }
    await loadWorkspaces();
    loading = false;
  });

  $effect(() => {
    wsList = $workspaces;
  });

  $effect(() => {
    if (newLoaderType === "vanilla") {
      loaderVersions = [];
      newLoaderVersion = "";
      return;
    }

    loadingLoaderVersions = true;
    const loaderPromise =
      newLoaderType === "fabric"
        ? listFabricLoaderVersions(newMcVersion)
        : newLoaderType === "forge"
          ? listForgeLoaderVersions(newMcVersion)
          : listNeoForgeLoaderVersions(newMcVersion);

    loaderPromise
      .then((versions) => {
        loaderVersions = versions;
        const stable = versions.find((entry) => entry.stable);
        const fallback = versions[0];
        const nextVersion = stable?.version ?? fallback?.version ?? "";
        if (!versions.some((entry) => entry.version === newLoaderVersion)) {
          newLoaderVersion = nextVersion;
        }
      })
      .finally(() => {
        loadingLoaderVersions = false;
      });
  });

  async function handleCreate() {
    if (!newName.trim() || creating) return;
    creating = true;
    const ws = await createWorkspaceOnDisk(
      newName.trim(),
      newMcVersion,
      newDesc,
      newLoaderType,
      newLoaderType === "vanilla" ? null : newLoaderVersion.trim() || null,
    );
    creating = false;
    if (ws) goto(`/workspace/${ws.id}`);
  }

  function openWorkspace(id: string) {
    showWorkspacePicker = false;
    goto(`/workspace/${id}`);
  }

  function loaderVersionLabel() {
    if (newLoaderType === "forge") return "Forge 版本";
    if (newLoaderType === "neoforge") return "NeoForge 版本";
    if (newLoaderType === "fabric") return "Fabric 版本";
    return "Loader 版本";
  }
</script>

<div class="home-shell">
  <div class="home-stage">
    <div class="home-grid">
      <button class="entry-card" onclick={() => (showWorkspacePicker = true)}>
        <div>
          <h1 class="entry-card__title">Open Source</h1>
        </div>
      </button>

      <button class="entry-card" onclick={() => (showCreatePanel = true)}>
        <div>
          <h1 class="entry-card__title">Create Now</h1>
        </div>
      </button>
    </div>
  </div>
</div>

{#if showWorkspacePicker}
  <div
    class="overlay"
    role="button"
    tabindex="0"
    aria-label="关闭工作区选择"
    onclick={() => (showWorkspacePicker = false)}
    onkeydown={(e) => (e.key === "Enter" || e.key === "Escape") && (showWorkspacePicker = false)}
  >
    <div
      class="panel panel--narrow"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="panel__header">
        <div>
          <h2 class="panel__title">Open Source</h2>
          <div class="panel__subtitle">选择一个现有工作区</div>
        </div>
        <button class="icon-button" onclick={() => (showWorkspacePicker = false)} aria-label="关闭">×</button>
      </div>
      <div class="panel__body">
        {#if loading}
          <div class="empty-state">正在加载工作区...</div>
        {:else if wsList.length === 0}
          <div class="empty-state">还没有工作区</div>
        {:else}
          <div class="workspace-picker">
            {#each wsList as ws}
              <button class="workspace-option" onclick={() => openWorkspace(ws.id)}>
                <div class="workspace-option__title">
                  <span>{ws.name}</span>
                  <span>{ws.mc_version}</span>
                </div>
                <div class="workspace-option__meta">{ws.loader_type || "vanilla"} {ws.loader_version || ""}</div>
                <div class="workspace-option__desc">{ws.description || "无描述"}</div>
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}

{#if showCreatePanel}
  <div
    class="overlay"
    role="button"
    tabindex="0"
    aria-label="关闭新建工作区"
    onclick={() => (showCreatePanel = false)}
    onkeydown={(e) => (e.key === "Enter" || e.key === "Escape") && (showCreatePanel = false)}
  >
    <div
      class="panel"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="panel__header">
        <div>
          <h2 class="panel__title">Create Now</h2>
          <div class="panel__subtitle">新建一个工作区</div>
        </div>
        <button class="icon-button" onclick={() => (showCreatePanel = false)} aria-label="关闭">×</button>
      </div>
      <div class="panel__body">
        <div class="field-grid field-grid--two">
          <div class="field">
            <label for="ws-name">工作区名称</label>
            <input id="ws-name" type="text" bind:value={newName} onkeydown={(e) => e.key === "Enter" && handleCreate()} />
          </div>
          <div class="field">
            <label for="ws-mc">MC 版本</label>
            <select id="ws-mc" bind:value={newMcVersion}>
              {#each releaseVersions as version}
                <option value={version}>{version}</option>
              {/each}
            </select>
          </div>
        </div>

        <div class="field-grid field-grid--two">
          <div class="field">
            <label for="ws-loader-type">Loader 类型</label>
            <select id="ws-loader-type" bind:value={newLoaderType}>
              <option value="vanilla">Vanilla</option>
              <option value="fabric">Fabric</option>
              <option value="forge">Forge</option>
              <option value="neoforge">NeoForge</option>
            </select>
          </div>
          <div class="field">
            <label for="ws-loader-version">{loaderVersionLabel()}</label>
            {#if newLoaderType !== "vanilla"}
              <select id="ws-loader-version" bind:value={newLoaderVersion} disabled={loadingLoaderVersions || loaderVersions.length === 0}>
                {#each loaderVersions as loader}
                  <option value={loader.version}>{loader.version}{loader.stable ? " · stable" : ""}</option>
                {/each}
              </select>
            {:else}
              <input id="ws-loader-version" type="text" bind:value={newLoaderVersion} disabled />
            {/if}
            {#if loadingLoaderVersions}
              <div class="inline-message">正在加载版本列表...</div>
            {/if}
          </div>
        </div>

        <div class="field">
          <label for="ws-desc">描述</label>
          <textarea id="ws-desc" bind:value={newDesc}></textarea>
        </div>
      </div>
      <div class="panel__footer">
        <button class="secondary-button" onclick={() => (showCreatePanel = false)}>取消</button>
        <button class="primary-button" onclick={handleCreate} disabled={!newName.trim() || creating || (newLoaderType !== "vanilla" && !newLoaderVersion.trim())}>
          {creating ? "创建中..." : "创建工作区"}
        </button>
      </div>
    </div>
  </div>
{/if}
