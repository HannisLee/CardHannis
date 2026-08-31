//! CardHannis 的跨平台任务核心。
//!
//! 推荐 GUI、Web API、CLI 等适配层依赖 [`TaskService`]，而不是直接操作 SQLite。

mod application;
mod domain;
mod error;
mod persistence;

pub use application::{BlockTaskCommand, CreateTaskCommand, TaskService, UpdateTaskCommand};
pub use domain::{NewTask, Task, TaskBlock, TaskStatus, WorkSession};
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
}
