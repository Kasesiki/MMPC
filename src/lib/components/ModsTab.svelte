<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { ModrinthSearchResult, WorkspaceMod } from "$lib/types";

  let { workspace }: any = $props();

  let searchQuery = $state("");
  let showAddModal = $state(false);
  let searching = $state(false);
  let installingProjectId = $state("");
  let removingProjectId = $state("");
  let error = $state("");
  let searchResults = $state<ModrinthSearchResult[]>([]);
  let mods = $state<WorkspaceMod[]>([]);

  $effect(() => {
    mods = Array.isArray(workspace?.config?.mods) ? workspace.config.mods : [];
  });

  async function runSearch() {
    if (!workspace?.id || !searchQuery.trim()) {
      searchResults = [];
      return;
    }
    searching = true;
    error = "";
    try {
      searchResults = await invoke<ModrinthSearchResult[]>("search_modrinth_mods", {
        workspaceId: workspace.id,
        query: searchQuery.trim(),
      });
    } catch (e: any) {
      error = String(e);
      searchResults = [];
    } finally {
      searching = false;
    }
  }

  async function addMod(result: ModrinthSearchResult) {
    const version = result.latest_version;
    if (!workspace?.id || !version || installingProjectId) return;
    installingProjectId = result.project_id;
    error = "";
    try {
      const installed = await invoke<WorkspaceMod>("install_modrinth_mod", {
        workspaceId: workspace.id,
        projectId: result.project_id,
        versionId: version.version_id,
      });
      const next = mods.filter((item) => item.project_id !== installed.project_id);
      next.push(installed);
      mods = next;
      workspace.config = { ...workspace.config, mods: next };
    } catch (e: any) {
      error = String(e);
    } finally {
      installingProjectId = "";
    }
  }

  async function removeMod(projectId: string) {
    if (!workspace?.id || removingProjectId) return;
    removingProjectId = projectId;
    error = "";
    try {
      await invoke("remove_workspace_mod", {
        workspaceId: workspace.id,
        projectId,
      });
      const next = mods.filter((item) => item.project_id !== projectId);
      mods = next;
      workspace.config = { ...workspace.config, mods: next };
    } catch (e: any) {
      error = String(e);
    } finally {
      removingProjectId = "";
    }
  }

  function isInstalled(projectId: string) {
    return mods.some((item) => item.project_id === projectId);
  }
</script>

<div class="flex flex-col gap-4">
  <div class="flex items-center justify-between gap-3">
    <h3 class="text-lg font-semibold">模组 ({mods.length})</h3>
    <button class="btn btn-primary btn-sm" onclick={() => { showAddModal = true; searchResults = []; error = ""; }}>
      添加
    </button>
  </div>

  {#if error}
    <div class="alert alert-error"><span>{error}</span></div>
  {/if}

  {#if mods.length === 0}
    <div class="py-12 text-center text-base-content/50">
      <p>暂无模组</p>
    </div>
  {:else}
    <table class="table table-zebra">
      <thead>
        <tr>
          <th>名称</th>
          <th>版本</th>
          <th>MC</th>
          <th>缓存文件</th>
          <th class="w-20"></th>
        </tr>
      </thead>
      <tbody>
        {#each mods as mod}
          <tr>
            <td class="font-medium">{mod.title || mod.mod_name}</td>
            <td>{mod.mod_version || "-"}</td>
            <td><span class="badge badge-outline badge-sm">{mod.mc_version || workspace.mc_version}</span></td>
            <td class="text-xs break-all">{mod.file_name || "-"}</td>
            <td>
              <button
                class="btn btn-ghost btn-xs text-error"
                onclick={() => removeMod(mod.project_id)}
                disabled={removingProjectId === mod.project_id}
              >
                {removingProjectId === mod.project_id ? "移除中" : "移除"}
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

{#if showAddModal}
  <div class="modal modal-open">
    <div class="modal-box max-w-4xl">
      <h3 class="font-bold text-lg mb-4">从 Modrinth 添加模组</h3>
      <div class="flex gap-2 mb-4">
        <input
          type="text"
          class="input input-bordered w-full"
          placeholder="搜索模组名称..."
          bind:value={searchQuery}
          onkeydown={(e) => e.key === "Enter" && runSearch()}
        />
        <button class="btn btn-primary" onclick={runSearch} disabled={searching}>
          {searching ? "搜索中" : "搜索"}
        </button>
      </div>

      <div class="max-h-96 overflow-y-auto">
        {#if searching}
          <div class="py-10 text-center text-base-content/60">正在搜索...</div>
        {:else if searchResults.length === 0}
          <div class="py-10 text-center text-base-content/50">输入关键词后搜索支持当前工作区版本的模组</div>
        {:else}
          <div class="space-y-3">
            {#each searchResults as result}
              <div class="card bg-base-200 border border-base-300">
                <div class="card-body py-4">
                  <div class="flex items-start justify-between gap-4">
                    <div class="min-w-0">
                      <div class="font-semibold">{result.title}</div>
                      <div class="text-sm text-base-content/70 break-words">{result.description}</div>
                      {#if result.latest_version}
                        <div class="mt-2 flex flex-wrap gap-2 text-xs text-base-content/60">
                          <span class="badge badge-outline">{result.latest_version.version_number}</span>
                          <span class="badge badge-outline">{result.latest_version.game_versions.join(", ")}</span>
                          <span class="badge badge-outline">{result.latest_version.loaders.join(", ")}</span>
                        </div>
                      {/if}
                    </div>
                    <button
                      class="btn btn-primary btn-sm"
                      disabled={!result.latest_version || isInstalled(result.project_id) || installingProjectId === result.project_id}
                      onclick={() => addMod(result)}
                    >
                      {#if isInstalled(result.project_id)}
                        已安装
                      {:else if installingProjectId === result.project_id}
                        安装中
                      {:else}
                        添加
                      {/if}
                    </button>
                  </div>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <div class="modal-action">
        <button
          class="btn"
          onclick={() => {
            showAddModal = false;
            searchQuery = "";
            searchResults = [];
            error = "";
          }}
        >
          关闭
        </button>
      </div>
    </div>
  </div>
{/if}
