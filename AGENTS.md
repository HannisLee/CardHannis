# AGENTS.md

本文件是 CardHannis 项目的开发协作说明，面向自动化代理和维护者。规则适用于仓库根目录及其子目录；当前未发现更深层的 `AGENTS.md`、`CONTRIBUTING` 或格式化配置文件。

## 项目概览

CardHannis 是一个本地优先的任务管理工具，核心能力包括：

- 任务生命周期：待处理（`pending`）、进行中（`in_progress`）、已完成（`completed`）。
- 任务阻塞记录：一个任务可以有多段历史阻塞，但同时最多一个未结束阻塞。
- 工作会话记录：一个任务同时最多一个未结束工作会话。
- SQLite 持久化、内置迁移、软删除和基于 `version` 的乐观并发控制。
- 共享业务边界：`cardhannis-core` 的 `TaskService` 供桌面端、CLI 等适配层使用。

项目目前包含两层/两种运行形态：

1. **Rust 核心库**：`core/`，领域模型、业务服务和 SQLite 存储。
2. **Tauri 2 桌面端**：`src-tauri/` + `ui/`，面向最终桌面应用。

根目录没有检测到 Git 元数据（执行 `git status` 会提示不是 Git 仓库），因此不要假设当前目录可用分支、提交或 Git hook 工作流。

## 目录结构

```text
.
├── Cargo.toml              # Rust workspace，成员为 core 和 src-tauri
├── package.json            # 根级 npm 脚本和 Tauri CLI
├── README.md               # Rust 核心库说明与验证命令
├── core/
│   ├── migrations/         # SQLite 迁移；当前为 0007_workspace_scoped_priorities.sql
│   └── src/
│       ├── application.rs  # TaskService、命令 DTO
│       ├── domain.rs       # Task、TaskBlock、WorkSession 等类型
│       ├── error.rs        # CoreError 和 Result
│       ├── persistence.rs  # TaskStore、SQL 和状态约束
│       └── lib.rs          # 对外导出与核心集成测试
├── src-tauri/
│   ├── src/lib.rs          # AppState、Tauri commands、数据库初始化
│   ├── src/main.rs         # 桌面入口
│   └── tauri.conf.json     # Tauri 2 构建、窗口和打包配置
└── ui/
    ├── src/main.js         # 原生 DOM UI、Tauri invoke、浏览器预览回退
    ├── src/style.css       # 视觉样式
    └── dist/               # Vite 构建产物，不要直接编辑
```

`target/`、`node_modules/`、`ui/node_modules/` 属于生成/环境目录，不应手工修改或作为源码依据。

## 架构与边界

### Rust 核心（`core/`）

- 新的业务规则优先放在 `TaskService` / `TaskStore`，不要在 Tauri command 或前端复制业务逻辑。
- `TaskService` 是推荐的稳定业务门面；适配层只负责请求参数转换、调用服务和错误映射。
- `TaskStore` 内部使用 `Mutex<rusqlite::Connection>`，初始化时开启外键并执行 `core/migrations/0001_initial.sql`。
- 领域结构体实现 `Serialize` / `Deserialize`，对外 JSON 字段使用 Rust 当前命名（如 `estimated_active_minutes`、`due_date`、`resolution_reason`）。
- 时间统一使用 UTC RFC3339 毫秒字符串，常规生成方式是 `Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)`。
- ID 使用 UUID v4 字符串。

### Tauri 桌面端（`src-tauri/` + `ui/`）

- `src-tauri/src/lib.rs` 的 `shared_data_dir()` 用跨平台规则解析数据目录（macOS `~/Library/Application Support/CardHannis`、Windows `%APPDATA%/CardHannis`、Linux `~/.local/share/CardHannis`，兜底 Tauri `app_data_dir`）；SQLite 文件平台无关，可整文件跨机器/跨系统无损迁移。
- AppState 以 `Mutex<TaskService>` 注入 Tauri state；command 返回可序列化领域对象，错误当前转换为字符串。
- `src-tauri/tauri.conf.json` 中 `beforeDevCommand`/`beforeBuildCommand` 分别调用根级 `npm run ui:dev`/`npm run ui:build`，前端产物目录为 `ui/dist`。
- `ui/src/main.js` 使用 `@tauri-apps/api/core` 的 `invoke`。不在 Tauri 中运行时，会进入浏览器预览模式（内存 state），只适合 UI 快速预览，不提供持久化。
- 前端当前为 Vite + 原生 JavaScript/DOM，没有 React/Vue 等框架；沿用现有事件绑定和 `escapeHtml` 防注入方式。
- `ui/dist` 是构建输出，应通过 `npm run ui:build` 生成，不直接改动其中的压缩文件。

### Supabase（已退役）

- Python 原型（`web/`）及其 Supabase 同步已于 2026-09-04 删除；桌面端为唯一运行形态。
- `supabase-schema.sql` 保留为远端 schema 预留件，将来若在 Rust 侧重实现同步时启用；届时必须接入 Supabase Auth 与按用户 RLS，不能沿用旧的公开读写策略。

- 桌面端内置 Web 设置控制台：默认关闭，仅监听 `127.0.0.1:1421`，设置页点击「前往」后启动并打开浏览器，5 分钟无 HTTP 操作自动关闭；Web 端不提供任务基础操作。
- 桌面端以 macOS 菜单栏常驻图标运行，不再显示 Dock 图标；左键菜单栏图标可显示/隐藏主窗口，右键菜单可退出。
- Windows 端使用系统托盘常驻图标；关闭按钮隐藏主窗口，应用仍保留在托盘。
- 桌面端（`ui/src`）当前为 340×400 置顶便签小窗：横向工作区标签 + 纵向可收起分级 + 单行条目（标题+状态/元信息+行内按钮）；已完成任务归档到内置「已完成」工作区；标题栏始终不透明，下面的内容区支持失焦透明度和字号微调，透明度为 0 时须从标题栏唤醒；自绘拖拽；应用内弹窗（webview 无原生 prompt/confirm）。

## 领域不变量

修改任务、阻塞或工作会话逻辑时，必须保持以下约束：

- 任务标题、阻塞原因和设备 ID 不能是空白字符串。
- `estimated_active_minutes` 只能为 `NULL` 或非负整数。
- `due_date` 只能为 `NULL` 或 `YYYY-MM-DD` 格式日期。
- 完成任务必须有 `completed_at`；未完成任务不能有 `completed_at`。
- `completed_at` 不能早于 `started_at`。
- 已删除任务不能继续正常更新、开始工作或创建阻塞。
- 已完成任务不能开始工作或创建阻塞。
- 任务被阻塞时不能开始新的工作；开始阻塞会结束该任务当前未结束的工作会话。
- 任务从进行中转为待处理或完成时，核心层会结束当前活动工作会话，保证累计活动时间落库。
- 同一任务最多一个活动阻塞（由 SQLite 部分唯一索引 `ux_task_blocks_one_active` 保证）。
- 同一任务最多一个活动工作会话（由 `ux_work_sessions_one_active` 保证）；`task_blocks.resolution_reason` 保存可选解除阻塞原因。
- 更新任务、删除任务、完成任务、结束阻塞等并发敏感操作必须校验 `expected_version`；冲突返回 `VersionConflict`。
- 迁移 SQL 是跨实现共享契约。新增迁移时要考虑已有数据库升级路径，不要只修改当前建表 SQL 而破坏现有数据库。
- 迁移通过 `schema_migrations` 表记录执行进度，Rust 与 Python 两端都按文件名顺序执行 `core/migrations/*.sql`；新增迁移直接加文件，不要改历史文件。
- 任务状态共四态：`pending` / `in_progress` / `waiting`（等待中，解除阻塞后的默认落点）/ `completed`；阻塞不是状态，由未结束的阻塞记录派生。
- 已完成任务可通过 `reopen`（`TaskService::reopen` / 桌面 `reopen_task` 命令）回到 `pending`。
- `workspaces`、`priorities` 是用户可管理实体（增/改名/软删）；每个分级只属于一个工作区，新建工作区自动创建 P0/P1/P2。删除前提：工作区无任务、分级无任务且该工作区至少保留一个分级；任务所选分级必须属于其工作区。任务的 `workspace_id`/`priority_id` 可为空（旧数据由迁移回填并按工作区拆分）。

## 常用命令

在项目根目录执行：

```bash
# Rust 格式、测试和检查
cargo fmt --all -- --check
cargo test --workspace
cargo check --workspace

# 前端依赖和构建
npm install
npm run ui:dev       # Vite，固定 http://127.0.0.1:1420（ui/vite.config.js 中与 Tauri devUrl 一致）
npm run ui:build

# Tauri 桌面开发/打包
npm run tauri:dev
npm run tauri:build
```

## 修改与验证规范

1. **先判断修改层次**：业务规则改 `core/`；桌面桥接改 `src-tauri/`；桌面 UI 改 `ui/`。不要在适配层复制业务逻辑。
2. **优先复用核心服务**：新桌面 command 应调用 `TaskService`，而不是直接拼接 SQL。
3. **数据库变更**：修改 `core/migrations/` 后，同时检查 Rust `include_str!` 和 Python `read_text()` 的路径、字段顺序、索引、约束；必要时增加迁移文件或测试。
4. **并发操作**：任何带版本的更新都必须使用调用方传入的旧版本，成功后由存储层递增 `version`；不要在前端静默覆盖版本冲突。
5. **错误处理**：核心层使用 `CoreError`，不要吞掉错误；适配层再将其转成 Tauri `String`。新增错误尽量复用现有语义。
6. **前端输入**：动态插入 HTML 时继续转义用户可控文本；不要把标题、备注、阻塞原因直接拼入未转义的 HTML。
7. **风格**：Rust 遵循 `rustfmt`；Python 保持类型注解和现有模块结构；前端保持 ES modules、原生 DOM 和当前无框架实现。避免为了小改动引入新框架或大规模重排。
8. **生成文件**：不要手工编辑 `target/`、`ui/dist/`、`node_modules/` 或 Tauri 生成 schema；通过对应构建命令重新生成。
9. **文档同步**：启动方式、数据库路径、命令发生变化时，更新根目录 `README.md` 和本文件。

## 测试重点

当前自动化测试主要位于 `core/src/lib.rs` 的 Rust 集成测试，覆盖：

- 重复阻塞历史能够保留；
- 版本号并发控制和任务生命周期；
- 开始阻塞会结束活动工作会话。

改动核心业务时，优先补充这些测试或在相邻模块增加单元测试。至少运行：

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo check --workspace
npm run ui:build
```

若新增测试框架或命令，请把命令写入对应 README 和本文件。

## 提交前检查清单

- [ ] 修改范围与对应层次一致，没有把业务规则散落到 UI。
- [ ] `cargo fmt --all -- --check` 通过。
- [ ] `cargo test --workspace` 通过。
- [ ] `cargo check --workspace` 通过。
- [ ] `npm run ui:build` 通过，且未直接编辑 `ui/dist`。
- [ ] 数据库字段、迁移、序列化模型和 API 请求/响应已同步。
- [ ] 版本冲突、软删除、阻塞和活动工作会话等边界行为已考虑。
- [ ] 用户可控文本仍经过安全转义/验证。
- [ ] 相关 README/文档已更新。
