<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { activeWorkspaceId, launchStatus, workspaces } from "$lib/stores/workspace";
  import type { AppSettings, JavaRuntime, ModrinthProjectHit, PackConfig, Workspace, WorkspaceMod } from "$lib/types";

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
  let showJavaModal = $state(false);
  let javaSaving = $state(false);
  let selectedJavaId = $state("");
  let javaErr = $state("");
  let gameLogs = $state<string[]>([]);
  let viewMode = $state<ViewMode>("overview");
  let sidebarWidth = $state(312);
  let panelHeight = $state(248);
  let searchQuery = $state("");
  let searchResults = $state<ModrinthProjectHit[]>([]);
  let modSearchLoading = $state(false);
  let modSearchError = $state("");
  let installingProjectId = $state("");
  let currentTheme = $state<AppSettings["theme"]>("dark");
  let savingTheme = $state(false);
  let exportKind = $state<ExportKind>("client");
  let includeJava = $state(false);
  let exporting = $state(false);
  let exportError = $state("");
  let exportSuccess = $state("");
  let exportProgress = $state<ExportProgress | null>(null);
  let exportStages = $state<string[]>([]);
  let terminalLogEl = $state<HTMLDivElement | null>(null);
  const modSearchPattern = "^$|^.{2,80}$";

  function pushLog(message: string) {
    gameLogs = [...gameLogs, message].slice(-160);
  }

  function pushExportStage(stage: string) {
    const trimmed = stage.trim();
    if (!trimmed) return;
    if (exportStages[exportStages.length - 1] === trimmed) return;
    exportStages = [...exportStages, trimmed];
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
      pushExportStage(event.payload.stage);
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
    if (viewMode === "mods" && ws && searchResults.length === 0 && !modSearchLoading && !modSearchError) {
      void runModSearch();
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

  function openJavaModal() {
    if (!fullCfg) return;
    selectedJavaId = fullCfg.java_runtime_id || "";
    javaErr = "";
    showJavaModal = true;
  }

  async function saveJavaSelection() {
    if (!ws || !fullCfg || javaSaving) return;
    javaSaving = true;
    javaErr = "";
    try {
      const nextCfg = {
        ...fullCfg,
        java_runtime_id: selectedJavaId || null,
      };
      await invoke("save_pack_config", { id: ws.id, config: nextCfg });
      fullCfg = nextCfg as PackConfig;
      showJavaModal = false;
      pushLog(`[info] Java 运行时已切换为 ${currentJavaLabel()}`);
    } catch (e: any) {
      javaErr = String(e);
    } finally {
      javaSaving = false;
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

  async function addMod(hit: ModrinthProjectHit) {
    if (!ws?.id || installingProjectId) return;
    installingProjectId = hit.project_id;
    modSearchError = "";
    try {
      const installed = await invoke<WorkspaceMod>("install_modrinth_mod", {
        workspaceId: ws.id,
        projectId: hit.project_id,
      });
      const nextMods = [...(fullCfg?.mods || []).filter((item) => item.project_id !== installed.project_id), installed];
      if (fullCfg) {
        fullCfg = { ...fullCfg, mods: nextMods };
      }
      ws = { ...ws, mod_count: nextMods.length };
      workspaces.update((list) => list.map((item) => item.id === ws?.id ? { ...item, mod_count: nextMods.length } : item));
      pushLog(`[mod] 已添加 ${hit.title}`);
    } catch (e: any) {
      modSearchError = String(e);
      pushLog(`[error] ${String(e)}`);
    } finally {
      installingProjectId = "";
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
    exportStages = [];
    pushExportStage("等待导出开始");
    try {
      const result = await invoke<{ export_dir: string }>("export_workspace", {
        request: {
          workspace_id: ws.id,
          export_kind: exportKind,
          include_java: includeJava,
        },
      });
      exportSuccess = `导出完成：${result.export_dir}`;
      pushExportStage("导出完成");
      pushLog(`[export] ${exportSuccess}`);
    } catch (e: any) {
      exportError = String(e);
      pushExportStage("导出失败");
      pushLog(`[error] ${exportError}`);
    } finally {
      exporting = false;
    }
  }

  function exportStepClass(index: number) {
    const lastIndex = exportStages.length - 1;
    if (index < lastIndex) return "step-success";
    if (exportError && index === lastIndex) return "step-error";
    if (exportSuccess && index === lastIndex) return "step-success";
    return "step-primary";
  }

  function editorTitle() {
    if (viewMode === "mods") return "Mods";
    if (viewMode === "export") return "Export";
    return "Explorer";
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
          <div class="sidebar__eyebrow">{viewMode === "mods" ? "mod" : "explorer"}</div>
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

        <button class="ghost-button" onclick={openJavaModal}>配置 Java</button>

        <div class="panel-heading">Description</div>
        <textarea class="textarea textarea-bordered sidebar-description-box" readonly>{ws.description || "当前工作区还没有描述。"}</textarea>

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
      <div class="editor-tabs">
        <div class="editor-tab">{editorTitle()}</div>
      </div>

      {#if viewMode === "mods"}
        <div class="mod-editor">
          <div class="mod-editor__toolbar">
            <input
              class="input input-bordered validator mod-search-input"
              type="text"
              pattern={modSearchPattern}
              title="留空显示热门模组，输入时至少 2 个字符"
              placeholder="搜索 Modrinth 模组，留空显示热门模组"
              bind:value={searchQuery}
              onkeydown={(e) => e.key === "Enter" && runModSearch()}
            />
            <button class="primary-button" onclick={runModSearch} disabled={modSearchLoading}>
              {modSearchLoading ? "加载中..." : "搜索"}
            </button>
          </div>
          {#if modSearchError}
            <div class="mod-inline-error">{modSearchError}</div>
          {/if}

          <div class="mod-list">
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
              <ul class="list bg-base-200 mod-list-frame">
                {#each searchResults as result}
                  <li class="list-row mod-list-row">
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
                      <div class="mod-list-item__slug">{result.slug}</div>
                      <p class="mod-list-item__desc">{result.description}</p>
                    </div>
                    <div class="mod-list-item__actions">
                      <button
                        class="primary-button"
                        onclick={() => addMod(result)}
                        disabled={installingProjectId === result.project_id || installedIds.has(result.project_id)}
                      >
                        {installedIds.has(result.project_id) ? "已安装" : installingProjectId === result.project_id ? "安装中..." : "安装"}
                      </button>
                    </div>
                  </li>
                {/each}
              </ul>
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

            {#if exportStages.length > 0}
              <ul class="steps steps-vertical export-steps">
                {#each exportStages as stage, index}
                  <li class={`step ${exportStepClass(index)}`}>{stage}</li>
                {/each}
              </ul>
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

{#if showJavaModal}
  <div
    class="overlay"
    role="button"
    tabindex="0"
    aria-label="关闭 Java 配置"
    onclick={() => (showJavaModal = false)}
    onkeydown={(e) => (e.key === "Enter" || e.key === "Escape") && (showJavaModal = false)}
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
          <h2 class="panel__title">配置 Java</h2>
          <div class="panel__subtitle">选择启动当前工作区时使用的 Java 运行时</div>
        </div>
        <button class="icon-button" onclick={() => (showJavaModal = false)} aria-label="关闭">×</button>
      </div>
      <div class="panel__body">
        <div class="field">
          <label for="java-select">Java Runtime</label>
          <select id="java-select" bind:value={selectedJavaId}>
            <option value="">默认（系统 Java）</option>
            {#each javaList as j}
              <option value={j.id}>{j.name} - {j.major_version ? `Java ${j.major_version}` : j.version_text}</option>
            {/each}
          </select>
          {#if javaList.length === 0}
            <div class="inline-message">还没有可用 Java，请先到 Java 管理页面添加。</div>
          {/if}
          {#if javaErr}
            <div class="inline-message" style="color: var(--vscode-danger);">{javaErr}</div>
          {/if}
        </div>
      </div>
      <div class="panel__footer">
        <button class="secondary-button" onclick={() => (showJavaModal = false)}>取消</button>
        <button class="primary-button" onclick={saveJavaSelection} disabled={javaSaving}>
          {javaSaving ? "保存中..." : "保存"}
        </button>
      </div>
    </div>
  </div>
{/if}
