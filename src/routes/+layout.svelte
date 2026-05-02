<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { AppSettings } from "$lib/types";
  import "../app.css";

  let { children } = $props();

  let currentTheme = $state<AppSettings["theme"]>("dark");
  let savingTheme = false;

  function applyTheme(theme: AppSettings["theme"]) {
    currentTheme = theme;
    document.documentElement.setAttribute("data-theme", theme);
  }

  onMount(async () => {
    try {
      const settings = await invoke<AppSettings>("get_settings");
      applyTheme(settings.theme);
    } catch {
      applyTheme("dark");
    }
  });

  async function persistTheme(theme: AppSettings["theme"]) {
    if (savingTheme) return;
    savingTheme = true;
    try {
      const next = await invoke<AppSettings>("save_settings", {
        settings: {
          download_pool_size: 16,
          theme
        }
      });
      applyTheme(next.theme);
    } catch {
      applyTheme(theme);
    } finally {
      savingTheme = false;
    }
  }

  async function toggleTheme() {
    const nextTheme = currentTheme === "dark" ? "cupcake" : "dark";
    applyTheme(nextTheme);
    try {
      const settings = await invoke<AppSettings>("get_settings");
      await invoke<AppSettings>("save_settings", {
        settings: {
          ...settings,
          theme: nextTheme
        }
      });
    } catch {
      await persistTheme(nextTheme);
    }
  }
</script>

<div class="drawer lg:drawer-open h-screen">
  <input id="sidebar-drawer" type="checkbox" class="drawer-toggle" />

  <!-- Page content -->
  <div class="drawer-content flex flex-col">
    <!-- Top navbar (mobile) -->
    <nav class="navbar bg-base-200 lg:hidden shadow-sm">
      <div class="flex-none">
        <label for="sidebar-drawer" class="btn btn-square btn-ghost">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" class="inline-block h-6 w-6 stroke-current">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 6h16M4 12h16M4 18h16" />
          </svg>
        </label>
      </div>
      <div class="flex-1">
        <a href="/" class="text-xl font-bold">MMPC</a>
      </div>
      <button class="btn btn-ghost btn-sm" onclick={toggleTheme}>
        {currentTheme === "dark" ? "☀️" : "🌙"}
      </button>
    </nav>

    <!-- Main slot -->
    <main class="flex-1 p-4 lg:p-6 overflow-auto">
      {@render children()}
    </main>
  </div>

  <!-- Sidebar -->
  <div class="drawer-side z-40">
    <label for="sidebar-drawer" class="drawer-overlay"></label>
    <aside class="bg-base-200 min-h-full w-64 p-4 flex flex-col gap-4">
      <!-- Brand -->
      <a href="/" class="text-2xl font-bold tracking-tight pt-2 px-2">
        🧊 MMPC
      </a>
      <p class="text-xs text-base-content/60 px-2 -mt-3">
        Minecraft Modpack Maker
      </p>

      <!-- Nav links (can be extended later) -->
      <ul class="menu menu-md rounded-box flex-1">
        <li class="menu-title">导航</li>
        <li><a href="/">📦 工作区</a></li>
        <li><a href="/java">☕ Java 管理</a></li>
        <li><a href="/settings">⚙️ 设置</a></li>
      </ul>

      <!-- Theme toggle at bottom -->
      <div class="border-t border-base-300 pt-3 px-2 flex items-center justify-between">
        <span class="text-sm text-base-content/70">主题</span>
        <button class="btn btn-ghost btn-sm" onclick={toggleTheme}>
          {currentTheme === "dark" ? "🌙 深色" : "🧁 蛋糕"}
        </button>
      </div>
    </aside>
  </div>
</div>

<style>
  :global(body) {
    margin: 0;
    font-family: system-ui, -apple-system, sans-serif;
  }
</style>
