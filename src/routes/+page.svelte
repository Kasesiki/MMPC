<script lang="ts">
  import { onMount } from "svelte";
  import {
    loadWorkspaces,
    createWorkspaceOnDisk,
    deleteWorkspaceOnDisk,
    listReleaseVersions,
    listFabricLoaderVersions,
    listForgeLoaderVersions,
    listNeoForgeLoaderVersions
  } from "$lib/stores/workspace";
  import { workspaces } from "$lib/stores/workspace";
  import { goto } from "$app/navigation";
  import type { LoaderVersionOption } from "$lib/types";

  const fallbackReleaseVersions = ["1.21", "1.20.6", "1.20.4", "1.20.1", "1.19.4", "1.18.2", "1.16.5", "1.12.2"];

  let wsList = $state<any[]>([]);
  let loading = $state(true);
  let loadingVersions = $state(true);
  let showNewModal = $state(false);
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
    const [versions] = await Promise.all([
      listReleaseVersions(),
      loadWorkspaces().then(() => {
        loading = false;
      })
    ]);
    if (versions.length > 0) {
      releaseVersions = versions;
      if (!versions.includes(newMcVersion)) {
        newMcVersion = versions[0];
      }
    }
    loadingVersions = false;
  });
  $effect(() => { wsList = $workspaces; });

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

  function loaderVersionPlaceholder() {
    if (newLoaderType === "forge") return "选择 Forge 版本";
    if (newLoaderType === "neoforge") return "选择 NeoForge 版本";
    if (newLoaderType === "fabric") return "选择 Fabric 版本";
    return "";
  }

  async function handleCreate() {
    if (!newName.trim() || creating) return;
    creating = true;
    const ws = await createWorkspaceOnDisk(
      newName.trim(),
      newMcVersion,
      newDesc,
      newLoaderType,
      newLoaderType === "vanilla" ? null : newLoaderVersion.trim() || null
    );
    creating = false;
    showNewModal = false;
    newName = "";
    newLoaderType = "vanilla";
    newLoaderVersion = "";
    newDesc = "";
    if (ws) goto(`/workspace/${ws.id}`);
  }

  function handleDelete(id: string) {
    if (confirm("确认删除？")) deleteWorkspaceOnDisk(id);
  }

  function fmt(iso: string) {
    return new Date(iso).toLocaleDateString("zh-CN", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
  }
</script>

<div class="min-h-screen p-6">
  <div class="flex items-center justify-between mb-8">
    <div>
      <h1 class="text-3xl font-bold">MMPC</h1>
      <p class="text-base-content/60 text-sm mt-1">Minecraft Modpack Maker</p>
    </div>
    <button class="btn btn-primary" onclick={() => showNewModal = true}>+ 新建工作区</button>
  </div>

  {#if loading}
    <div class="flex justify-center py-24"><span class="loading loading-spinner loading-lg"></span></div>
  {:else if wsList.length === 0}
    <div class="flex flex-col items-center py-24 text-base-content/40">
      <p class="text-lg">还没有工作区</p>
      <p class="text-sm mt-1">点击上方按钮创建</p>
    </div>
  {:else}
    <div class="overflow-x-auto">
      <table class="table table-zebra">
        <thead><tr><th>名称</th><th>MC 版本</th><th>模组</th><th class="max-md:hidden">描述</th><th>最近打开</th><th></th></tr></thead>
        <tbody>
          {#each wsList as ws}
            <tr class="hover cursor-pointer" onclick={() => goto(`/workspace/${ws.id}`)} role="button" tabindex="0"
              onkeydown={(e) => e.key === "Enter" && goto(`/workspace/${ws.id}`)}>
              <td class="font-medium">{ws.name}</td>
              <td><span class="badge badge-outline badge-sm">MC {ws.mc_version}</span></td>
              <td>{ws.mod_count} 个</td>
              <td class="text-sm text-base-content/60 max-md:hidden max-w-xs truncate">{ws.description || "—"}</td>
              <td class="text-sm text-base-content/50">{fmt(ws.last_opened)}</td>
              <td><button class="btn btn-ghost btn-xs text-error" onclick={(e) => { e.stopPropagation(); handleDelete(ws.id); }}>✕</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

{#if showNewModal}
  <div class="modal modal-open">
    <div class="modal-box">
      <h3 class="font-bold text-lg mb-4">新建工作区</h3>
      <div class="form-control gap-4">
        <div>
          <label class="label" for="ws-name"><span class="label-text">名称</span></label>
          <input id="ws-name" type="text" class="input input-bordered w-full" bind:value={newName}
            onkeydown={(e) => e.key === "Enter" && handleCreate()} />
        </div>
        <div>
          <label class="label" for="ws-mc"><span class="label-text">MC 版本</span></label>
          <select id="ws-mc" class="select select-bordered w-full" bind:value={newMcVersion}>
            {#each releaseVersions as version}
              <option value={version}>{version}</option>
            {/each}
          </select>
          {#if loadingVersions}
            <p class="text-xs text-base-content/50 mt-2">正在从 Mojang 官方加载正式版版本列表...</p>
          {/if}
        </div>
        <div class="grid grid-cols-1 gap-4 md:grid-cols-2">
          <div>
            <label class="label" for="ws-loader-type"><span class="label-text">Loader</span></label>
            <select id="ws-loader-type" class="select select-bordered w-full" bind:value={newLoaderType}>
              <option value="vanilla">Vanilla</option>
              <option value="fabric">Fabric</option>
              <option value="forge">Forge</option>
              <option value="neoforge">NeoForge</option>
            </select>
          </div>
          <div>
            <label class="label" for="ws-loader-version"><span class="label-text">Loader 版本</span></label>
            {#if newLoaderType !== "vanilla"}
              <select
                id="ws-loader-version"
                class="select select-bordered w-full"
                bind:value={newLoaderVersion}
                disabled={loadingLoaderVersions || loaderVersions.length === 0}
              >
                {#each loaderVersions as loader}
                  <option value={loader.version}>
                    {loader.version}{loader.stable ? " · stable" : ""}
                  </option>
                {/each}
              </select>
              {#if loadingLoaderVersions}
                <p class="text-xs text-base-content/50 mt-2">正在加载 {loaderVersionPlaceholder()}...</p>
              {/if}
            {:else}
              <input
                id="ws-loader-version"
                type="text"
                class="input input-bordered w-full"
                placeholder=""
                bind:value={newLoaderVersion}
                disabled={newLoaderType === "vanilla"}
              />
            {/if}
          </div>
        </div>
        <div>
          <label class="label" for="ws-desc"><span class="label-text">描述</span></label>
          <textarea id="ws-desc" class="textarea textarea-bordered w-full" rows="3" bind:value={newDesc}></textarea>
        </div>
      </div>
      <div class="modal-action">
        <button class="btn btn-ghost" onclick={() => showNewModal = false}>取消</button>
        <button
          class="btn btn-primary"
          onclick={handleCreate}
          disabled={!newName.trim() || creating || (newLoaderType !== "vanilla" && !newLoaderVersion.trim())}
        >
          {creating ? "创建中..." : "创建"}
        </button>
      </div>
    </div>
    <div class="modal-backdrop" role="button" tabindex="0" onclick={() => showNewModal = false}
      onkeydown={(e) => e.key === "Enter" && (showNewModal = false)}></div>
  </div>
{/if}
