# PROJECT_LIST

本文件用于记录当前项目中保留文件的职责分工，便于后续继续清理、交接和扩展。

说明：
- 以当前仓库中的正式工程文件为主。
- `README_AI.md`、`.reasonix/`、`.codex` 这类本地协作文件也单独标注，因为它们会影响 AI 接手方式，但不参与运行时逻辑。
- 已删除的构建产物和临时文件放在文末，避免和正式源码混淆。

## 根目录

- `.gitignore`：根级忽略规则，负责过滤 Rust、SvelteKit、构建输出、统计文件和临时脚本。
- `Cargo.toml`：Rust 工作区入口，声明 `mc_launcher_core` 与 `src-tauri` 两个成员 crate。
- `Cargo.lock`：Rust 工作区锁定依赖版本，保证构建结果可复现。
- `README.md`：项目基础说明，目前仍保留 Tauri + SvelteKit 模板级介绍。
- `README_AI.md`：本地 AI 协作说明，定义项目目标、存储约定、当前任务进度和提交要求。
- `package.json`：前端工程入口配置，声明 SvelteKit/Tauri 前端依赖与开发脚本。
- `package-lock.json`：Node 依赖锁文件，固定前端依赖版本。
- `postcss.config.js`：PostCSS 配置，启用 Tailwind CSS 与 Autoprefixer。
- `svelte.config.js`：SvelteKit 配置，使用 `adapter-static` 并开启 SPA 回退。
- `tailwind.config.js`：Tailwind 与 DaisyUI 主题配置，控制扫描范围和可用主题。
- `tsconfig.json`：前端 TypeScript 配置，继承 `.svelte-kit` 生成的 tsconfig。
- `vite.config.js`：Vite 开发与构建配置，适配 Tauri 的固定端口、HMR 和文件监听策略。
- `PROJECT_LIST.md`：项目文件职责总表，记录各文件用途与清理结论。

## 前端入口与全局资源

- `src/app.html`：SvelteKit HTML 外壳，定义页面语言、标题、图标和挂载点。
- `src/app.css`：全局样式入口，仅注入 Tailwind 的 base/components/utilities。

## 前端路由

- `src/routes/+layout.svelte`：全局布局页，提供抽屉式侧边栏、导航入口和主题切换。
- `src/routes/+layout.ts`：关闭 SSR，确保 Tauri 前端以 SPA 模式运行。
- `src/routes/+page.svelte`：工作区首页，负责加载、创建、删除工作区并跳转详情页。
- `src/routes/java/+page.svelte`：Java 管理页面，负责检测、添加、删除 Java 运行时。
- `src/routes/settings/+page.svelte`：设置页面，负责读取和保存全局下载池上限。
- `src/routes/workspace/[id]/+page.svelte`：单个工作区详情页，负责标签切换、下载进度、启动日志和 Java 选择。

## 前端组件

- `src/lib/components/OverviewTab.svelte`：工作区概览标签页，展示启动按钮、运行状态、Java 入口与基础统计信息。
- `src/lib/components/ConfigTab.svelte`：工作区配置标签页，编辑整合包名称、MC 版本、加载器、内存、分辨率和 JVM 参数。
- `src/lib/components/ModsTab.svelte`：模组标签页，使用 mock 数据演示模组的添加与移除流程。

## 前端状态与类型

- `src/lib/stores/workspace.ts`：工作区与启动状态的 Svelte store，封装工作区增删改查调用。
- `src/lib/types/index.ts`：前后端共用的前端类型定义，包括工作区、配置、Java、设置和启动状态。
- `src/lib/mock/data.ts`：当前模组页使用的 mock 数据与示例工作区配置。

## Tauri 后端根配置

- `src-tauri/.gitignore`：Tauri 子工程忽略规则，主要过滤其独立的 `target/` 输出。
- `src-tauri/Cargo.toml`：Tauri 应用 crate 配置，声明桌面端依赖和本地 `mc_launcher_core` 依赖。
- `src-tauri/Cargo.lock`：Tauri crate 自身依赖锁文件。
- `src-tauri/build.rs`：Tauri 构建脚本，交由 `tauri-build` 生成平台相关构建信息。
- `src-tauri/tauri.conf.json`：Tauri 应用清单，定义窗口、打包、前端资源等桌面参数。
- `src-tauri/capabilities/default.json`：Tauri 能力声明文件，定义应用默认权限边界。

## Tauri 应用入口

- `src-tauri/src/main.rs`：桌面程序真实入口，调用库 crate 的 `run()` 启动应用。
- `src-tauri/src/lib.rs`：Tauri 应用装配层，注册插件与所有 `tauri::command` 命令。
- `src-tauri/src/commands/mod.rs`：命令模块汇总入口，统一导出工作区、下载、启动、Java、设置命令。

## Tauri 命令模块

- `src-tauri/src/commands/workspace.rs`：工作区 CRUD 核心，读写 `.MMPC/workspaces/<id>/pack.json`。
- `src-tauri/src/commands/settings.rs`：全局设置读写逻辑，当前负责下载池大小的默认值、校验和持久化。
- `src-tauri/src/commands/java.rs`：Java 运行时管理逻辑，负责版本探测、保存列表和按 id 解析路径。
- `src-tauri/src/commands/download.rs`：Minecraft 运行时下载与校验逻辑，负责版本清单、资源、库、多线程下载池和进度事件。
- `src-tauri/src/commands/launch.rs`：游戏启动逻辑，负责校验工作区运行时、组装 classpath、准备 natives、写入 argfile 并启动进程。

## mc_launcher_core 启动核心库

- `mc_launcher_core/Cargo.toml`：核心启动库配置，声明认证、UUID、错误处理等依赖。
- `mc_launcher_core/src/lib.rs`：核心库总入口，导出认证与启动模块。
- `mc_launcher_core/src/auth/mod.rs`：认证抽象层，定义认证接口、可序列化用户接口与认证错误。
- `mc_launcher_core/src/auth/offline.rs`：离线用户实现，负责生成离线 UUID 与离线认证用户对象。
- `mc_launcher_core/src/launch/mod.rs`：启动模块公共入口，定义通用启动错误和模块边界。
- `mc_launcher_core/src/launch/offline.rs`：离线启动命令构造器，负责 `LaunchConfig`、命令拼装和版本参数解析接入。
- `mc_launcher_core/src/launch/version.rs`：`version.json` 元数据解析与合并逻辑，负责新旧参数规则、条件参数和占位符替换。

## 本地协作与辅助元数据

- `.codex`：本地 Codex 协作标记文件，不参与运行时逻辑。
- `.reasonix/semantic/index.jsonl`：Reasonix 的语义索引数据，用于本地辅助检索。
- `.reasonix/semantic/index.meta.json`：Reasonix 语义索引的元信息文件。
- `.reasonix/skills/svelte-code-writer/SKILL.md`：本地 AI 的 Svelte 写作技能说明。
- `.reasonix/skills/svelte-core-bestpractices/SKILL.md`：本地 AI 的 Svelte 核心最佳实践说明。
- `.reasonix/skills/svelte-core-bestpractices/references/$inspect.md`：Svelte `$inspect` 参考资料。
- `.reasonix/skills/svelte-core-bestpractices/references/@attach.md`：Svelte `@attach` 指令参考资料。
- `.reasonix/skills/svelte-core-bestpractices/references/@render.md`：Svelte `@render` 语法参考资料。
- `.reasonix/skills/svelte-core-bestpractices/references/await-expressions.md`：Svelte 异步表达式参考资料。
- `.reasonix/skills/svelte-core-bestpractices/references/bind.md`：Svelte `bind` 绑定语法参考资料。
- `.reasonix/skills/svelte-core-bestpractices/references/each.md`：Svelte `each` 列表渲染参考资料。
- `.reasonix/skills/svelte-core-bestpractices/references/hydratable.md`：Svelte 可水合能力相关参考资料。
- `.reasonix/skills/svelte-core-bestpractices/references/snippet.md`：Svelte `snippet` 片段能力参考资料。
- `.reasonix/skills/svelte-core-bestpractices/references/svelte-reactivity.md`：Svelte 响应式机制参考资料。

## 已清理的生成物与临时文件

以下内容已从工作区中清理，不再视为正式项目文件：

- `.svelte-kit/`：SvelteKit 生成缓存与中间产物。
- `build/`：前端构建输出目录。
- `.VSCodeCounter/`：VS Code Counter 统计输出结果。
- `tmp-fix2.cjs`：一次性修补脚本。
- `tmp-write-overview.js`：一次性文档/页面写入脚本。
- `src-tauri/target/`：Tauri 子工程编译输出目录。

以下内容属于本地编译缓存，当前未纳入正式文件清单：

- `target/`：Rust 工作区根级编译输出目录。
- `node_modules/`：Node 依赖安装目录。
