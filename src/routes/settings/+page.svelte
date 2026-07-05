<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { AppSettings } from "$lib/types";

  const minPoolSize = 1;
  const maxPoolSize = 64;

  let settings = $state<AppSettings>({ download_pool_size: 16, theme: "dark" });
  let loading = $state(true);
  let saving = $state(false);
  let saveMessage = $state("");
  let saveError = $state("");

  async function loadSettings() {
    loading = true;
    saveError = "";
    try {
      settings = await invoke<AppSettings>("load_settings");
    } catch (e: any) {
      saveError = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadSettings();
  });

  async function handleSave() {
    if (saving) return;
    saving = true;
    saveMessage = "";
    saveError = "";
    try {
      settings = await invoke<AppSettings>("save_settings", {
        settings: {
          ...settings,
          download_pool_size: Math.min(maxPoolSize, Math.max(minPoolSize, Number(settings.download_pool_size) || 16))
        }
      });
      saveMessage = "设置已保存";
    } catch (e: any) {
      saveError = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<div class="min-h-screen p-6">
  <div class="flex items-center justify-between mb-8">
    <div>
      <h1 class="text-3xl font-bold">设置</h1>
      <p class="text-base-content/60 text-sm mt-1">管理全局下载行为与性能参数</p>
    </div>
  </div>

  {#if loading}
    <div class="flex justify-center py-24"><span class="loading loading-spinner loading-lg"></span></div>
  {:else}
    <div class="max-w-2xl card bg-base-200 border border-base-300">
      <div class="card-body gap-5">
        <div>
          <h2 class="card-title">界面主题</h2>
          <p class="text-sm text-base-content/60 mt-1">主题切换会立即生效，并同步保存到全局设置。</p>
        </div>

        <div class="grid gap-3 sm:grid-cols-2">
          <button
            class={`btn justify-start h-auto py-4 ${settings.theme === "dark" ? "btn-primary" : "btn-outline"}`}
            onclick={() => (settings.theme = "dark")}
          >
            深色主题
          </button>
          <button
            class={`btn justify-start h-auto py-4 ${settings.theme === "cupcake" ? "btn-primary" : "btn-outline"}`}
            onclick={() => (settings.theme = "cupcake")}
          >
            蛋糕主题
          </button>
        </div>

        <div>
          <h2 class="card-title">下载池上限</h2>
          <p class="text-sm text-base-content/60 mt-1">默认 16。值越大，同时下载的资源越多，但会占用更多带宽与系统资源。</p>
        </div>

        <div class="grid gap-4 md:grid-cols-[1fr_180px] md:items-end">
          <div>
            <label class="label" for="download-pool-size">
              <span class="label-text">并发下载数</span>
            </label>
            <input
              id="download-pool-size"
              class="range range-primary"
              type="range"
              min={minPoolSize}
              max={maxPoolSize}
              step="1"
              bind:value={settings.download_pool_size}
            />
            <div class="flex justify-between text-xs text-base-content/50 mt-2">
              <span>{minPoolSize}</span>
              <span>{maxPoolSize}</span>
            </div>
          </div>

          <div>
            <label class="label" for="download-pool-input">
              <span class="label-text">精确值</span>
            </label>
            <input
              id="download-pool-input"
              class="input input-bordered w-full"
              type="number"
              min={minPoolSize}
              max={maxPoolSize}
              bind:value={settings.download_pool_size}
            />
          </div>
        </div>

        <div class="stats shadow bg-base-100 border border-base-300">
          <div class="stat">
            <div class="stat-title">当前上限</div>
            <div class="stat-value text-3xl">{settings.download_pool_size}</div>
            <div class="stat-desc">超出范围时保存会自动限制在 1 到 64 之间</div>
          </div>
        </div>

        {#if saveMessage}
          <div class="alert alert-success text-sm"><span>{saveMessage}</span></div>
        {/if}
        {#if saveError}
          <div class="alert alert-error text-sm"><span>{saveError}</span></div>
        {/if}

        <div class="card-actions justify-end">
          <button class="btn btn-primary" onclick={handleSave} disabled={saving}>
            {saving ? "保存中..." : "保存设置"}
          </button>
        </div>
      </div>
    </div>
  {/if}
</div>
