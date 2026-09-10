//! CardHannis 的跨平台任务核心。
//!
//! 推荐 GUI、Web API、CLI 等适配层依赖 [`TaskService`]，而不是直接操作 SQLite。

mod application;
mod domain;
mod error;
mod persistence;
mod sync;

pub use application::{BlockTaskCommand, CreateTaskCommand, TaskService, UpdateTaskCommand};
pub use domain::{NewTask, Priority, Task, TaskBlock, TaskStatus, WorkSession, Workspace};
pub use error::{CoreError, Result};
pub use persistence::TaskStore;
pub use sync::{SyncCounts, SyncSnapshot, merge_sync_snapshots};

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
                due_date: None,
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
            .end_block(
                &first.id,
                first.version,
                Some("接口已确认"),
                "2026-08-31T03:00:00.000Z",
            )
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
    fn sync_merges_newer_remote_task_and_recomputes_blocked() {
        let service = service();
        let task = create(&service);
        let local = service.sync_snapshot().unwrap();

        let mut remote_task = task.clone();
        remote_task.title = "远端更新后的标题".to_owned();
        remote_task.updated_at = "2030-01-01T00:00:00.000Z".to_owned();
        remote_task.version += 1;
        let remote_block = TaskBlock {
            id: "remote-block".to_owned(),
            task_id: task.id.clone(),
            started_at: "2026-09-10T12:01:00.000Z".to_owned(),
            ended_at: None,
            reason: "远端阻塞".to_owned(),
            note: None,
            resolution_reason: None,
            created_at: "2026-09-10T12:01:00.000Z".to_owned(),
            updated_at: "2026-09-10T12:01:00.000Z".to_owned(),
            version: 1,
            deleted_at: None,
        };
        let remote = SyncSnapshot {
            tasks: vec![remote_task],
            task_blocks: vec![remote_block],
            ..SyncSnapshot::default()
        };

        let merged = merge_sync_snapshots(local, remote);
        service.apply_sync_snapshot(merged).unwrap();
        let synced = service.get(&task.id).unwrap().unwrap();
        assert_eq!(synced.title, "远端更新后的标题");
        assert_eq!(synced.version, task.version + 1);
        assert!(synced.is_blocked);
    }

    #[test]
    fn sync_resolves_conflicting_active_sessions() {
        let service = service();
        let task = create(&service);
        service.start(&task.id, task.version).unwrap();
        service.begin_work(&task.id, None).unwrap();
        let local = service.sync_snapshot().unwrap();

        let remote_session = WorkSession {
            id: "remote-session".to_owned(),
            task_id: task.id.clone(),
            started_at: "2030-01-01T00:30:00.000Z".to_owned(),
            ended_at: None,
            note: None,
            created_at: "2030-01-01T00:30:00.000Z".to_owned(),
        };
        let remote = SyncSnapshot {
            tasks: local.tasks.clone(),
            work_sessions: vec![remote_session],
            ..SyncSnapshot::default()
        };

        let merged = merge_sync_snapshots(local, remote);
        service.apply_sync_snapshot(merged).unwrap();
        let sessions = service.store().list_all_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(
            sessions
                .iter()
                .filter(|session| session.ended_at.is_none())
                .count(),
            1
        );
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
        let priorities = service.priorities(false).unwrap();
        assert_eq!(priorities.len(), 6);
        assert_eq!(
            priorities
                .iter()
                .filter(|priority| priority.workspace_id == "daily")
                .count(),
            3
        );
        assert_eq!(
            priorities
                .iter()
                .filter(|priority| priority.workspace_id == "work")
                .count(),
            3
        );
        assert_eq!(
            priorities
                .iter()
                .find(|priority| priority.id == "P1")
                .map(|priority| priority.workspace_id.as_str()),
            Some("daily")
        );
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
        let ws_priority_ids: Vec<String> = service
            .priorities(false)
            .unwrap()
            .into_iter()
            .filter(|priority| priority.workspace_id == ws.id)
            .map(|priority| priority.id)
            .collect();
        assert_eq!(ws_priority_ids.len(), 3);
        let renamed = service
            .rename_workspace(&ws.id, ws.version, "研究")
            .unwrap();
        assert_eq!(renamed.name, "研究");
        service
            .delete_workspace(&renamed.id, renamed.version)
            .unwrap();
        assert!(service.workspace(&ws.id).unwrap().is_none());
        assert!(
            ws_priority_ids
                .iter()
                .all(|priority_id| service.priority(priority_id).unwrap().is_none())
        );

        // 有任务的工作区不能删
        let ws2 = service.create_workspace("项目A").unwrap();
        let task = service
            .create(CreateTaskCommand {
                title: "项目任务".into(),
                notes: None,
                estimated_active_minutes: None,
                due_date: None,
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

        // 分级仅属于项目A；日常工作区的分级不能用于项目A任务。
        let daily_priority = service
            .priorities(false)
            .unwrap()
            .into_iter()
            .find(|priority| priority.workspace_id == "daily")
            .unwrap();
        let err = service
            .create(CreateTaskCommand {
                title: "跨工作区分级".into(),
                notes: None,
                estimated_active_minutes: None,
                due_date: None,
                sort_order: 0,
                created_device_id: "test-device".into(),
                workspace_id: Some(ws2.id.clone()),
                priority_id: Some(daily_priority.id),
            })
            .unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));

        // 分级：新建/改名改色/删除规则只影响当前工作区。
        let p = service
            .create_priority(&ws2.id, "P3", Some("#5c7699"))
            .unwrap();
        let updated = service
            .update_priority(&p.id, p.version, "长期", Some("#7a5ea6"))
            .unwrap();
        assert_eq!(updated.name, "长期");
        let others: Vec<Priority> = service
            .priorities(false)
            .unwrap()
            .into_iter()
            .filter(|priority| priority.workspace_id == ws2.id)
            .collect();
        assert_eq!(others.len(), 4);
        for other in &others {
            if other.id != p.id {
                let err = service.delete_priority(&other.id, other.version);
                assert!(err.is_ok()); // 空分级可删
            }
        }
        let last: Vec<Priority> = service
            .priorities(false)
            .unwrap()
            .into_iter()
            .filter(|priority| priority.workspace_id == ws2.id)
            .collect();
        assert_eq!(last.len(), 1);
        assert_eq!(last[0].id, p.id);
        assert!(matches!(
            service.delete_priority(&p.id, updated.version),
            Err(CoreError::InvalidState(_))
        ));
        assert_eq!(
            service
                .priorities(false)
                .unwrap()
                .into_iter()
                .filter(|priority| priority.workspace_id == "daily")
                .count(),
            3
        );
    }

    #[test]
    fn workspace_reorder_keeps_done_last() {
        let service = service();
        let daily = service.workspace("daily").unwrap().unwrap();
        let work = service.workspace("work").unwrap().unwrap();

        let ordered_ids = vec![work.id.clone(), daily.id.clone()];
        let expected_versions = vec![work.version, daily.version];
        let reordered = service
            .reorder_workspaces(&ordered_ids, &expected_versions)
            .unwrap();
        let names: Vec<&str> = reordered
            .iter()
            .map(|workspace| workspace.name.as_str())
            .collect();
        assert_eq!(names, vec!["工作", "日常", "已完成"]);
        assert_eq!(
            reordered.last().map(|workspace| workspace.id.as_str()),
            Some("done")
        );

        let project = service.create_workspace("项目").unwrap();
        let workspaces = service.workspaces(false).unwrap();
        assert_eq!(
            workspaces.last().map(|workspace| workspace.id.as_str()),
            Some("done")
        );
        assert_eq!(
            workspaces
                .iter()
                .map(|workspace| workspace.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                work.id.as_str(),
                daily.id.as_str(),
                project.id.as_str(),
                "done"
            ]
        );

        let daily = service.workspace("daily").unwrap().unwrap();
        let stale_version = daily.version - 1;
        let err = service.reorder_workspaces(
            &[daily.id.clone(), work.id.clone(), project.id.clone()],
            &[stale_version, work.version, project.version],
        );
        assert!(matches!(err, Err(CoreError::VersionConflict)));
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
                due_date: None,
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

        // 只有「已完成」是内置工作区；日常/工作在为空时可删除。
        let daily = service.workspace("daily").unwrap().unwrap();
        assert!(!daily.builtin);
        service.delete_workspace("daily", daily.version).unwrap();
        assert!(service.workspace("daily").unwrap().is_none());

        let work = service.workspace("work").unwrap().unwrap();
        assert!(!work.builtin);
        service.delete_workspace("work", work.version).unwrap();
        assert!(service.workspace("work").unwrap().is_none());

        let err = service.delete_workspace("done", 1).unwrap_err();
        assert!(matches!(err, CoreError::InvalidState(_)));
    }

    #[test]
    fn updating_archived_task_preserves_historical_priority() {
        let service = service();
        let workspace = service.create_workspace("项目Y").unwrap();
        let priority = service
            .priorities(false)
            .unwrap()
            .into_iter()
            .find(|priority| priority.workspace_id == workspace.id)
            .unwrap();
        let task = service
            .create(CreateTaskCommand {
                title: "归档后编辑".into(),
                notes: None,
                estimated_active_minutes: None,
                due_date: None,
                sort_order: 0,
                created_device_id: "test-device".into(),
                workspace_id: Some(workspace.id.clone()),
                priority_id: Some(priority.id.clone()),
            })
            .unwrap();
        let done = service.complete(&task.id, task.version).unwrap();
        let updated = service
            .update(
                &done.id,
                done.version,
                UpdateTaskCommand {
                    title: "归档后已编辑".into(),
                    notes: done.notes.clone(),
                    review_notes: done.review_notes.clone(),
                    estimated_active_minutes: done.estimated_active_minutes,
                    due_date: done.due_date.clone(),
                    sort_order: done.sort_order,
                    workspace_id: done.workspace_id.clone(),
                    priority_id: done.priority_id.clone(),
                },
            )
            .unwrap();
        assert_eq!(updated.title, "归档后已编辑");
        assert_eq!(updated.workspace_id.as_deref(), Some("done"));
        assert_eq!(
            updated.home_workspace_id.as_deref(),
            Some(workspace.id.as_str())
        );
        assert_eq!(updated.priority_id.as_deref(), Some(priority.id.as_str()));
        let reopened = service.reopen(&updated.id, updated.version).unwrap();
        assert_eq!(
            reopened.workspace_id.as_deref(),
            Some(workspace.id.as_str())
        );
    }

    #[test]
    fn create_task_persists_due_date() {
        let service = service();
        let task = service
            .create(CreateTaskCommand {
                title: "带截止日期".into(),
                notes: None,
                estimated_active_minutes: None,
                due_date: Some("2026-09-05".into()),
                sort_order: 0,
                created_device_id: "test-device".into(),
                workspace_id: None,
                priority_id: None,
            })
            .unwrap();
        assert_eq!(task.due_date.as_deref(), Some("2026-09-05"));
        let updated = service
            .update(
                &task.id,
                task.version,
                UpdateTaskCommand {
                    title: task.title.clone(),
                    notes: task.notes.clone(),
                    review_notes: task.review_notes.clone(),
                    estimated_active_minutes: task.estimated_active_minutes,
                    due_date: Some("2026-09-06".into()),
                    sort_order: task.sort_order,
                    workspace_id: task.workspace_id.clone(),
                    priority_id: task.priority_id.clone(),
                },
            )
            .unwrap();
        assert_eq!(updated.due_date.as_deref(), Some("2026-09-06"));

        let err = service
            .create(CreateTaskCommand {
                title: "非法日期".into(),
                notes: None,
                estimated_active_minutes: None,
                due_date: Some("2026/09/05".into()),
                sort_order: 0,
                created_device_id: "test-device".into(),
                workspace_id: None,
                priority_id: None,
            })
            .unwrap_err();
        assert!(matches!(err, CoreError::InvalidInput(_)));
    }

    #[test]
    fn leaving_in_progress_ends_active_work_session() {
        let service = service();
        let task = create(&service);
        service.begin_work(&task.id, None).unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        service.pause(&task.id, task.version).unwrap();

        let sessions = service.sessions(&task.id).unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].ended_at.is_some());

        service.begin_work(&task.id, None).unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        let done = service.complete(&task.id, task.version).unwrap();
        let sessions = service.sessions(&task.id).unwrap();
        assert_eq!(sessions.len(), 2);
        assert!(sessions.iter().all(|session| session.ended_at.is_some()));
        assert_eq!(done.status, TaskStatus::Completed);
    }

    #[test]
    fn unblock_returns_to_pending_and_reopen_works() {
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
        // 解除阻塞 → 待处理（等待中已并入）
        let unblocked = service
            .unblock(&block.id, block.version, Some("依赖已就绪"))
            .unwrap();
        assert_eq!(unblocked.resolution_reason.as_deref(), Some("依赖已就绪"));
        let task = service.get(&task.id).unwrap().unwrap();
        assert_eq!(task.status, TaskStatus::Pending);
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

    #[test]
    fn correcting_active_work_time_shortens_and_restarts() {
        let service = service();
        let task = service
            .store()
            .create_task_at(
                NewTask {
                    title: "修正进行中".into(),
                    notes: None,
                    estimated_active_minutes: None,
                    due_date: None,
                    sort_order: 0,
                    created_device_id: "test-device".into(),
                    workspace_id: None,
                    priority_id: None,
                },
                "2026-01-01T08:00:00.000Z",
            )
            .unwrap();
        service
            .store()
            .start_work(&task.id, "2026-01-01T08:00:00.000Z", None)
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();

        let corrected = service
            .store()
            .correct_work_time_at(&task.id, task.version, 180, "2026-01-01T18:00:00.000Z")
            .unwrap();

        assert_eq!(corrected.status, TaskStatus::InProgress);
        let sessions = service.sessions(&task.id).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].started_at, "2026-01-01T15:00:00.000Z");
        assert_eq!(
            sessions[0].ended_at.as_deref(),
            Some("2026-01-01T18:00:00.000Z")
        );
        assert_eq!(sessions[1].started_at, "2026-01-01T18:00:00.000Z");
        assert!(sessions[1].ended_at.is_none());
    }

    #[test]
    fn correcting_pending_work_time_updates_last_pause() {
        let service = service();
        let task = service
            .store()
            .create_task_at(
                NewTask {
                    title: "修正待办".into(),
                    notes: None,
                    estimated_active_minutes: None,
                    due_date: None,
                    sort_order: 0,
                    created_device_id: "test-device".into(),
                    workspace_id: None,
                    priority_id: None,
                },
                "2026-01-01T08:00:00.000Z",
            )
            .unwrap();
        service
            .store()
            .start_work(&task.id, "2026-01-01T08:00:00.000Z", None)
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        service
            .store()
            .set_status(
                &task.id,
                task.version,
                TaskStatus::Pending,
                "2026-01-01T18:00:00.000Z",
            )
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();

        let corrected = service
            .store()
            .correct_work_time_at(&task.id, task.version, 180, "2026-01-01T18:00:00.000Z")
            .unwrap();

        assert_eq!(corrected.status, TaskStatus::Pending);
        let sessions = service.sessions(&task.id).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].started_at, "2026-01-01T15:00:00.000Z");
        assert_eq!(
            sessions[0].ended_at.as_deref(),
            Some("2026-01-01T18:00:00.000Z")
        );
    }

    #[test]
    fn correcting_work_time_without_history_creates_session() {
        let service = service();
        let task = service
            .store()
            .create_task_at(
                NewTask {
                    title: "补录时间".into(),
                    notes: None,
                    estimated_active_minutes: None,
                    due_date: None,
                    sort_order: 0,
                    created_device_id: "test-device".into(),
                    workspace_id: None,
                    priority_id: None,
                },
                "2026-01-01T08:00:00.000Z",
            )
            .unwrap();

        let corrected = service
            .store()
            .correct_work_time_at(&task.id, task.version, 180, "2026-01-01T18:00:00.000Z")
            .unwrap();

        assert_eq!(corrected.status, TaskStatus::Pending);
        let sessions = service.sessions(&task.id).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].started_at, "2026-01-01T15:00:00.000Z");
        assert_eq!(
            sessions[0].ended_at.as_deref(),
            Some("2026-01-01T18:00:00.000Z")
        );
    }

    #[test]
    fn correcting_work_time_respects_blocks() {
        let service = service();
        let task = service
            .store()
            .create_task_at(
                NewTask {
                    title: "阻塞边界".into(),
                    notes: None,
                    estimated_active_minutes: None,
                    due_date: None,
                    sort_order: 0,
                    created_device_id: "test-device".into(),
                    workspace_id: None,
                    priority_id: None,
                },
                "2026-01-01T08:00:00.000Z",
            )
            .unwrap();
        service
            .store()
            .start_work(&task.id, "2026-01-01T08:00:00.000Z", None)
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        let block = service
            .store()
            .start_block(&task.id, "等待依赖", None, "2026-01-01T12:00:00.000Z")
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();

        let corrected = service
            .store()
            .correct_work_time_at(&task.id, task.version, 180, "2026-01-01T12:00:00.000Z")
            .unwrap();

        assert_eq!(corrected.status, TaskStatus::InProgress);
        assert!(corrected.is_blocked);
        let sessions = service.sessions(&task.id).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].started_at, "2026-01-01T09:00:00.000Z");
        assert_eq!(
            sessions[0].ended_at.as_deref(),
            Some("2026-01-01T12:00:00.000Z")
        );
        assert_eq!(block.started_at, "2026-01-01T12:00:00.000Z");
    }

    #[test]
    fn correcting_work_time_respects_completion_time() {
        let service = service();
        let task = service
            .store()
            .create_task_at(
                NewTask {
                    title: "完成边界".into(),
                    notes: None,
                    estimated_active_minutes: None,
                    due_date: None,
                    sort_order: 0,
                    created_device_id: "test-device".into(),
                    workspace_id: None,
                    priority_id: None,
                },
                "2026-01-01T08:00:00.000Z",
            )
            .unwrap();
        service
            .store()
            .start_work(&task.id, "2026-01-01T08:00:00.000Z", None)
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        service
            .store()
            .set_status(
                &task.id,
                task.version,
                TaskStatus::Completed,
                "2026-01-01T18:00:00.000Z",
            )
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();

        let corrected = service
            .store()
            .correct_work_time_at(&task.id, task.version, 180, "2026-01-01T20:00:00.000Z")
            .unwrap();

        assert_eq!(corrected.status, TaskStatus::Completed);
        let sessions = service.sessions(&task.id).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].started_at, "2026-01-01T15:00:00.000Z");
        assert_eq!(
            sessions[0].ended_at.as_deref(),
            Some("2026-01-01T18:00:00.000Z")
        );
    }

    #[test]
    fn correcting_work_time_can_recurse_across_available_windows() {
        let service = service();
        let task = service
            .store()
            .create_task_at(
                NewTask {
                    title: "递归修正".into(),
                    notes: None,
                    estimated_active_minutes: None,
                    due_date: None,
                    sort_order: 0,
                    created_device_id: "test-device".into(),
                    workspace_id: None,
                    priority_id: None,
                },
                "2026-01-01T08:00:00.000Z",
            )
            .unwrap();
        service
            .store()
            .start_work(&task.id, "2026-01-01T08:00:00.000Z", None)
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        service
            .store()
            .set_status(
                &task.id,
                task.version,
                TaskStatus::Pending,
                "2026-01-01T09:00:00.000Z",
            )
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        let block = service
            .store()
            .start_block(&task.id, "等待依赖", None, "2026-01-01T09:00:00.000Z")
            .unwrap();
        service
            .store()
            .end_block(&block.id, block.version, None, "2026-01-01T10:00:00.000Z")
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        service
            .store()
            .start_work(&task.id, "2026-01-01T10:00:00.000Z", None)
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();
        service
            .store()
            .set_status(
                &task.id,
                task.version,
                TaskStatus::Pending,
                "2026-01-01T11:00:00.000Z",
            )
            .unwrap();
        let task = service.get(&task.id).unwrap().unwrap();

        let corrected = service
            .store()
            .correct_work_time_at(&task.id, task.version, 90, "2026-01-01T11:00:00.000Z")
            .unwrap();

        assert_eq!(corrected.status, TaskStatus::Pending);
        let sessions = service.sessions(&task.id).unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].started_at, "2026-01-01T08:30:00.000Z");
        assert_eq!(
            sessions[0].ended_at.as_deref(),
            Some("2026-01-01T09:00:00.000Z")
        );
        assert_eq!(sessions[1].started_at, "2026-01-01T10:00:00.000Z");
        assert_eq!(
            sessions[1].ended_at.as_deref(),
            Some("2026-01-01T11:00:00.000Z")
        );
    }

    #[test]
    fn correcting_work_time_rejects_invalid_target_and_version() {
        let service = service();
        let task = service
            .store()
            .create_task_at(
                NewTask {
                    title: "非法目标".into(),
                    notes: None,
                    estimated_active_minutes: None,
                    due_date: None,
                    sort_order: 0,
                    created_device_id: "test-device".into(),
                    workspace_id: None,
                    priority_id: None,
                },
                "2026-01-01T08:00:00.000Z",
            )
            .unwrap();

        let zero = service
            .store()
            .correct_work_time_at(&task.id, task.version, 0, "2026-01-01T18:00:00.000Z")
            .unwrap_err();
        assert!(matches!(zero, CoreError::InvalidInput(_)));

        let too_long = service
            .store()
            .correct_work_time_at(&task.id, task.version, 120, "2026-01-01T09:00:00.000Z")
            .unwrap_err();
        assert!(matches!(too_long, CoreError::InvalidInput(_)));

        let conflict = service
            .store()
            .correct_work_time_at(&task.id, task.version + 1, 60, "2026-01-01T09:00:00.000Z")
            .unwrap_err();
        assert!(matches!(conflict, CoreError::VersionConflict));
    }
}
