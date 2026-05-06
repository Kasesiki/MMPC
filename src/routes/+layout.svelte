<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import type { AppSettings } from "$lib/types";
  import "../app.css";

  let { children } = $props();

  function applyTheme(theme: AppSettings["theme"]) {
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
</script>

<div class="app-shell">
  <div class="desktop-surface">
    {@render children()}
  </div>
</div>
