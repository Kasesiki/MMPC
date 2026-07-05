<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { activeWorkspaceId, launchStatus, workspaces } from "$lib/stores/workspace";
  import type { AppSettings, JavaRuntime, ModrinthProjectHit, ModUsageType, PackConfig, Workspace, WorkspaceMod, WorkspaceModOverview } from "$lib/types";

  type ViewMode = "overview" | "mods" | "export";
  type ExportKind = "client" | "server";
  type ExportProgress = {
    stage: string;
    current: number;
    total: number;
    message?: string | null;
  };

  let { params } = $props();

  let ws = $state<Workspace | null>(null);
  let fullCfg = $state<PackConfig | null>(null);
  let javaList = $state<JavaRuntime[]>([]);
  let gameLogs = $state<string[]>([]);
  let viewMode = $state<ViewMode>("overview");
  let sidebarWidth = $state(312);
  let panelHeight = $state(248);
  let searchQuery = $state("");
  let installedMods = $state<WorkspaceModOverview[]>([]);
  let overviewLoading = $state(false);
  let overviewError = $state("");
  let togglingProjectId = $state("");
  let updatingTypeProjectId = $state("");
  let searchResults = $state<ModrinthProjectHit[]>([]);
  let modSearchLoading = $state(false);
  let modSearchError = $state("");
  let installingProjectIds = $state<string[]>([]);
  let descriptionDraft = $state("");
  let descriptionSaving = $state(false);
  let descriptionError = $state("");
  let currentTheme = $state<AppSettings["theme"]>("dark");
  let savingTheme = $state(false);
  let exportKind = $state<ExportKind>("client");
  let includeJava = $state(false);
  let exporting = $state(false);
  let exportError = $state("");
  let exportSuccess = $state("");
  let exportProgress = $state<ExportProgress | null>(null);
  let exportLogs = $state<string[]>([]);
  let terminalLogEl = $state<HTMLDivElement | null>(null);
  let exportLogEl = $state<HTMLDivElement | null>(null);

  function pushLog(message: string) {
    gameLogs = [...gameLogs, message].slice(-160);
  }

  function pushExportLog(message: string) {
    const trimmed = message.trim();
    if (!trimmed) return;
    exportLogs = [...exportLogs, trimmed].slice(-200);
  }

  function applyTheme(theme: AppSettings["theme"]) {
    currentTheme = theme;
    document.documentElement.setAttribute("data-theme", theme);
  }

  onMount(() => {
    const id = params.id;
    activeWorkspaceId.set(id);

    const unsub = workspaces.subscribe((list) => {
      ws = list.find((w) => w.id === id) ?? null;
    });

    invoke("get_pack_config", { id }).then((cfg: any) => {
      fullCfg = cfg;
      if (ws) ws = { ...ws, config: cfg };
      void loadInstalledMods(id);
    }).catch(() => {});

    invoke<AppSettings>("get_settings").then((settings) => {
      applyTheme(settings.theme);
    }).catch(() => {
      applyTheme("dark");
    });

    invoke<JavaRuntime[]>("list_java_runtimes").then((list) => {
      javaList = list;
    }).catch(() => {});

    const unlistenProgress = listen<any>("download-progress", (event) => {
      const stage = String(event.payload?.stage ?? "启动中");
      const current = Number(event.payload?.current ?? 0);
      const total = Number(event.payload?.total ?? 0);
      launchStatus.update((prev) => prev.state === "launching"
        ? { state: "launching", stage, current, total }
        : prev);
      pushLog(`[download] ${total > 0 ? `${stage} ${current}/${total}` : stage}`);
    });

    const unlistenGame = listen<any>("game-status", (event) => {
      const state = String(event.payload?.state ?? "");
      const message = String(event.payload?.message ?? "");

      if (state === "log" && message) {
        pushLog(message);
      } else if (state === "stderr" && message) {
        pushLog(`[stderr] ${message}`);
      } else if (state === "stopped") {
        launchStatus.set({ state: "idle" });
        pushLog("[info] 游戏进程已结束");
      } else if (message) {
        pushLog(`[${state || "game"}] ${message}`);
      }
    });

    const unlistenExport = listen<ExportProgress>("export-progress", (event) => {
      exportProgress = event.payload;
      const progressText = event.payload.total > 0
        ? `[${event.payload.current}/${event.payload.total}] ${event.payload.stage}`
        : event.payload.stage;
      pushExportLog(event.payload.message ? `${progressText} - ${event.payload.message}` : progressText);
    });

    return () => {
      unsub();
      unlistenProgress.then((off) => off());
      unlistenGame.then((off) => off());
      unlistenExport.then((off) => off());
    };
  });

  let status = $state<any>({ state: "idle" });
  $effect(() => {
    status = $launchStatus;
  });

  $effect(() => {
    if (viewMode === "overview" && ws && installedMods.length === 0 && !overviewLoading && !overviewError) {
      void loadInstalledMods(ws.id);
    }
  });

  $effect(() => {
    if (viewMode === "mods" && ws && searchResults.length === 0 && !modSearchLoading && !modSearchError) {
      void runModSearch();
    }
  });

  $effect(() => {
    descriptionDraft = fullCfg?.description ?? ws?.description ?? "";
  });

  $effect(() => {
    exportLogs.length;
    if (exportLogEl) {
      requestAnimationFrame(() => {
        if (exportLogEl) {
          exportLogEl.scrollTop = exportLogEl.scrollHeight;
        }
      });
    }
  });

  $effect(() => {
    gameLogs.length;
    if (terminalLogEl) {
      requestAnimationFrame(() => {
        if (terminalLogEl) {
          terminalLogEl.scrollTop = terminalLogEl.scrollHeight;
        }
      });
    }
  });

  function formatLoader() {
    const loaderType = String(fullCfg?.loader_type || ws?.loader_type || "vanilla");
    const loaderVersion = String(fullCfg?.loader_version || ws?.loader_version || "").trim();
    if (loaderType === "vanilla") return "Vanilla";
    const title = loaderType === "neoforge" ? "NeoForge" : loaderType.charAt(0).toUpperCase() + loaderType.slice(1);
    return loaderVersion ? `${title} ${loaderVersion}` : title;
  }

  function currentJavaLabel() {
    const runtimeId = fullCfg?.java_runtime_id;
    if (!runtimeId) return "默认";
    const found = javaList.find((j) => j.id === runtimeId);
    if (!found) return "已删除";
    const major = found.major_version ? `Java ${found.major_version}` : found.version_text;
    return `${found.name} (${major})`;
  }

  function launchProgressText() {
    if (status.state === "running") return `运行中 PID ${status.pid}`;
    if (status.state === "error") return "启动失败";
    if (status.state !== "launching") return "准备启动";
    const stage = status.stage || "启动中";
    const current = Number(status.current ?? 0);
    const total = Number(status.total ?? 0);
    return total > 0 ? `${stage} ${current}/${total}` : stage;
  }

  function modTypeLabel(modType: ModUsageType | string | undefined) {
    switch (modType) {
      case "client_only":
        return "仅客户端";
      case "server_only":
        return "仅服务端";
      case "client_and_server":
        return "客户端/服务端";
      case "development_only":
        return "开发依赖";
      default:
        return "未知";
    }
  }

  function normalizeWorkspaceMod(mod: WorkspaceMod): WorkspaceMod {
    return {
      ...mod,
      enabled: mod.enabled ?? true,
    };
  }

  function syncLocalMod(updated: WorkspaceMod) {
    const normalized = normalizeWorkspaceMod(updated);
    const nextMods = (fullCfg?.mods || []).map((item) =>
      item.project_id === normalized.project_id ? { ...item, ...normalized } : item,
    );
    if (fullCfg) {
      fullCfg = { ...fullCfg, mods: nextMods };
    }
    installedMods = installedMods.map((item) =>
      item.project_id === normalized.project_id
        ? {
            ...item,
            title: normalized.title || item.title,
            mod_name: normalized.mod_name,
            mod_version: normalized.mod_version,
            enabled: normalized.enabled ?? true,
            mod_type: (normalized.mod_type as ModUsageType | undefined) ?? item.mod_type,
          }
        : item,
    );
  }

  async function loadInstalledMods(workspaceId: string) {
    overviewLoading = true;
    overviewError = "";
    try {
      installedMods = await invoke<WorkspaceModOverview[]>("list_workspace_mods", {
        workspaceId,
      });
    } catch (e: any) {
      overviewError = String(e);
      installedMods = [];
    } finally {
      overviewLoading = false;
    }
  }

  async function toggleModEnabled(mod: WorkspaceModOverview, enabled: boolean) {
    if (!ws?.id || togglingProjectId) return;
    togglingProjectId = mod.project_id;
    overviewError = "";
    try {
      const updated = await invoke<WorkspaceMod>("set_workspace_mod_enabled", {
        workspaceId: ws.id,
        projectId: mod.project_id,
        enabled,
      });
      syncLocalMod(updated);
      pushLog(`[mod] ${enabled ? "已启用" : "已禁用"} ${mod.title}`);
    } catch (e: any) {
      overviewError = String(e);
      pushLog(`[error] ${String(e)}`);
    } finally {
      togglingProjectId = "";
    }
  }

  async function updateModType(mod: WorkspaceModOverview, modType: ModUsageType) {
    if (!ws?.id || updatingTypeProjectId) return;
    updatingTypeProjectId = mod.project_id;
    overviewError = "";
    try {
      const updated = await invoke<WorkspaceMod>("update_workspace_mod_type", {
        workspaceId: ws.id,
        projectId: mod.project_id,
        modType,
      });
      syncLocalMod(updated);
      pushLog(`[mod] 已更新 ${mod.title} 的类型为 ${modTypeLabel(modType)}`);
    } catch (e: any) {
      overviewError = String(e);
      pushLog(`[error] ${String(e)}`);
    } finally {
      updatingTypeProjectId = "";
    }
  }

  async function handleLaunch() {
    if (!ws) return;
    launchStatus.set({ state: "launching", stage: "准备启动", current: 0, total: 0 });
    pushLog("[info] 启动任务已提交");
    try {
      const pid: number = await invoke("launch_game", {
        workspaceId: ws.id,
        playerName: "Player",
      });
      launchStatus.set({ state: "running", pid });
      pushLog(`[info] 游戏进程已启动 PID=${pid}`);
    } catch (e: any) {
      const message = String(e);
      launchStatus.set({ state: "error", message });
      pushLog(`[error] ${message}`);
    }
  }

  async function handleStop() {
    if (status.state !== "running") return;
    try {
      await invoke("stop_game", { pid: status.pid });
      launchStatus.set({ state: "idle" });
      pushLog("[info] 已请求停止游戏进程");
    } catch (e: any) {
      const message = String(e);
      launchStatus.set({ state: "error", message });
      pushLog(`[error] ${message}`);
    }
  }

  async function toggleTheme(nextIsLight: boolean) {
    const nextTheme: AppSettings["theme"] = nextIsLight ? "cupcake" : "dark";
    applyTheme(nextTheme);
    if (savingTheme) return;
    savingTheme = true;
    try {
      const settings = await invoke<AppSettings>("get_settings");
      const saved = await invoke<AppSettings>("save_settings", {
        settings: {
          ...settings,
          theme: nextTheme,
        },
      });
      applyTheme(saved.theme);
    } catch {
      applyTheme(nextTheme);
    } finally {
      savingTheme = false;
    }
  }

  async function saveDescription() {
    const nextDescription = descriptionDraft.trim();
    if (!ws || !fullCfg || descriptionSaving || nextDescription === (fullCfg.description ?? "").trim()) {
      return;
    }
    descriptionSaving = true;
    descriptionError = "";
    try {
      const nextCfg = {
        ...fullCfg,
        description: nextDescription,
      };
      await invoke("save_pack_config", { id: ws.id, config: nextCfg });
      fullCfg = nextCfg as PackConfig;
      ws = { ...ws, description: nextDescription };
      workspaces.update((list) => list.map((item) => item.id === ws?.id ? { ...item, description: nextDescription } : item));
      pushLog("[info] 工作区描述已保存");
    } catch (e: any) {
      descriptionError = String(e);
    } finally {
      descriptionSaving = false;
    }
  }

  function startSidebarResize(event: MouseEvent) {
    event.preventDefault();
    const startX = event.clientX;
    const startWidth = sidebarWidth;

    const onMove = (moveEvent: MouseEvent) => {
      sidebarWidth = Math.min(520, Math.max(220, startWidth + moveEvent.clientX - startX));
    };

    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  function startPanelResize(event: MouseEvent) {
    event.preventDefault();
    const startY = event.clientY;
    const startHeight = panelHeight;

    const onMove = (moveEvent: MouseEvent) => {
      panelHeight = Math.min(460, Math.max(160, startHeight - (moveEvent.clientY - startY)));
    };

    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  }

  async function runModSearch() {
    if (!ws?.id) return;
    modSearchLoading = true;
    modSearchError = "";
    try {
      searchResults = await invoke<ModrinthProjectHit[]>("search_modrinth_mods", {
        workspaceId: ws.id,
        query: searchQuery.trim() ? searchQuery.trim() : null,
      });
    } catch (e: any) {
      modSearchError = String(e);
      searchResults = [];
    } finally {
      modSearchLoading = false;
    }
  }

  function installedProjectIds() {
    return new Set((fullCfg?.mods || []).map((mod) => mod.project_id));
  }

  function isInstallingProject(projectId: string) {
    return installingProjectIds.includes(projectId);
  }

  async function addMod(hit: ModrinthProjectHit) {
    if (!ws?.id || isInstallingProject(hit.project_id)) return;
    installingProjectIds = [...installingProjectIds, hit.project_id];
    modSearchError = "";
    try {
      const installed = await invoke<WorkspaceMod>("install_modrinth_mod", {
        workspaceId: ws.id,
        projectId: hit.project_id,
      });
      const normalized = normalizeWorkspaceMod(installed);
      const nextMods = [...(fullCfg?.mods || []).filter((item) => item.project_id !== normalized.project_id), normalized];
      if (fullCfg) {
        fullCfg = { ...fullCfg, mods: nextMods };
      }
      await loadInstalledMods(ws.id);
      ws = { ...ws, mod_count: nextMods.length };
      workspaces.update((list) => list.map((item) => item.id === ws?.id ? { ...item, mod_count: nextMods.length } : item));
      pushLog(`[mod] 已添加 ${hit.title}`);
    } catch (e: any) {
      modSearchError = String(e);
      pushLog(`[error] ${String(e)}`);
    } finally {
      installingProjectIds = installingProjectIds.filter((id) => id !== hit.project_id);
    }
  }

  async function runExport() {
    if (!ws?.id || exporting) return;
    exporting = true;
    exportError = "";
    exportSuccess = "";
    exportProgress = {
      stage: "等待导出开始",
      current: 0,
      total: 1,
      message: null,
    };
    exportLogs = [];
    pushExportLog("[0/1] 等待导出开始");
    try {
      const result = await invoke<{ export_dir: string }>("export_workspace", {
        request: {
          workspace_id: ws.id,
          export_kind: exportKind,
          include_java: includeJava,
        },
      });
      exportSuccess = `导出完成：${result.export_dir}`;
      pushExportLog(`[done] ${exportSuccess}`);
      pushLog(`[export] ${exportSuccess}`);
    } catch (e: any) {
      exportError = String(e);
      pushExportLog(`[error] ${exportError}`);
      pushLog(`[error] ${exportError}`);
    } finally {
      exporting = false;
    }
  }
</script>

{#if ws && fullCfg}
  <div
    class="workspace-shell workspace-shell--custom"
    style={`grid-template-columns: 56px ${sidebarWidth}px 4px minmax(0, 1fr); grid-template-rows: 52px minmax(0, 1fr) 4px ${panelHeight}px;`}
  >
    <aside class="activity-bar">
      <div class="activity-group">
        <button class={`activity-button ${viewMode === "overview" ? "is-active" : ""}`} aria-label="Explorer" onclick={() => (viewMode = "overview")}>
          <span>▣</span>
        </button>
        <button class={`activity-button ${viewMode === "mods" ? "is-active" : ""}`} aria-label="Mod" onclick={() => (viewMode = "mods")}>
          <span>M</span>
        </button>
        <button class={`activity-button ${viewMode === "export" ? "is-active" : ""}`} aria-label="Export" onclick={() => (viewMode = "export")}>
          <span>⇪</span>
        </button>
      </div>
    </aside>

    <header class="workspace-topbar">
      <div class="workspace-topbar__left">

      </div>

      <div class="workspace-topbar__right">
        <div class="workspace-status">{launchProgressText()}</div>
        {#if status.state === "running"}
          <button class="primary-button" onclick={handleStop}>停止游戏</button>
        {:else}
          <button class="primary-button" onclick={handleLaunch}>启动游戏</button>
        {/if}
        <input
          type="checkbox"
          class="toggle toggle-sm theme-controller workspace-theme-toggle"
          aria-label="切换主题"
          checked={currentTheme === "cupcake"}
          onchange={(e) => toggleTheme((e.currentTarget as HTMLInputElement).checked)}
        />
      </div>
    </header>

    <aside class="sidebar" style={`grid-column: 2; grid-row: 2; width: ${sidebarWidth}px;`}>
      <div class="sidebar__header">
        <div>
          <div class="sidebar__eyebrow">{viewMode === "mods" ? "mod" : viewMode === "export" ? "export" : "overview"}</div>
          <h1 class="sidebar__title">{ws.name}</h1>
        </div>
      </div>

      <div class="sidebar__content sidebar__content--flat">
        <div class="panel-heading">Workspace</div>
        <div class="meta-grid">
          <div class="meta-row">
            <span class="meta-row__label">MC 版本</span>
            <span class="meta-row__value">{fullCfg.mc_version || ws.mc_version}</span>
          </div>
          <div class="meta-row">
            <span class="meta-row__label">Loader 版本</span>
            <span class="meta-row__value">{formatLoader()}</span>
          </div>
          <div class="meta-row">
            <span class="meta-row__label">Java</span>
            <span class="meta-row__value">{currentJavaLabel()}</span>
          </div>
          <div class="meta-row">
            <span class="meta-row__label">模组数</span>
            <span class="meta-row__value">{fullCfg.mods.length}</span>
          </div>
        </div>

        <button class="ghost-button" onclick={() => goto(`/java?workspaceId=${params.id}`)}>配置 Java</button>

        <label class="panel-heading" for="workspace-description">Description</label>
        <textarea
          id="workspace-description"
          class="workspace-description-editor"
          bind:value={descriptionDraft}
          placeholder="输入工作区描述..."
          onblur={saveDescription}
          onkeydown={(e) => (e.key === "Enter" && (e.metaKey || e.ctrlKey)) && saveDescription()}
        ></textarea>
        {#if descriptionSaving}
          <div class="inline-message">正在保存描述...</div>
        {/if}
        {#if descriptionError}
          <div class="inline-message" style="color: var(--vscode-danger);">{descriptionError}</div>
        {/if}

        {#if status.state === "error"}
          <div class="inline-message" style="color: var(--vscode-danger);">{status.message}</div>
        {/if}
      </div>
    </aside>

    <button
      class="sidebar-resizer"
      type="button"
      aria-label="调整侧边栏宽度"
      onmousedown={startSidebarResize}
    ></button>

    <main class="main-editor" style="grid-column: 4; grid-row: 2;">
      {#if viewMode === "overview"}
        <div class="overview-editor">
          <div class="overview-header">
            <div>
              <div class="panel-heading">Installed Mods</div>
              <div class="inline-message">查看当前工作区已安装模组，并控制是否在启动时启用。</div>
            </div>
            <div class="overview-summary">{installedMods.length} 个模组</div>
          </div>

          {#if overviewError}
            <div class="mod-inline-error">{overviewError}</div>
          {/if}

          <div class="overview-list-scroll">
            <div class="overview-mod-list">
              {#each installedMods as mod}
                <article class="overview-mod-card">
                  <div class="overview-mod-card__main">
                    <div class="overview-mod-card__title-row">
                      <div class="overview-mod-card__title">{mod.title}</div>
                      <div class={`overview-mod-card__status ${mod.enabled ? "is-enabled" : "is-disabled"}`}>
                        {mod.enabled ? "已启用" : "已禁用"}
                      </div>
                    </div>
                    <div class="overview-mod-card__meta">
                      <span>版本 {mod.mod_version}</span>
                      <span>{mod.mod_name}</span>
                    </div>
                    <label class="overview-mod-card__field">
                      <span>类型</span>
                      <select
                        class="select select-bordered select-sm overview-mod-card__select"
                        value={mod.mod_type}
                        disabled={updatingTypeProjectId === mod.project_id}
                        onchange={(e) => updateModType(mod, (e.currentTarget as HTMLSelectElement).value as ModUsageType)}
                      >
                        <option value="client_only">仅客户端</option>
                        <option value="server_only">仅服务端</option>
                        <option value="client_and_server">双端</option>
                      </select>
                    </label>
                  </div>
                  <label class="overview-mod-card__toggle">
                    <span>启用</span>
                    <input
                      type="checkbox"
                      class="toggle toggle-primary"
                      checked={mod.enabled}
                      disabled={togglingProjectId === mod.project_id}
                      onchange={(e) => toggleModEnabled(mod, (e.currentTarget as HTMLInputElement).checked)}
                    />
                  </label>
                </article>
              {/each}
            </div>
          </div>
        </div>
      {:else if viewMode === "mods"}
        <div class="mod-editor">
          <div class="mod-editor__toolbar">
            <input
              class="mod-search-input"
              type="text"
              placeholder="搜索 Modrinth 模组，留空显示热门模组"
              bind:value={searchQuery}
              onkeydown={(e) => e.key === "Enter" && runModSearch()}
            />
            <button class="primary-button" onclick={runModSearch} disabled={modSearchLoading}>
              {modSearchLoading ? "加载中..." : "搜索"}
            </button>
          </div>
          <div class="inline-message">留空显示热门模组，输入关键词后按回车或点击搜索。</div>
          {#if modSearchError}
            <div class="mod-inline-error">{modSearchError}</div>
          {/if}

          <div class="mod-results-scroll">
            {#if modSearchLoading && searchResults.length === 0}
              <div class="editor-empty">
                <h2>Loading Mods</h2>
                <p>正在从 Modrinth 拉取模组列表。</p>
              </div>
            {:else if searchResults.length === 0}
              <div class="editor-empty">
                <h2>No Mods</h2>
                <p>当前没有可展示的模组结果。</p>
              </div>
            {:else}
              {@const installedIds = installedProjectIds()}
              <div class="mod-results">
                {#each searchResults as result}
                  <article class="mod-result">
                    <div class="mod-list-icon">
                      {#if result.icon_url}
                        <div class="mod-list-icon-box">
                          <img src={result.icon_url} alt={`${result.title} icon`} loading="lazy" />
                        </div>
                      {:else}
                        <div class="mod-list-icon-box mod-list-avatar">
                          <span>{result.title.slice(0, 1).toUpperCase()}</span>
                        </div>
                      {/if}
                    </div>
                    <div class="mod-list-content">
                      <div class="mod-list-item__title-row">
                        <div class="mod-list-item__title">{result.title}</div>
                        <div class="mod-list-item__badge">{result.downloads.toLocaleString()} downloads</div>
                      </div>
                      <p class="mod-list-item__desc" style="margin-top: 4px;">{result.description}</p>
                    </div>
                    <div class="mod-list-item__actions">
                      <button
                        class="primary-button"
                        onclick={() => addMod(result)}
                        disabled={isInstallingProject(result.project_id) || installedIds.has(result.project_id)}
                      >
                        {installedIds.has(result.project_id) ? "已安装" : isInstallingProject(result.project_id) ? "安装中..." : "安装"}
                      </button>
                    </div>
                  </article>
                {/each}
              </div>
            {/if}
          </div>
        </div>
      {:else if viewMode === "export"}
        <div class="export-editor">
          <div class="export-editor__section">
            <div class="panel-heading">Export Options</div>
            <div class="export-mode-grid">
              <label class="export-mode-card">
                <input type="radio" class="radio radio-primary" bind:group={exportKind} value="client" />
                <div>
                  <div class="export-mode-card__title">客户端</div>
                  <div class="export-mode-card__desc">导出完整客户端运行环境。</div>
                </div>
              </label>
              <label class="export-mode-card">
                <input type="radio" class="radio radio-primary" bind:group={exportKind} value="server" />
                <div>
                  <div class="export-mode-card__title">服务端</div>
                  <div class="export-mode-card__desc">导出服务器运行所需文件。</div>
                </div>
              </label>
            </div>

            <label class="label cursor-pointer justify-start gap-3 export-checkbox-row">
              <input type="checkbox" class="checkbox checkbox-primary" bind:checked={includeJava} />
              <span class="label-text">附带导出当前工作区使用的 Java</span>
            </label>

            <div class="export-action-row">
              <button class="primary-button" onclick={runExport} disabled={exporting}>
                {exporting ? "导出中..." : "开始导出"}
              </button>
            </div>
          </div>

          <div class="export-editor__section">
            <div class="panel-heading">Export Progress</div>

            {#if exportError}
              <div class="alert alert-error">
                <span>{exportError}</span>
              </div>
            {/if}

            {#if exportSuccess}
              <div class="alert alert-success">
                <span>{exportSuccess}</span>
              </div>
            {/if}

            {#if exportLogs.length > 0}
              <div class="export-log" bind:this={exportLogEl}>
                {#each exportLogs as line}
                  <div class={`export-log__line ${line.startsWith("[error]") ? "is-error" : ""}`}>{line}</div>
                {/each}
              </div>
            {:else}
              <div class="empty-state export-empty-state">等待导出任务开始。</div>
            {/if}

            {#if exportProgress}
              <div class="export-progress-meta">
                <div>{exportProgress.stage}</div>
                <div>{exportProgress.current}/{exportProgress.total}</div>
              </div>
              {#if exportProgress.message}
                <div class="inline-message">{exportProgress.message}</div>
              {/if}
            {/if}
          </div>
        </div>
      {:else}
        <!-- <div class="editor-surface">
          <div class="editor-empty">
            <h2>File Editor Placeholder</h2>
            <p>Main Editor 现在作为文件展示区预留。后续可以在这里接入工作区文件树、配置文件和资源文件编辑。</p>
          </div>
        </div> -->
      {/if}
    </main>

    <button
      class="panel-resizer"
      type="button"
      aria-label="调整终端高度"
      onmousedown={startPanelResize}
    ></button>

    <section class="bottom-panel bottom-panel--attached" style="grid-column: 2 / 5; grid-row: 4;">
      <div class="bottom-panel__tabs">
        <div class="bottom-panel__title">Terminal</div>
        <div class="bottom-panel__title">workspace logs</div>
      </div>
      <div class="bottom-panel__body bottom-panel__body--terminal">
        <div class="terminal-log terminal-log--stacked" bind:this={terminalLogEl}>
          {#if gameLogs.length === 0}
            <div class="terminal-line is-muted">[terminal] 等待命令输出...</div>
          {:else}
            {#each gameLogs as line}
              <div class={`terminal-line ${line.includes("[error]") || line.includes("[stderr]") ? "is-error" : ""}`}>
                {line}
              </div>
            {/each}
          {/if}
        </div>
        <!-- <form class="terminal-form terminal-form--attached" onsubmit={(e) => { e.preventDefault(); submitTerminal(); }}>
          <span class="terminal-prompt">$</span>
          <input class="terminal-input" type="text" bind:value={terminalInput} placeholder="command line" />
          <button class="secondary-button" type="submit">Send</button>
        </form> -->
      </div>
    </section>
  </div>
{:else if ws && !fullCfg}
  <div class="home-shell">
    <div class="panel panel--narrow">
      <div class="panel__body">
        <div class="empty-state">正在加载工作区配置...</div>
      </div>
    </div>
  </div>
{:else}
  <div class="home-shell">
    <div class="panel panel--narrow">
      <div class="panel__body">
        <div class="empty-state">工作区未找到</div>
      </div>
    </div>
  </div>
{/if}
