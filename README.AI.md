# MMPC AI Handoff

该文件用于 AI / 代理接手任务时快速理解项目当前状态。

## 项目概况

- 项目名称：`MMPC`
- 目标：Minecraft 整合包制作器
- 技术栈：`Svelte + Tauri + Rust + Tailwind/Shadcn`
- 当前实际数据目录：`.MMPC`
- 注意：旧文档里曾写成 `.MMCP`，实际代码已统一使用 `.MMPC`

## 当前实现概览

- 工作区目录：`.MMPC/workspaces/<workspace_id>`
- 全局资源目录：
  - `.MMPC/assets`
  - `.MMPC/cache/installers`
  - `.MMPC/modcache`
  - `.MMPC/tmp`
  - `.MMPC/settings.json`
  - `.MMPC/mods.json`
- 启动/下载核心：
  - `mc_launcher_core/src/runtime/*`
  - `src-tauri/src/commands/download.rs`
  - `src-tauri/src/commands/launch.rs`
- 模组管理：
  - `src-tauri/src/commands/mods.rs`
  - `src/lib/components/ModsTab.svelte`
- 导出：
  - `src-tauri/src/commands/export.rs`
  - `src/routes/export/+page.svelte`

## 已完成事项

### 基础能力

- `task1` 完成：构建 `mc_launcher_core`，支持基础启动与离线用户。
- `task2` 完成：前端基础界面已构建。
- `task3` 完成：真实数据接入，能够创建工作区并成功启动原版游戏。

### 工作区与设置

- 工作区支持保存：
  - `mc_version`
  - `loader_type`
  - `loader_version`
  - `java_runtime_id`
  - 内存上下限
  - 窗口宽高
  - `jvm_args`
- 主题设置已持久化到 `.MMPC/settings.json`
- 下载线程池可配置，默认 `16`

### 版本与运行时

- 已支持 loader：
  - `vanilla`
  - `fabric`
  - `forge`
  - `neoforge`
- 已实现每个工作区内缓存 `version.json`
- 版本列表能力：
  - Minecraft release 版本列表
  - Fabric / Forge / NeoForge loader 版本列表
- 下载源优先使用 `BMCLAPI`，失败时回退官方源

### 启动与运行

- Fabric 已可正常启动
- NeoForge 已可正常启动
- Forge 在工作区内启动链路已基本打通
- 启动状态、日志、进度展示链路已接入前端

### 模组管理

- Modrinth 搜索与下载已接入
- 模组文件统一缓存到 `.MMPC/modcache`
- 工作区 `mods` 目录采用链接/同步逻辑
- 已建立 `.MMPC/mods.json` 模组表
- 模组类型支持：
  - `client_only`
  - `server_only`
  - `client_and_server`
  - `development_only`
  - `unknown`
- 前端已支持手动切换模组类型
- 启动时不会把 `server_only` 模组链接进客户端 `mods`

### 导出功能

- 已新增导出页：`/export`
- 支持导出方式：
  - `client`
  - `server`
  - `full`
- 已支持导出进度事件：`export-progress`
- 导出页已展示实时进度消息框
- 已支持可选导出 Java
- 服务端导出已分流处理：
  - Vanilla server
  - Fabric server
  - Forge server
  - NeoForge server

## 用户已验证通过

- 原版服务端导出：通过
- Fabric 服务端导出：通过

## 当前判断与重要结论

### 关于 Fabric 客户端

- 当前没有证据表明“这轮修改把 Fabric 客户端导出搞坏了”
- 现有日志显示 Fabric 客户端实际上能正常进入游戏
- 之前提到的 `No dependencies to load found. Skipping!` 并不是 Fabric 报错

### 关于 Forge 的 `No dependencies to load found. Skipping!`

- 该日志来自 Forge 的 `JarInJarDependencyLocator`
- 日志级别是 `INFO`
- 这是“未发现 jar-in-jar 依赖，跳过”语义
- 不能单独把这条日志视为启动失败原因

### 关于 Linux narrator / flite

- Vanilla / Fabric / NeoForge 客户端日志中均可见：
  - `Failed to load library flite`
  - `libflite.so` 缺失
- 这是旁白库缺失，不等于游戏无法启动
- 现有日志里游戏通常仍能继续进入主界面/世界

## 当前未完成事项

### 1. 导出链路的最终验收仍不完整

- 虽然导出逻辑已经完成分流，但以下链路仍缺“最终人工验收”：
  - Fabric 客户端导出
  - Forge 客户端导出
  - Forge 服务端导出
  - NeoForge 客户端导出
  - NeoForge 服务端导出

### 2. Forge / NeoForge 导出后的自检还没做

- 目前导出完成后没有自动检查以下关键文件是否齐全：
  - `run.sh`
  - `run.bat`
  - `user_jvm_args.txt`
  - `unix_args.txt` / `win_args.txt`
  - 关键 `libraries`
- 建议补一套导出后自检逻辑，避免把普通日志误认为失败

### 3. 导出元信息未落盘

- 建议在导出目录中生成 `export-meta.json`
- 建议记录：
  - `workspace_id`
  - `mc_version`
  - `loader_type`
  - `loader_version`
  - `export_kind`
  - `include_java`
  - 导出时间

### 4. 旧文档与新文档并存风险

- 仓库原本只有 `README_AI.md`
- 本次按用户要求补充 `README.AI.md`
- 后续建议统一只保留一个 AI 交接文档，避免分叉

## 最近一轮已修改的重点文件

- `src-tauri/src/commands/export.rs`
- `src/routes/export/+page.svelte`
- `src-tauri/src/commands/workspace.rs`
- `src/lib/types/index.ts`

## 最近一轮实际完成的修正

- 导出页 `invoke("export_workspace")` 参数已按 snake_case 使用：
  - `workspace_id`
  - `export_kind`
  - `include_java`
- 导出页新增进度监听和消息框
- 导出逻辑已拆分客户端/服务端路径
- 服务端不再错误复用客户端 `java.args`
- 工作区列表现在返回并显示 `loader_type` / `loader_version`
- 导出页工作区下拉现在会显示 loader，降低误测概率

## 最近一轮已执行检查

- `cargo test --manifest-path src-tauri/Cargo.toml`
- `npm run check`

两者通过。

## 下一位接手建议顺序

1. 先用导出页分别导出 Fabric / Forge / NeoForge
2. 确认选中的工作区 loader 是否正确
3. 先看导出目录结构，再运行脚本
4. 如果 Forge / NeoForge 再报问题，优先区分：
   - 普通 `INFO` 日志
   - 真正的异常堆栈
5. 完成导出后自检与 `export-meta.json`

