//! CardHannis 的跨平台任务核心。
//!
//! 推荐 GUI、Web API、CLI 等适配层依赖 [`TaskService`]，而不是直接操作 SQLite。

mod application;
mod domain;
mod error;
mod persistence;

pub use application::{BlockTaskCommand, CreateTaskCommand, TaskService, UpdateTaskCommand};
pub use domain::{NewTask, Priority, Task, TaskBlock, TaskStatus, WorkSession, Workspace};
pub use error::{CoreError, Result};
pub use persistence::TaskStore;

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> TaskService {
        TaskService::new(TaskStore::open_in_memory().unwrap())
    }
    fn create(service: &TaskService) -> Task {
        service
            .create(CreateTaskCommand {
                title: "编写核心".into(),
                notes: None,
                estimated_active_minutes: Some(60),
                sort_order: 0,
                created_device_id: "test-device".into(),
                workspace_id: None,
                priority_id: None,
            })
            .unwrap()
    }

    #[test]
    fn service_exposes_repeated_block_history() {
        let service = service();
        let task = create(&service);
        let first = service
            .store()
            .start_block(
                &task.id,
                "等待接口定义",
                Some("第一次阻塞"),
                "2026-08-31T02:00:00.000Z",
            )
            .unwrap();
        let first = service
            .store()
            .end_block(&first.id, first.version, "2026-08-31T03:00:00.000Z")
            .unwrap();
        let second = service
            .store()
            .start_block(&task.id, "等待测试环境", None, "2026-08-31T04:00:00.000Z")
            .unwrap();
        assert!(first.ended_at.is_some());
        assert_eq!(service.blocks(&task.id, false).unwrap().len(), 2);
        assert_eq!(second.reason, "等待测试环境");
        assert!(service.get(&task.id).unwrap().unwrap().is_blocked);
    }

    #[test]
    fn service_enforces_versions_and_lifecycle() {
        let service = service();
        let task = create(&service);
        let started = service.start(&task.id, task.version).unwrap();
        assert_eq!(started.status, TaskStatus::InProgress);
        assert!(matches!(
            service.start(&task.id, task.version),
            Err(CoreError::VersionConflict)
        ));
        let completed = service.complete(&task.id, started.version).unwrap();
        assert_eq!(completed.status, TaskStatus::Completed);
        assert!(matches!(
            service.begin_work(&task.id, None),
            Err(CoreError::InvalidState(_))
        ));
    }

    #[test]
    fn starting_a_block_stops_active_work() {
        let service = service();
        let task = create(&service);
        service.begin_work(&task.id, None).unwrap();
        service
            .block(
                &task.id,
                BlockTaskCommand {
                    reason: "依赖未完成".into(),
                    note: None,
                },
            )
            .unwrap();
        let sessions = service.sessions(&task.id).unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].ended_at.is_some());
    }

    #[test]
    fn migration_0002_upgrades_legacy_database() {
        // 模拟只有 0001 的旧数据库：直接执行初始迁移并写入任务
        let dir = std::env::temp_dir().join(format!("cardhannis-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cardhannis.sqlite3");
        {
            let connection = rusqlite::Connection::open(&path).unwrap();
            connection
                .execute_batch(include_str!("../migrations/0001_initial.sql"))
                .unwrap();
            connection
                .execute(
                    "INSERT INTO tasks (id, title, created_at, sort_order, created_device_id, updated_at) VALUES ('t-old', '旧任务', '2026-01-01T00:00:00.000Z', 0, 'legacy', '2026-01-01T00:00:00.000Z')",
                    [],
                )
                .unwrap();
        }
        // 用新版 TaskStore 打开 → 0002 自动应用
        let service = TaskService::new(TaskStore::open(&path).unwrap());
        let tasks = service.list(false).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "旧任务");
        assert_eq!(tasks[0].workspace_id.as_deref(), Some("daily"));
        assert_eq!(tasks[0].priority_id.as_deref(), Some("P1"));
        assert_eq!(service.workspaces(false).unwrap().len(), 3);
        assert_eq!(service.priorities(false).unwrap().len(), 3);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn workspaces_and_priorities_lifecycle() {
        let service = service();
        // 内置种子存在
        assert!(service.workspace("daily").unwrap().is_some());
        assert!(service.workspace("work").unwrap().is_some());
        assert!(service.priority("P0").unwrap().is_some());

        // 新建/重命名/删除工作区
        let ws = service.create_workspace("学习").unwrap();
        assert_eq!(ws.name, "学习");
        let renamed = service
            .rename_workspace(&ws.id, ws.version, "研究")
            .unwrap();
        assert_eq!(renamed.name, "研究");
        service
            .delete_workspace(&renamed.id, renamed.version)
            .unwrap();
        assert!(service.workspace(&ws.id).unwrap().is_none());

        // 有任务的工作区不能删
        let ws2 = service.create_workspace("项目A").unwrap();
        let task = service
            .create(CreateTaskCommand {
                title: "项目任务".into(),
                notes: None,
                estimated_active_minutes: None,
                sort_order: 0,
                created_device_id: "test-device".into(),
                workspace_id: Some(ws2.id.clone()),
                priority_id: None,
            })
            .unwrap();
        assert_eq!(task.workspace_id.as_deref(), Some(ws2.id.as_str()));
        assert!(task.priority_id.is_none());
        let err = service.delete_workspace(&ws2.id, ws2.version).unwrap_err();
        assert!(matches!(err, CoreError::InvalidState(_)));

        // 分级：新建/改名改色/删除规则
        let p = service.create_priority("P3", Some("#5c7699")).unwrap();
        let updated = service
            .update_priority(&p.id, p.version, "长期", Some("#7a5ea6"))
            .unwrap();
        assert_eq!(updated.name, "长期");
        // 最后一个分级不能删
        let others = service.priorities(false).unwrap();
        assert_eq!(others.len(), 4);
        for other in &others {
            if other.id != p.id {
                let err = service.delete_priority(&other.id, other.version);
                assert!(err.is_ok()); // 空分级可删
            }
        }
        let last = service.priorities(false).unwrap();
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].id, p.id);
        let err = service
            .delete_priority(&p.id, updated.version + 3)
            .unwrap_err();
        assert!(matches!(
            err,
            CoreError::VersionConflict | CoreError::InvalidState(_)
        ));
        assert!(service.delete_priority(&p.id, last[0].version).is_err()); // 剩最后一个
    }

    #[test]
    fn completion_archives_to_done_workspace_and_reopen_returns_home() {
        let service = service();
        // done 工作区由迁移种子
        let done_ws = service.workspace("done").unwrap();
        assert!(done_ws.is_some());
        assert!(done_ws.unwrap().builtin);

        // 在工作区创建任务并完成 → 自动归档到 done
        let ws = service.create_workspace("项目X").unwrap();
        let task = service
            .create(CreateTaskCommand {
                title: "归档验证".into(),
                notes: None,
                estimated_active_minutes: None,
                sort_order: 0,
                created_device_id: "test-device".into(),
                workspace_id: Some(ws.id.clone()),
                priority_id: None,
            })
            .unwrap();
        assert_eq!(task.home_workspace_id.as_deref(), Some(ws.id.as_str()));
        let task = service.get(&task.id).unwrap().unwrap();
        let done = service.complete(&task.id, task.version).unwrap();
        assert_eq!(done.workspace_id.as_deref(), Some("done"));

        // 重新打开 → 回到原工作区
        let reopened = service.reopen(&done.id, done.version).unwrap();
        assert_eq!(reopened.workspace_id.as_deref(), Some(ws.id.as_str()));

        // 内置工作区（含已完成）不能删除
        let err = service.delete_workspace("done", 1).unwrap_err();
        assert!(matches!(err, CoreError::InvalidState(_)));
        let err = service.delete_workspace("daily", 1).unwrap_err();
        assert!(matches!(err, CoreError::InvalidState(_)));
    }

    #[test]
    fn unblock_moves_task_to_waiting_and_reopen_works() {
        let service = service();
        let task = create(&service);
        let block = service
            .block(
                &task.id,
                BlockTaskCommand {
                    reason: "等依赖".into(),
                    note: None,
                },
            )
            .unwrap();
        // 解除阻塞 → 等待中
        service.unblock(&block.id, block.version).unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Waiting);
        assert!(!task.is_blocked);
        // 等待中可以开工
        service.begin_work(&task.id, None).unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::InProgress);
        // 完成后可重新打开
        let task = service.get(&task.id).unwrap().unwrap();
        let done = service.complete(&task.id, task.version).unwrap();
        assert_eq!(done.status, TaskStatus::Completed);
        let reopened = service.reopen(&done.id, done.version).unwrap();
        assert_eq!(reopened.status, TaskStatus::Pending);
        assert!(reopened.completed_at.is_none());
        // 未完成任务不能重新打开
        let err = service.reopen(&reopened.id, reopened.version).unwrap_err();
        assert!(matches!(err, CoreError::InvalidState(_)));
    }
}
