# CardHannis Core

CardHannis Core 是一个跨平台 Rust 核心库，负责任务、重复阻塞记录和工作会话。

## 当前实现

- `cardhannis-core`：可嵌入桌面 GUI、CLI、Tauri 或 HTTP/Web API 适配层。
- SQLite 持久化，内置迁移。
- `task_blocks` 独立保存每次阻塞，支持同一任务多次阻塞；解除原因保存在 `resolution_reason`。
- 任务支持可选 `due_date`（`YYYY-MM-DD`）作为预计完成日期。
- 同一任务最多一个未结束的阻塞和一个未结束的工作会话；任务离开进行中状态时核心层会结束活动会话。
- `TaskService::correct_work_time` 可按目标总分钟数修正历史工作会话；修正会遵守阻塞区间和完成时间边界，不引入独立修正字段。
- SQLite 外键、软删除、版本号乐观并发控制。
- `TaskService` 作为 GUI、CLI 共用的业务门面。
- 分级按工作区独立管理；新建工作区自动带有 P0/P1/P2，任务不能引用其他工作区的分级；只有「已完成」是内置工作区且始终排在最后，普通工作区支持拖动排序。
- 桌面端内置一个默认关闭的 Web 设置控制台，可在设置中点击「前往」打开，5 分钟无操作后自动关闭；其中可配置 Supabase Project URL、publishable/anon key、Auth 邮箱/密码、schema、5 张表名和自动同步间隔，配置保存到项目根目录的 `supabase.local.json`；该文件已被 Git 忽略，可复制到另一台电脑的项目根目录直接复用。
- 桌面端会按配置自动拉取并上传 Supabase 数据，默认每 5 分钟一次；Web 设置页左下角显示数据库连接状态。同步使用 Supabase Auth 登录用户和按 `user_id` 隔离的 RLS。
- macOS 桌面端以菜单栏常驻图标运行，不占用 Dock；关闭按钮会隐藏窗口，应用仍保留在菜单栏。
- Windows 端同样支持系统托盘常驻；主窗口隐藏后应用仍保留在托盘。
- 任务和记录类型实现 `Serialize` / `Deserialize`，便于 JSON API。

## 目录

```text
core/src/
├── application.rs   # TaskService 与命令 DTO
├── domain.rs        # 任务、阻塞、工作会话领域类型
├── error.rs         # 统一错误类型
├── persistence.rs   # SQLite 存储与查询
├── sync.rs          # Supabase 同步快照与合并
└── lib.rs           # 对外导出
```

## 验证

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo check --workspace
```

## 推荐接入方式

```text
Tauri 2 桌面 GUI ─┐
Axum Web API      ├── TaskService ─── TaskStore ─── SQLite
CLI / 自动化工具 ─┘
```

推荐先以 `TaskService` 作为稳定业务边界，再分别添加 Tauri command、Axum handler 或 UniFFI 绑定。

## Supabase 同步

`supabase-schema.sql` 是远端同步数据库表结构。5 张表均包含 `user_id`，RLS 使用 `auth.uid()` 限制 authenticated 用户只能读写自己的数据；anon 角色没有权限。同步按 `updated_at` / `version` 合并本地与远端记录，并保留软删除数据。

`supabase-sample/` 提供与当前本地模型一致的样例 CSV，导入顺序为 `workspaces` → `priorities` → `tasks` → `task_blocks` → `work_sessions`。
