import type {
  Workspace,
  Mod,
  PackConfig,
} from "$lib/types";

// ─── Mock mods ───

export const mockMods: Mod[] = [
  {
    id: "sodium",
    name: "Sodium",
    version: "0.6.0",
    mc_version: "1.21",
    description: "Modern rendering engine for Minecraft,大幅提升 FPS",
    url: "https://modrinth.com/mod/sodium",
    file_size: 2_456_000,
  },
  {
    id: "lithium",
    name: "Lithium",
    version: "0.13.1",
    mc_version: "1.21",
    description: "General-purpose optimisation mod — 优化服务器与客户端性能",
    url: "https://modrinth.com/mod/lithium",
    file_size: 890_000,
  },
  {
    id: "jei",
    name: "Just Enough Items",
    version: "19.0.0",
    mc_version: "1.21",
    description: "View item recipes and usages in-game",
    url: "https://modrinth.com/mod/jei",
    file_size: 3_200_000,
  },
  {
    id: "create",
    name: "Create",
    version: "6.0.0",
    mc_version: "1.21",
    description: "Aesthetic technology mod — 机械动力模组",
    url: "https://modrinth.com/mod/create",
    file_size: 18_500_000,
  },
  {
    id: "iris",
    name: "Iris",
    version: "1.8.0",
    mc_version: "1.21",
    description: "Shader loader — 光影加载器",
    url: "https://modrinth.com/mod/iris",
    file_size: 1_200_000,
  },
];

// ─── Mock pack config ───

export function mockPackConfig(overrides?: Partial<PackConfig>): PackConfig {
  return {
    name: "我的整合包",
    description: "一个高性能 Minecraft 整合包，包含优化与内容模组",
    mc_version: "1.21",
    mods: ["sodium", "lithium", "jei"],
    jvm_args: ["-XX:+UseG1GC", "-Dsun.rmi.dgc.server.gcInterval=2147483646"],
    min_memory_mb: 1024,
    max_memory_mb: 4096,
    window_width: 1280,
    window_height: 720,
    ...overrides,
  };
}

// ─── Mock workspaces ───

export function mockWorkspaces(): Workspace[] {
  return [
    {
      id: "my-performance-pack",
      name: "性能优化包",
      mc_version: "1.21",
      description: "使用 Sodium + Lithium 大幅提升帧率，适合低配电脑",
      mod_count: 2,
      path: ".MMCP/workspaces/my-performance-pack",
      config: mockPackConfig({
        name: "性能优化包",
        description: "使用 Sodium + Lithium 大幅提升帧率，适合低配电脑",
        mods: ["sodium", "lithium"],
      }),
      last_opened: "2024-12-28T10:30:00Z",
      created_at: "2024-12-20T08:00:00Z",
    },
    {
      id: "create-adventure",
      name: "机械冒险",
      mc_version: "1.20.1",
      description: "以 Create 模组为核心的机械 + 探索整合包",
      mod_count: 2,
      path: ".MMCP/workspaces/create-adventure",
      config: mockPackConfig({
        name: "机械冒险",
        description: "以 Create 模组为核心的机械 + 探索整合包",
        mc_version: "1.20.1",
        mods: ["create", "jei"],
      }),
      last_opened: "2024-12-27T15:00:00Z",
      created_at: "2024-12-15T12:00:00Z",
    },
    {
      id: "shader-showcase",
      name: "光影展示",
      mc_version: "1.21",
      description: "Iris 光影展示包，体验顶级视觉特效",
      mod_count: 2,
      path: ".MMCP/workspaces/shader-showcase",
      config: mockPackConfig({
        name: "光影展示",
        description: "Iris 光影展示包，体验顶级视觉特效",
        mods: ["iris", "sodium"],
        max_memory_mb: 8192,
      }),
      last_opened: "2024-12-25T09:00:00Z",
      created_at: "2024-12-10T16:00:00Z",
    },
  ];
}
