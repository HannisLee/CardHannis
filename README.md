# CardHannis Core

CardHannis Core 是一个跨平台 Rust 核心库，负责任务、重复阻塞记录和工作会话。

## 当前实现

- `cardhannis-core`：可嵌入桌面 GUI、CLI、Tauri 或 HTTP/Web API 适配层。
- SQLite 持久化，内置迁移。
- `task_blocks` 独立保存每次阻塞，支持同一任务多次阻塞；解除原因保存在 `resolution_reason`。
- 任务支持可选 `due_date`（`YYYY-MM-DD`）作为预计完成日期。
- 同一任务最多一个未结束的阻塞和一个未结束的工作会话；任务离开进行中状态时核心层会结束活动会话。
- SQLite 外键、软删除、版本号乐观并发控制。
- `TaskService` 作为 GUI、CLI 共用的业务门面。
- 桌面端内置一个默认关闭的 Web 设置控制台，可在设置中点击「前往」打开，5 分钟无操作后自动关闭。
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
