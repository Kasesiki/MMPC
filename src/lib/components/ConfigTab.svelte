<script lang="ts">
  import type { PackConfig, Workspace } from "$lib/types";

  let {
    workspace,
    config,
    releaseVersions = [],
    onsave
  }: {
    workspace: Workspace;
    config: PackConfig;
    releaseVersions?: string[];
    onsave?: (patch: PackConfig) => void;
  } = $props();

  let saved = $state(false);

  function normalizedConfig(): PackConfig {
    return {
      ...config,
      min_memory_mb: Math.max(256, Number(config.min_memory_mb) || 1024),
      max_memory_mb: Math.max(Number(config.min_memory_mb) || 1024, Number(config.max_memory_mb) || 4096),
      window_width: Math.max(640, Number(config.window_width) || 1280),
      window_height: Math.max(480, Number(config.window_height) || 720)
    };
  }

  function handleSave() {
    onsave?.(normalizedConfig());
    saved = true;
    setTimeout(() => (saved = false), 2000);
  }
</script>

<div class="max-w-2xl">
  <h3 class="text-lg font-semibold mb-4">整合包配置</h3>

  <div class="form-control gap-5">
    <!-- Pack name -->
    <div>
      <label class="label" for="cfg-name">
        <span class="label-text">整合包名称</span>
      </label>
      <input
        id="cfg-name"
        type="text"
        class="input input-bordered w-full"
        bind:value={config.name}
      />
    </div>

    <!-- Description -->
    <div>
      <label class="label" for="cfg-desc">
        <span class="label-text">描述</span>
      </label>
      <textarea
        id="cfg-desc"
        class="textarea textarea-bordered w-full"
        rows="3"
        placeholder="整合包描述..."
        bind:value={config.description}
      ></textarea>
    </div>

    <!-- Memory -->
    <div class="grid grid-cols-2 gap-4">
      <div>
        <label class="label" for="cfg-minmem">
          <span class="label-text">最小内存 (MB)</span>
        </label>
        <input
          id="cfg-minmem"
          type="number"
          class="input input-bordered w-full"
          min="256"
          bind:value={config.min_memory_mb}
        />
      </div>
      <div>
        <label class="label" for="cfg-maxmem">
          <span class="label-text">最大内存 (MB)</span>
        </label>
        <input
          id="cfg-maxmem"
          type="number"
          class="input input-bordered w-full"
          min={config.min_memory_mb || 256}
          bind:value={config.max_memory_mb}
        />
      </div>
    </div>

    <!-- Resolution -->
    <div class="grid grid-cols-2 gap-4">
      <div>
        <label class="label" for="cfg-width">
          <span class="label-text">窗口宽度</span>
        </label>
        <input
          id="cfg-width"
          type="number"
          class="input input-bordered w-full"
          bind:value={config.window_width}
        />
      </div>
      <div>
        <label class="label" for="cfg-height">
          <span class="label-text">窗口高度</span>
        </label>
        <input
          id="cfg-height"
          type="number"
          class="input input-bordered w-full"
          bind:value={config.window_height}
        />
      </div>
    </div>

    <!-- JVM args -->
    <div>
      <label class="label" for="cfg-jvm">
        <span class="label-text">额外 JVM 参数（每行一个）</span>
      </label>
      <textarea
        id="cfg-jvm"
        class="textarea textarea-bordered w-full font-mono text-sm"
        rows="4"
        placeholder="-XX:+UseG1GC&#10;-Dsun.rmi.dgc.server.gcInterval=2147483646"
        value={config.jvm_args.join("\n")}
        oninput={(e) => {
          const val = (e.target as HTMLTextAreaElement).value;
          config.jvm_args = val ? val.split("\n").filter((s) => s.trim()) : [];
        }}
      ></textarea>
    </div>

    <!-- Save button -->
    <div class="flex items-center gap-3 pt-2">
      <button class="btn btn-primary" onclick={handleSave}>
        {saved ? "✓ 已保存" : "保存配置"}
      </button>
      {#if saved}
        <span class="text-success text-sm">配置已保存</span>
      {/if}
    </div>
  </div>
</div>
