<script lang="ts">
  import type { Workspace } from "$lib/types";

  let { workspace, onsave }: { workspace: Workspace; onsave?: (patch: Partial<Workspace['config']>) => void } = $props();

  let saved = $state(false);

  function handleSave() {
    onsave?.({ ...workspace.config });
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
        bind:value={workspace.config.name}
      />
    </div>

    <!-- MC version -->
    <div>
      <label class="label" for="cfg-mcver">
        <span class="label-text">Minecraft 版本</span>
      </label>
      <select
        id="cfg-mcver"
        class="select select-bordered w-full"
        bind:value={workspace.config.mc_version}
      >
        <option>1.21</option>
        <option>1.20.4</option>
        <option>1.20.1</option>
        <option>1.19.4</option>
        <option>1.18.2</option>
        <option>1.16.5</option>
        <option>1.12.2</option>
      </select>
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
        bind:value={workspace.config.description}
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
          bind:value={workspace.config.min_memory_mb}
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
          bind:value={workspace.config.max_memory_mb}
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
          bind:value={workspace.config.window_width}
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
          bind:value={workspace.config.window_height}
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
        value={workspace.config.jvm_args.join("\n")}
        oninput={(e) => {
          const val = (e.target as HTMLTextAreaElement).value;
          workspace.config.jvm_args = val ? val.split("\n").filter((s) => s.trim()) : [];
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
