use crate::{domain::*, error::*};
use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Mutex,
};
use uuid::Uuid;

const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_initial.sql",
        include_str!("../migrations/0001_initial.sql"),
    ),
    (
        "0002_workspaces_priorities.sql",
        include_str!("../migrations/0002_workspaces_priorities.sql"),
    ),
    (
        "0003_done_workspace.sql",
        include_str!("../migrations/0003_done_workspace.sql"),
    ),
    (
        "0004_archive_completed.sql",
        include_str!("../migrations/0004_archive_completed.sql"),
    ),
    (
        "0005_merge_waiting.sql",
        include_str!("../migrations/0005_merge_waiting.sql"),
    ),
    (
        "0006_due_date_and_unblock_reason.sql",
        include_str!("../migrations/0006_due_date_and_unblock_reason.sql"),
    ),
    (
        "0007_workspace_scoped_priorities.sql",
        include_str!("../migrations/0007_workspace_scoped_priorities.sql"),
    ),
    (
        "0008_only_done_builtin_workspace.sql",
        include_str!("../migrations/0008_only_done_builtin_workspace.sql"),
    ),
];

/// SQLite 持久化实现。上层 GUI、Web API 和 CLI 不应直接拼 SQL，而应通过
/// [`crate::TaskService`] 调用业务操作。
pub struct TaskStore {
    connection: Mutex<Connection>,
}

impl TaskStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(connection: Connection) -> Result<Self> {
        connection.execute_batch("PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (name TEXT PRIMARY KEY NOT NULL);",
        )?;
        for (name, sql) in MIGRATIONS {
            let applied = connection
                .query_row(
                    "SELECT 1 FROM schema_migrations WHERE name = ?1",
                    params![name],
                    |_| Ok(true),
                )
                .optional()?
                .unwrap_or(false);
            if applied {
                continue;
            }
            connection.execute_batch(sql)?;
            connection.execute(
                "INSERT OR IGNORE INTO schema_migrations (name) VALUES (?1)",
                params![name],
            )?;
        }
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn create_task(&self, input: NewTask) -> Result<Task> {
        self.create_task_at(input, now())
    }

    pub fn create_task_at(&self, input: NewTask, created_at: impl Into<String>) -> Result<Task> {
        validate_new_task(&input)?;
        let created_at = created_at.into();
        if let Some(workspace_id) = &input.workspace_id {
            self.get_workspace(workspace_id)?
                .ok_or_else(|| CoreError::WorkspaceNotFound(workspace_id.clone()))?;
        }
        if let Some(priority_id) = &input.priority_id {
            let priority = self
                .get_priority(priority_id)?
                .ok_or_else(|| CoreError::PriorityNotFound(priority_id.clone()))?;
            if input.workspace_id.as_deref() != Some(priority.workspace_id.as_str()) {
                return Err(CoreError::InvalidInput("分级不属于当前工作区".into()));
            }
        }
        let id = Uuid::new_v4().to_string();
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "INSERT INTO tasks (id, title, notes, estimated_active_minutes, due_date, created_at, sort_order, created_device_id, updated_at, workspace_id, priority_id, home_workspace_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?6, ?9, ?10, ?9)",
            params![id, input.title.trim(), input.notes, input.estimated_active_minutes, input.due_date, created_at, input.sort_order, input.created_device_id.trim(), input.workspace_id, input.priority_id],
        )?;
        drop(connection);
        self.get_task(&id)?.ok_or(CoreError::TaskNotFound(id))
    }

    pub fn get_task(&self, id: &str) -> Result<Option<Task>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        load_task(&*connection, id)
    }

    pub fn list_tasks(&self, include_deleted: bool) -> Result<Vec<Task>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let sql = if include_deleted {
            TASK_SELECT_SQL
        } else {
            "SELECT t.id, t.title, t.notes, t.review_notes, t.estimated_active_minutes, t.due_date, t.created_at, t.started_at, t.completed_at, t.status, t.sort_order, t.created_device_id, t.updated_at, t.deleted_at, t.version, EXISTS (SELECT 1 FROM task_blocks b WHERE b.task_id = t.id AND b.ended_at IS NULL AND b.deleted_at IS NULL), t.workspace_id, t.priority_id, t.home_workspace_id FROM tasks t WHERE t.deleted_at IS NULL ORDER BY t.sort_order, t.created_at"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], map_task)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn update_task(
        &self,
        id: &str,
        expected_version: i64,
        title: &str,
        notes: Option<&str>,
        review_notes: Option<&str>,
        estimated_active_minutes: Option<i64>,
        due_date: Option<&str>,
        sort_order: i64,
        workspace_id: Option<&str>,
        priority_id: Option<&str>,
        updated_at: impl Into<String>,
    ) -> Result<Task> {
        validate_task_fields(title, estimated_active_minutes)?;
        validate_due_date(due_date)?;
        let archived_task_home_workspace = if workspace_id == Some("done") {
            self.get_task(id)?
                .ok_or_else(|| CoreError::TaskNotFound(id.to_owned()))?
                .home_workspace_id
        } else {
            None
        };
        if let Some(priority_id) = priority_id {
            let priority = self
                .get_priority(priority_id)?
                .ok_or_else(|| CoreError::PriorityNotFound(priority_id.to_owned()))?;
            let validation_workspace = archived_task_home_workspace.as_deref().or(workspace_id);
            if validation_workspace != Some(priority.workspace_id.as_str()) {
                return Err(CoreError::InvalidInput("分级不属于当前工作区".into()));
            }
        }
        let connection = self.connection.lock().expect("database mutex poisoned");
        let changed = connection.execute(
            "UPDATE tasks SET title = ?1, notes = ?2, review_notes = ?3, estimated_active_minutes = ?4, due_date = ?11, sort_order = ?5, workspace_id = ?9, priority_id = ?10, home_workspace_id = CASE WHEN ?9 = 'done' THEN home_workspace_id ELSE COALESCE(?9, home_workspace_id) END, updated_at = ?6, version = version + 1 WHERE id = ?7 AND version = ?8 AND deleted_at IS NULL",
            params![
                title.trim(),
                notes,
                review_notes,
                estimated_active_minutes,
                sort_order,
                updated_at.into(),
                id,
                expected_version,
                workspace_id,
                priority_id,
                due_date,
            ],
        )?;
        if changed == 0 {
            return Err(resolve_task_update_error(&connection, id));
        }
        drop(connection);
        self.get_task(id)?
            .ok_or_else(|| CoreError::TaskNotFound(id.to_owned()))
    }

    pub fn set_status(
        &self,
        id: &str,
        expected_version: i64,
        status: TaskStatus,
        updated_at: impl Into<String>,
    ) -> Result<Task> {
        let updated_at = updated_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let task =
            load_task(&*connection, id)?.ok_or_else(|| CoreError::TaskNotFound(id.to_owned()))?;
        validate_status_change(&task, status)?;
        if status == TaskStatus::Completed && task.is_blocked {
            return Err(CoreError::InvalidState("请先解除任务阻塞".into()));
        }
        if task.version != expected_version {
            return Err(CoreError::VersionConflict);
        }
        // 离开进行中状态时结束活动会话，保证累计活动时间在完成/暂停后立即落库。
        if task.status == TaskStatus::InProgress && status != TaskStatus::InProgress {
            connection.execute(
                "UPDATE work_sessions SET ended_at = ?1 WHERE task_id = ?2 AND ended_at IS NULL",
                params![updated_at, id],
            )?;
        }
        let completed_at = (status == TaskStatus::Completed).then(|| updated_at.clone());
        let started_at = match status {
            TaskStatus::Pending => None,
            TaskStatus::InProgress | TaskStatus::Waiting | TaskStatus::Completed => {
                task.started_at.or(Some(updated_at.clone()))
            }
        };
        let workspace_sql = if status == TaskStatus::Completed {
            ", workspace_id = 'done'"
        } else {
            ""
        };
        connection.execute(
            &format!(
                "UPDATE tasks SET status = ?1, started_at = ?2, completed_at = ?3, updated_at = ?4{workspace_sql}, version = version + 1 WHERE id = ?5 AND version = ?6 AND deleted_at IS NULL"
            ),
            params![status.as_str(), started_at, completed_at, updated_at, id, expected_version],
        )?;
        drop(connection);
        self.get_task(id)?
            .ok_or_else(|| CoreError::TaskNotFound(id.to_owned()))
    }

    pub fn soft_delete_task(
        &self,
        id: &str,
        expected_version: i64,
        deleted_at: impl Into<String>,
    ) -> Result<()> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let deleted_at = deleted_at.into();
        let changed = connection.execute(
            "UPDATE tasks SET deleted_at = ?1, updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND deleted_at IS NULL",
            params![deleted_at, id, expected_version],
        )?;
        if changed == 0 {
            return Err(resolve_task_update_error(&connection, id));
        }
        Ok(())
    }

    pub fn start_work(
        &self,
        task_id: &str,
        started_at: impl Into<String>,
        note: Option<&str>,
    ) -> Result<WorkSession> {
        let started_at = started_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let tx = connection.unchecked_transaction()?;
        let task =
            load_task(&tx, task_id)?.ok_or_else(|| CoreError::TaskNotFound(task_id.to_owned()))?;
        validate_can_start_work(&task)?;
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO work_sessions (id, task_id, started_at, note, created_at) VALUES (?1, ?2, ?3, ?4, ?3)",
            params![id, task_id, started_at, note],
        ).map_err(map_constraint_error)?;
        if task.status == TaskStatus::Pending {
            tx.execute("UPDATE tasks SET status = 'in_progress', started_at = ?1, updated_at = ?1, version = version + 1 WHERE id = ?2", params![started_at, task_id])?;
        }
        tx.commit()?;
        drop(connection);
        self.get_session(&id)?.ok_or(CoreError::SessionNotFound(id))
    }

    pub fn end_work(&self, session_id: &str, ended_at: impl Into<String>) -> Result<WorkSession> {
        let ended_at = ended_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let changed = connection.execute(
            "UPDATE work_sessions SET ended_at = ?1 WHERE id = ?2 AND ended_at IS NULL",
            params![ended_at, session_id],
        )?;
        if changed == 0 {
            return Err(resolve_session_update_error(&connection, session_id));
        }
        drop(connection);
        self.get_session(session_id)?
            .ok_or_else(|| CoreError::SessionNotFound(session_id.to_owned()))
    }

    pub fn start_block(
        &self,
        task_id: &str,
        reason: &str,
        note: Option<&str>,
        started_at: impl Into<String>,
    ) -> Result<TaskBlock> {
        if reason.trim().is_empty() {
            return Err(CoreError::InvalidInput("阻塞原因不能为空".into()));
        }
        let started_at = started_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let tx = connection.unchecked_transaction()?;
        let task =
            load_task(&tx, task_id)?.ok_or_else(|| CoreError::TaskNotFound(task_id.to_owned()))?;
        validate_can_start_block(&task)?;
        tx.execute(
            "UPDATE work_sessions SET ended_at = ?1 WHERE task_id = ?2 AND ended_at IS NULL",
            params![started_at, task_id],
        )?;
        let id = Uuid::new_v4().to_string();
        tx.execute(
            "INSERT INTO task_blocks (id, task_id, started_at, reason, note, resolution_reason, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?3, ?3)",
            params![id, task_id, started_at, reason.trim(), note],
        ).map_err(map_constraint_error)?;
        tx.commit()?;
        drop(connection);
        self.get_block(&id)?.ok_or(CoreError::BlockNotFound(id))
    }

    pub fn end_block(
        &self,
        block_id: &str,
        expected_version: i64,
        resolution_reason: Option<&str>,
        ended_at: impl Into<String>,
    ) -> Result<TaskBlock> {
        let ended_at = ended_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let tx = connection.unchecked_transaction()?;
        let changed = tx.execute("UPDATE task_blocks SET ended_at = ?1, resolution_reason = ?2, updated_at = ?1, version = version + 1 WHERE id = ?3 AND version = ?4 AND ended_at IS NULL AND deleted_at IS NULL", params![ended_at, resolution_reason, block_id, expected_version])?;
        if changed == 0 {
            let error = resolve_block_update_error(&tx, block_id, expected_version);
            return Err(error);
        }
        // 解除阻塞后任务回到待处理（未完成且未删除时）
        tx.execute(
            "UPDATE tasks SET status = 'pending', updated_at = ?1, version = version + 1 WHERE id = (SELECT task_id FROM task_blocks WHERE id = ?2) AND deleted_at IS NULL AND status <> 'completed'",
            params![ended_at, block_id],
        )?;
        tx.commit()?;
        drop(connection);
        self.get_block(block_id)?
            .ok_or_else(|| CoreError::BlockNotFound(block_id.to_owned()))
    }

    /// 重新打开已完成任务（回到待处理）。
    pub fn reopen_task(
        &self,
        id: &str,
        expected_version: i64,
        updated_at: impl Into<String>,
    ) -> Result<Task> {
        let updated_at = updated_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let task =
            load_task(&*connection, id)?.ok_or_else(|| CoreError::TaskNotFound(id.to_owned()))?;
        if task.deleted_at.is_some() {
            return Err(CoreError::InvalidState("已删除任务不能重新打开".into()));
        }
        if task.status != TaskStatus::Completed {
            return Err(CoreError::InvalidState("只有已完成任务可以重新打开".into()));
        }
        if task.version != expected_version {
            return Err(CoreError::VersionConflict);
        }
        connection.execute(
            "UPDATE tasks SET status = 'pending', completed_at = NULL, workspace_id = COALESCE(home_workspace_id, 'daily'), updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND deleted_at IS NULL",
            params![updated_at, id, expected_version],
        )?;
        drop(connection);
        self.get_task(id)?
            .ok_or_else(|| CoreError::TaskNotFound(id.to_owned()))
    }

    pub fn list_blocks(&self, task_id: &str, include_deleted: bool) -> Result<Vec<TaskBlock>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let sql = if include_deleted {
            "SELECT id, task_id, started_at, ended_at, reason, note, resolution_reason, created_at, updated_at, version, deleted_at FROM task_blocks WHERE task_id = ?1 ORDER BY started_at"
        } else {
            "SELECT id, task_id, started_at, ended_at, reason, note, resolution_reason, created_at, updated_at, version, deleted_at FROM task_blocks WHERE task_id = ?1 AND deleted_at IS NULL ORDER BY started_at"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map(params![task_id], map_block)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_all_blocks(&self) -> Result<Vec<TaskBlock>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT id, task_id, started_at, ended_at, reason, note, resolution_reason, created_at, updated_at, version, deleted_at FROM task_blocks ORDER BY started_at, id",
        )?;
        let rows = statement.query_map([], map_block)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_all_sessions(&self) -> Result<Vec<WorkSession>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT id, task_id, started_at, ended_at, note, created_at FROM work_sessions ORDER BY started_at, id",
        )?;
        let rows = statement.query_map([], map_session)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn list_sessions(&self, task_id: &str) -> Result<Vec<WorkSession>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare("SELECT id, task_id, started_at, ended_at, note, created_at FROM work_sessions WHERE task_id = ?1 ORDER BY started_at")?;
        let rows = statement.query_map(params![task_id], map_session)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub(crate) fn correct_work_time_at(
        &self,
        task_id: &str,
        expected_version: i64,
        target_active_minutes: i64,
        corrected_at: impl Into<String>,
    ) -> Result<Task> {
        if target_active_minutes <= 0 {
            return Err(CoreError::InvalidInput(
                "目标进行时长必须大于 0 分钟".into(),
            ));
        }

        let corrected_at = corrected_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let tx = connection.unchecked_transaction()?;
        let task =
            load_task(&tx, task_id)?.ok_or_else(|| CoreError::TaskNotFound(task_id.to_owned()))?;
        if task.deleted_at.is_some() {
            return Err(CoreError::InvalidState("已删除任务不能修改".into()));
        }
        if task.version != expected_version {
            return Err(CoreError::VersionConflict);
        }

        let sessions = query_sessions_for_correction(&tx, task_id)?;
        let blocks = query_blocks_for_correction(&tx, task_id)?;
        let active_block = blocks.iter().find(|block| block.ended_at.is_none());

        let upper_bound = if task.status == TaskStatus::Completed {
            task.completed_at
                .as_deref()
                .ok_or_else(|| CoreError::InvalidState("已完成任务缺少完成时间".into()))?
                .to_owned()
        } else if let Some(block) = active_block {
            block.started_at.clone()
        } else {
            corrected_at.clone()
        };

        let lower_bound = task.created_at.clone();
        let upper = parse_timestamp(&upper_bound)?;
        let lower = parse_timestamp(&lower_bound)?;
        if upper <= lower {
            return Err(CoreError::InvalidState(
                "任务时间边界无效，无法修正进行时长".into(),
            ));
        }

        let hard_blocks = clipped_blocks(&blocks, lower, upper)?;
        let windows = available_windows(lower, upper, &hard_blocks);
        let target = Duration::try_minutes(target_active_minutes)
            .ok_or_else(|| CoreError::InvalidInput("目标进行时长超出可表示范围".into()))?;
        let segments = latest_segments(&windows, target)?;

        let in_progress = task.status == TaskStatus::InProgress && !task.is_blocked;
        let desired_count = segments.len() + usize::from(in_progress);
        if desired_count == 0 {
            return Err(CoreError::InvalidState("没有可用的进行时间区间".into()));
        }

        if sessions.len() > desired_count {
            let extra_ids: Vec<&str> = sessions[desired_count..]
                .iter()
                .map(|session| session.id.as_str())
                .collect();
            for id in extra_ids {
                tx.execute("DELETE FROM work_sessions WHERE id = ?1", params![id])?;
            }
        }

        let reuse_count = sessions.len().min(desired_count);
        for index in 0..reuse_count {
            let session = &sessions[index];
            if index < segments.len() {
                let segment = segments[index];
                tx.execute(
                    "UPDATE work_sessions SET started_at = ?1, ended_at = ?2 WHERE id = ?3",
                    params![
                        format_timestamp(segment.0),
                        format_timestamp(segment.1),
                        session.id
                    ],
                )?;
            } else {
                tx.execute(
                    "UPDATE work_sessions SET started_at = ?1, ended_at = NULL WHERE id = ?2",
                    params![format_timestamp(upper), session.id],
                )?;
            }
        }

        if sessions.len() < desired_count {
            for index in sessions.len()..desired_count {
                let id = Uuid::new_v4().to_string();
                if index < segments.len() {
                    let segment = segments[index];
                    tx.execute(
                        "INSERT INTO work_sessions (id, task_id, started_at, ended_at, note, created_at) VALUES (?1, ?2, ?3, ?4, NULL, ?5)",
                        params![id, task_id, format_timestamp(segment.0), format_timestamp(segment.1), format_timestamp(upper)],
                    )?;
                } else {
                    tx.execute(
                        "INSERT INTO work_sessions (id, task_id, started_at, ended_at, note, created_at) VALUES (?1, ?2, ?3, NULL, NULL, ?4)",
                        params![id, task_id, format_timestamp(upper), format_timestamp(upper)],
                    )?;
                }
            }
        }

        let changed = tx.execute(
            "UPDATE tasks SET updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND deleted_at IS NULL",
            params![corrected_at, task_id, expected_version],
        )?;
        if changed == 0 {
            return Err(CoreError::VersionConflict);
        }

        tx.commit()?;
        drop(connection);
        self.get_task(task_id)?
            .ok_or_else(|| CoreError::TaskNotFound(task_id.to_owned()))
    }

    pub fn apply_sync_snapshot(&self, snapshot: crate::sync::SyncSnapshot) -> Result<()> {
        crate::sync::ensure_valid_snapshot(&snapshot)?;
        let mut normalized = crate::sync::SyncSnapshot {
            workspaces: snapshot.workspaces,
            priorities: snapshot.priorities,
            tasks: snapshot.tasks,
            task_blocks: snapshot.task_blocks,
            work_sessions: snapshot.work_sessions,
        };
        // normalize_snapshot is crate-private through a public wrapper below.
        crate::sync::normalize_for_storage(&mut normalized);
        // SQLite checks partial unique indexes while probing an upsert. Apply
        // already-ended rows first, then the single remaining active row.
        normalized
            .task_blocks
            .sort_by_key(|block| block.ended_at.is_none());
        normalized
            .work_sessions
            .sort_by_key(|session| session.ended_at.is_none());
        let connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.unchecked_transaction()?;
        // `snapshot` is the complete union selected by the sync merge, not a
        // partial patch. Replacing rows removes migration-only seed records on
        // a newly initialized device, while preserving all real local and
        // remote rows because both sides were included in that union.
        transaction.execute("DELETE FROM work_sessions", [])?;
        transaction.execute("DELETE FROM task_blocks", [])?;
        transaction.execute("DELETE FROM tasks", [])?;
        transaction.execute("DELETE FROM priorities", [])?;
        transaction.execute("DELETE FROM workspaces", [])?;
        for workspace in &normalized.workspaces {
            transaction.execute(
                "INSERT INTO workspaces (id, name, sort_order, builtin, created_at, updated_at, deleted_at, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT(id) DO UPDATE SET name=excluded.name, sort_order=excluded.sort_order, builtin=excluded.builtin, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, version=excluded.version",
                params![workspace.id, workspace.name, workspace.sort_order, workspace.builtin, workspace.created_at, workspace.updated_at, workspace.deleted_at, workspace.version],
            )?;
        }
        for priority in &normalized.priorities {
            transaction.execute(
                "INSERT INTO priorities (id, workspace_id, name, color, sort_order, created_at, updated_at, deleted_at, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) ON CONFLICT(id) DO UPDATE SET workspace_id=excluded.workspace_id, name=excluded.name, color=excluded.color, sort_order=excluded.sort_order, created_at=excluded.created_at, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, version=excluded.version",
                params![priority.id, priority.workspace_id, priority.name, priority.color, priority.sort_order, priority.created_at, priority.updated_at, priority.deleted_at, priority.version],
            )?;
        }
        for task in &normalized.tasks {
            transaction.execute(
                "INSERT INTO tasks (id, title, notes, review_notes, estimated_active_minutes, created_at, started_at, completed_at, status, sort_order, created_device_id, updated_at, deleted_at, version, workspace_id, priority_id, home_workspace_id, due_date) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18) ON CONFLICT(id) DO UPDATE SET title=excluded.title, notes=excluded.notes, review_notes=excluded.review_notes, estimated_active_minutes=excluded.estimated_active_minutes, created_at=excluded.created_at, started_at=excluded.started_at, completed_at=excluded.completed_at, status=excluded.status, sort_order=excluded.sort_order, created_device_id=excluded.created_device_id, updated_at=excluded.updated_at, deleted_at=excluded.deleted_at, version=excluded.version, workspace_id=excluded.workspace_id, priority_id=excluded.priority_id, home_workspace_id=excluded.home_workspace_id, due_date=excluded.due_date",
                params![task.id, task.title, task.notes, task.review_notes, task.estimated_active_minutes, task.created_at, task.started_at, task.completed_at, task.status.as_str(), task.sort_order, task.created_device_id, task.updated_at, task.deleted_at, task.version, task.workspace_id, task.priority_id, task.home_workspace_id, task.due_date],
            )?;
        }
        for block in &normalized.task_blocks {
            transaction.execute(
                "INSERT INTO task_blocks (id, task_id, started_at, ended_at, reason, note, resolution_reason, created_at, updated_at, version, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11) ON CONFLICT(id) DO UPDATE SET task_id=excluded.task_id, started_at=excluded.started_at, ended_at=excluded.ended_at, reason=excluded.reason, note=excluded.note, resolution_reason=excluded.resolution_reason, created_at=excluded.created_at, updated_at=excluded.updated_at, version=excluded.version, deleted_at=excluded.deleted_at",
                params![block.id, block.task_id, block.started_at, block.ended_at, block.reason, block.note, block.resolution_reason, block.created_at, block.updated_at, block.version, block.deleted_at],
            )?;
        }
        for session in &normalized.work_sessions {
            transaction.execute(
                "INSERT INTO work_sessions (id, task_id, started_at, ended_at, note, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(id) DO UPDATE SET task_id=excluded.task_id, started_at=excluded.started_at, ended_at=excluded.ended_at, note=excluded.note, created_at=excluded.created_at",
                params![session.id, session.task_id, session.started_at, session.ended_at, session.note, session.created_at],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    // ===== 工作区 =====
    pub fn create_workspace(&self, name: &str, created_at: impl Into<String>) -> Result<Workspace> {
        if name.trim().is_empty() {
            return Err(CoreError::InvalidInput("工作区名称不能为空".into()));
        }
        let created_at = created_at.into();
        let id = Uuid::new_v4().to_string();
        let sort_order = self.next_workspace_sort_order()?;
        let connection = self.connection.lock().expect("database mutex poisoned");
        let tx = connection.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO workspaces (id, name, sort_order, builtin, created_at, updated_at) VALUES (?1, ?2, ?3, 0, ?4, ?4)",
            params![id, name.trim(), sort_order, created_at],
        )?;
        for (name, color, sort_order) in [
            ("P0", "#b0432f", 0_i64),
            ("P1", "#b16d42", 1_i64),
            ("P2", "#8f9a90", 2_i64),
        ] {
            tx.execute(
                "INSERT INTO priorities (id, workspace_id, name, color, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![Uuid::new_v4().to_string(), id, name, color, sort_order, created_at],
            )?;
        }
        tx.commit()?;
        drop(connection);
        self.get_workspace(&id)?
            .ok_or_else(|| CoreError::WorkspaceNotFound(id))
    }

    fn next_workspace_sort_order(&self) -> Result<i64> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let max = connection
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) FROM workspaces WHERE deleted_at IS NULL AND id <> 'done'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(-1);
        Ok(max + 1)
    }

    pub fn list_workspaces(&self, include_deleted: bool) -> Result<Vec<Workspace>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let sql = if include_deleted {
            "SELECT id, name, sort_order, builtin, created_at, updated_at, deleted_at, version FROM workspaces ORDER BY CASE WHEN id = 'done' THEN 1 ELSE 0 END, sort_order, created_at"
        } else {
            "SELECT id, name, sort_order, builtin, created_at, updated_at, deleted_at, version FROM workspaces WHERE deleted_at IS NULL ORDER BY CASE WHEN id = 'done' THEN 1 ELSE 0 END, sort_order, created_at"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], map_workspace)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn reorder_workspaces(
        &self,
        ordered_ids: &[String],
        expected_versions: &[i64],
        updated_at: impl Into<String>,
    ) -> Result<Vec<Workspace>> {
        if ordered_ids.len() != expected_versions.len() {
            return Err(CoreError::InvalidInput(
                "工作区排序的 ID 与版本数量不一致".into(),
            ));
        }

        let updated_at = updated_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare(
            "SELECT id, name, sort_order, builtin, created_at, updated_at, deleted_at, version FROM workspaces WHERE deleted_at IS NULL",
        )?;
        let rows = statement.query_map([], map_workspace)?;
        let active_workspaces: Vec<Workspace> = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        drop(statement);

        let active_workspace_map: HashMap<&str, &Workspace> = active_workspaces
            .iter()
            .map(|workspace| (workspace.id.as_str(), workspace))
            .collect();
        let movable_workspace_count = active_workspaces
            .iter()
            .filter(|workspace| !workspace.builtin)
            .count();
        let mut seen_ids = HashSet::with_capacity(ordered_ids.len());
        for (id, expected_version) in ordered_ids.iter().zip(expected_versions) {
            if !seen_ids.insert(id.as_str()) {
                return Err(CoreError::InvalidInput("工作区排序包含重复 ID".into()));
            }
            let workspace = active_workspace_map
                .get(id.as_str())
                .copied()
                .ok_or_else(|| CoreError::WorkspaceNotFound(id.clone()))?;
            if workspace.builtin {
                return Err(CoreError::InvalidState("内置工作区不能参与排序".into()));
            }
            if workspace.version != *expected_version {
                return Err(CoreError::VersionConflict);
            }
        }
        if seen_ids.len() != movable_workspace_count {
            return Err(CoreError::InvalidInput(
                "工作区排序必须包含所有普通工作区".into(),
            ));
        }

        let tx = connection.unchecked_transaction()?;
        for (sort_order, (id, expected_version)) in
            ordered_ids.iter().zip(expected_versions).enumerate()
        {
            let changed = tx.execute(
                "UPDATE workspaces SET sort_order = ?1, updated_at = ?2, version = version + 1 WHERE id = ?3 AND version = ?4 AND deleted_at IS NULL",
                params![sort_order as i64, updated_at, id, expected_version],
            )?;
            if changed == 0 {
                return Err(CoreError::VersionConflict);
            }
        }
        tx.commit()?;
        drop(connection);
        self.list_workspaces(false)
    }

    pub fn get_workspace(&self, id: &str) -> Result<Option<Workspace>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection
            .query_row(
                "SELECT id, name, sort_order, builtin, created_at, updated_at, deleted_at, version FROM workspaces WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                map_workspace,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn rename_workspace(
        &self,
        id: &str,
        expected_version: i64,
        name: &str,
        updated_at: impl Into<String>,
    ) -> Result<Workspace> {
        if name.trim().is_empty() {
            return Err(CoreError::InvalidInput("工作区名称不能为空".into()));
        }
        let updated_at = updated_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let changed = connection.execute(
            "UPDATE workspaces SET name = ?1, updated_at = ?2, version = version + 1 WHERE id = ?3 AND version = ?4 AND deleted_at IS NULL",
            params![name.trim(), updated_at, id, expected_version],
        )?;
        if changed == 0 {
            return Err(resolve_workspace_update_error(&connection, id));
        }
        drop(connection);
        self.get_workspace(id)?
            .ok_or_else(|| CoreError::WorkspaceNotFound(id.to_owned()))
    }

    pub fn soft_delete_workspace(
        &self,
        id: &str,
        expected_version: i64,
        deleted_at: impl Into<String>,
    ) -> Result<()> {
        let deleted_at = deleted_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let builtin: bool = connection
            .query_row(
                "SELECT builtin FROM workspaces WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
            .map(|v| v != 0)
            .unwrap_or(false);
        if builtin {
            return Err(CoreError::InvalidState("内置工作区不能删除".into()));
        }
        let task_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM tasks WHERE workspace_id = ?1 AND deleted_at IS NULL",
            params![id],
            |r| r.get(0),
        )?;
        if task_count > 0 {
            return Err(CoreError::InvalidState(
                "工作区还有任务，先移走再删除".into(),
            ));
        }
        let tx = connection.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE workspaces SET deleted_at = ?1, updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND deleted_at IS NULL",
            params![&deleted_at, id, expected_version],
        )?;
        if changed == 0 {
            let error = match tx
                .query_row(
                    "SELECT version FROM workspaces WHERE id = ?1 AND deleted_at IS NULL",
                    params![id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()
            {
                Ok(Some(_)) => CoreError::VersionConflict,
                Ok(None) => CoreError::WorkspaceNotFound(id.to_owned()),
                Err(error) => CoreError::Database(error),
            };
            return Err(error);
        }
        tx.execute(
            "UPDATE priorities SET deleted_at = ?1, updated_at = ?1, version = version + 1 WHERE workspace_id = ?2 AND deleted_at IS NULL",
            params![&deleted_at, id],
        )?;
        tx.commit()?;
        Ok(())
    }

    // ===== 优先级分级 =====
    pub fn create_priority(
        &self,
        workspace_id: &str,
        name: &str,
        color: Option<&str>,
        created_at: impl Into<String>,
    ) -> Result<Priority> {
        if name.trim().is_empty() {
            return Err(CoreError::InvalidInput("分级名称不能为空".into()));
        }
        let created_at = created_at.into();
        self.get_workspace(workspace_id)?
            .ok_or_else(|| CoreError::WorkspaceNotFound(workspace_id.to_owned()))?;
        let id = Uuid::new_v4().to_string();
        let sort_order = self.next_priority_sort_order(workspace_id)?;
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "INSERT INTO priorities (id, workspace_id, name, color, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![id, workspace_id, name.trim(), color, sort_order, created_at],
        )?;
        drop(connection);
        self.get_priority(&id)?
            .ok_or_else(|| CoreError::PriorityNotFound(id))
    }

    fn next_priority_sort_order(&self, workspace_id: &str) -> Result<i64> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let max = connection
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) FROM priorities WHERE workspace_id = ?1 AND deleted_at IS NULL",
                params![workspace_id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(-1);
        Ok(max + 1)
    }

    pub fn list_priorities(&self, include_deleted: bool) -> Result<Vec<Priority>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let sql = if include_deleted {
            "SELECT id, workspace_id, name, color, sort_order, created_at, updated_at, deleted_at, version FROM priorities ORDER BY workspace_id, sort_order, created_at"
        } else {
            "SELECT id, workspace_id, name, color, sort_order, created_at, updated_at, deleted_at, version FROM priorities WHERE deleted_at IS NULL ORDER BY workspace_id, sort_order, created_at"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], map_priority)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn get_priority(&self, id: &str) -> Result<Option<Priority>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection
            .query_row(
                "SELECT id, workspace_id, name, color, sort_order, created_at, updated_at, deleted_at, version FROM priorities WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                map_priority,
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn update_priority(
        &self,
        id: &str,
        expected_version: i64,
        name: &str,
        color: Option<&str>,
        updated_at: impl Into<String>,
    ) -> Result<Priority> {
        if name.trim().is_empty() {
            return Err(CoreError::InvalidInput("分级名称不能为空".into()));
        }
        let updated_at = updated_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let changed = connection.execute(
            "UPDATE priorities SET name = ?1, color = ?2, updated_at = ?3, version = version + 1 WHERE id = ?4 AND version = ?5 AND deleted_at IS NULL",
            params![name.trim(), color, updated_at, id, expected_version],
        )?;
        if changed == 0 {
            return Err(resolve_priority_update_error(&connection, id));
        }
        drop(connection);
        self.get_priority(id)?
            .ok_or_else(|| CoreError::PriorityNotFound(id.to_owned()))
    }

    pub fn soft_delete_priority(
        &self,
        id: &str,
        expected_version: i64,
        deleted_at: impl Into<String>,
    ) -> Result<()> {
        let deleted_at = deleted_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let task_count: i64 = connection.query_row(
            "SELECT COUNT(*) FROM tasks WHERE priority_id = ?1 AND deleted_at IS NULL",
            params![id],
            |r| r.get(0),
        )?;
        if task_count > 0 {
            return Err(CoreError::InvalidState(
                "该分级还有任务，先移走再删除".into(),
            ));
        }
        let workspace_id: String = connection.query_row(
            "SELECT workspace_id FROM priorities WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            |r| r.get(0),
        )?;
        let remaining: i64 = connection.query_row(
            "SELECT COUNT(*) FROM priorities WHERE workspace_id = ?1 AND id <> ?2 AND deleted_at IS NULL",
            params![workspace_id, id],
            |r| r.get(0),
        )?;
        if remaining == 0 {
            return Err(CoreError::InvalidState(
                "每个工作区至少要保留一个分级".into(),
            ));
        }
        let changed = connection.execute(
            "UPDATE priorities SET deleted_at = ?1, updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND deleted_at IS NULL",
            params![deleted_at, id, expected_version],
        )?;
        if changed == 0 {
            return Err(resolve_priority_update_error(&connection, id));
        }
        Ok(())
    }

    fn get_block(&self, id: &str) -> Result<Option<TaskBlock>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection
            .query_row(BLOCK_SELECT_SQL, params![id], map_block)
            .optional()
            .map_err(Into::into)
    }

    fn get_session(&self, id: &str) -> Result<Option<WorkSession>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection
            .query_row(SESSION_SELECT_SQL, params![id], map_session)
            .optional()
            .map_err(Into::into)
    }
}

const TASK_BY_ID_SQL: &str = "SELECT t.id, t.title, t.notes, t.review_notes, t.estimated_active_minutes, t.due_date, t.created_at, t.started_at, t.completed_at, t.status, t.sort_order, t.created_device_id, t.updated_at, t.deleted_at, t.version, EXISTS (SELECT 1 FROM task_blocks b WHERE b.task_id = t.id AND b.ended_at IS NULL AND b.deleted_at IS NULL), t.workspace_id, t.priority_id, t.home_workspace_id FROM tasks t WHERE t.id = ?1";
const TASK_SELECT_SQL: &str = "SELECT t.id, t.title, t.notes, t.review_notes, t.estimated_active_minutes, t.due_date, t.created_at, t.started_at, t.completed_at, t.status, t.sort_order, t.created_device_id, t.updated_at, t.deleted_at, t.version, EXISTS (SELECT 1 FROM task_blocks b WHERE b.task_id = t.id AND b.ended_at IS NULL AND b.deleted_at IS NULL), t.workspace_id, t.priority_id, t.home_workspace_id FROM tasks t ORDER BY t.sort_order, t.created_at";
const BLOCK_SELECT_SQL: &str = "SELECT id, task_id, started_at, ended_at, reason, note, resolution_reason, created_at, updated_at, version, deleted_at FROM task_blocks WHERE id = ?1";
const SESSION_SELECT_SQL: &str =
    "SELECT id, task_id, started_at, ended_at, note, created_at FROM work_sessions WHERE id = ?1";

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn validate_new_task(input: &NewTask) -> Result<()> {
    validate_task_fields(&input.title, input.estimated_active_minutes)?;
    validate_due_date(input.due_date.as_deref())?;
    if input.created_device_id.trim().is_empty() {
        return Err(CoreError::InvalidInput("设备 ID 不能为空".into()));
    }
    Ok(())
}
fn validate_task_fields(title: &str, estimated: Option<i64>) -> Result<()> {
    if title.trim().is_empty() {
        return Err(CoreError::InvalidInput("任务标题不能为空".into()));
    }
    if estimated.is_some_and(|v| v < 0) {
        return Err(CoreError::InvalidInput("预计分钟数不能为负数".into()));
    }
    Ok(())
}
fn validate_due_date(due_date: Option<&str>) -> Result<()> {
    if let Some(value) = due_date {
        NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map_err(|_| CoreError::InvalidInput("预计完成日期必须是 YYYY-MM-DD".into()))?;
    }
    Ok(())
}

fn validate_status_change(task: &Task, status: TaskStatus) -> Result<()> {
    if task.deleted_at.is_some() {
        return Err(CoreError::InvalidState("已删除任务不能修改".into()));
    }
    if task.status == TaskStatus::Completed && status != TaskStatus::Completed {
        return Err(CoreError::InvalidState("已完成任务不能重新打开".into()));
    }
    Ok(())
}
fn validate_can_start_work(task: &Task) -> Result<()> {
    if task.deleted_at.is_some() || task.status == TaskStatus::Completed {
        return Err(CoreError::InvalidState("该任务不能开始工作".into()));
    }
    if task.is_blocked {
        return Err(CoreError::InvalidState("该任务当前被阻塞".into()));
    }
    Ok(())
}
fn validate_can_start_block(task: &Task) -> Result<()> {
    if task.deleted_at.is_some() || task.status == TaskStatus::Completed {
        return Err(CoreError::InvalidState("该任务不能被阻塞".into()));
    }
    if task.is_blocked {
        return Err(CoreError::ActiveBlockExists);
    }
    Ok(())
}

fn map_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        title: row.get(1)?,
        notes: row.get(2)?,
        review_notes: row.get(3)?,
        estimated_active_minutes: row.get(4)?,
        due_date: row.get(5)?,
        created_at: row.get(6)?,
        started_at: row.get(7)?,
        completed_at: row.get(8)?,
        status: TaskStatus::parse(&row.get::<_, String>(9)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
        })?,
        sort_order: row.get(10)?,
        created_device_id: row.get(11)?,
        updated_at: row.get(12)?,
        deleted_at: row.get(13)?,
        version: row.get(14)?,
        is_blocked: row.get(15)?,
        workspace_id: row.get(16)?,
        priority_id: row.get(17)?,
        home_workspace_id: row.get(18)?,
    })
}
fn map_block(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskBlock> {
    Ok(TaskBlock {
        id: row.get(0)?,
        task_id: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        reason: row.get(4)?,
        note: row.get(5)?,
        resolution_reason: row.get(6)?,
        created_at: row.get(7)?,
        updated_at: row.get(8)?,
        version: row.get(9)?,
        deleted_at: row.get(10)?,
    })
}
fn query_sessions_for_correction(
    connection: &Connection,
    task_id: &str,
) -> Result<Vec<WorkSession>> {
    let mut statement = connection.prepare(
        "SELECT id, task_id, started_at, ended_at, note, created_at FROM work_sessions WHERE task_id = ?1 ORDER BY started_at",
    )?;
    let rows = statement.query_map(params![task_id], map_session)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn query_blocks_for_correction(connection: &Connection, task_id: &str) -> Result<Vec<TaskBlock>> {
    let mut statement = connection.prepare(
        "SELECT id, task_id, started_at, ended_at, reason, note, resolution_reason, created_at, updated_at, version, deleted_at FROM task_blocks WHERE task_id = ?1 AND deleted_at IS NULL ORDER BY started_at",
    )?;
    let rows = statement.query_map(params![task_id], map_block)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Into::into)
}

fn parse_timestamp(value: &str) -> Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|_| CoreError::InvalidState("时间格式无效".into()))
}

fn format_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn clipped_blocks(
    blocks: &[TaskBlock],
    lower: DateTime<Utc>,
    upper: DateTime<Utc>,
) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
    let mut clipped: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for block in blocks {
        let start = parse_timestamp(&block.started_at)?;
        let end = block
            .ended_at
            .as_deref()
            .map(parse_timestamp)
            .transpose()?
            .unwrap_or(upper);
        let start = start.max(lower);
        let end = end.min(upper);
        if start < end {
            clipped.push((start, end));
        }
    }
    clipped.sort_by_key(|(start, _)| *start);

    let mut merged: Vec<(DateTime<Utc>, DateTime<Utc>)> = Vec::new();
    for (start, end) in clipped {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    Ok(merged)
}

fn available_windows(
    lower: DateTime<Utc>,
    upper: DateTime<Utc>,
    blocks: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut windows = Vec::new();
    let mut cursor = lower;
    for (start, end) in blocks {
        if *start > cursor {
            windows.push((cursor, *start));
        }
        cursor = cursor.max(*end);
    }
    if cursor < upper {
        windows.push((cursor, upper));
    }
    windows
}

fn latest_segments(
    windows: &[(DateTime<Utc>, DateTime<Utc>)],
    target: Duration,
) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
    let mut selected = Vec::new();
    let mut remaining = target;
    for (start, end) in windows.iter().rev() {
        let available = *end - *start;
        if remaining <= available {
            selected.push((*end - remaining, *end));
            remaining = Duration::zero();
            break;
        }
        selected.push((*start, *end));
        remaining = remaining - available;
    }
    if remaining > Duration::zero() {
        return Err(CoreError::InvalidInput(
            "目标进行时长超过可用时间范围".into(),
        ));
    }
    selected.reverse();
    Ok(selected)
}

fn map_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkSession> {
    Ok(WorkSession {
        id: row.get(0)?,
        task_id: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        note: row.get(4)?,
        created_at: row.get(5)?,
    })
}

trait QueryConnection {
    fn query_task(&self, id: &str) -> rusqlite::Result<Option<Task>>;
}

impl QueryConnection for Connection {
    fn query_task(&self, id: &str) -> rusqlite::Result<Option<Task>> {
        self.query_row(TASK_BY_ID_SQL, params![id], map_task)
            .optional()
    }
}

impl<'a> QueryConnection for rusqlite::Transaction<'a> {
    fn query_task(&self, id: &str) -> rusqlite::Result<Option<Task>> {
        self.query_row(TASK_BY_ID_SQL, params![id], map_task)
            .optional()
    }
}

fn load_task<C: QueryConnection>(connection: &C, id: &str) -> Result<Option<Task>> {
    connection.query_task(id).map_err(Into::into)
}

fn resolve_task_update_error(connection: &Connection, id: &str) -> CoreError {
    match connection
        .query_row(
            "SELECT version FROM tasks WHERE id = ?1",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .optional()
    {
        Ok(Some(_)) => CoreError::VersionConflict,
        Ok(None) => CoreError::TaskNotFound(id.to_owned()),
        Err(e) => CoreError::Database(e),
    }
}
fn resolve_session_update_error(connection: &Connection, id: &str) -> CoreError {
    match connection
        .query_row(
            "SELECT id FROM work_sessions WHERE id = ?1",
            params![id],
            |r| r.get::<_, String>(0),
        )
        .optional()
    {
        Ok(Some(_)) => CoreError::InvalidState("工作会话已经结束".into()),
        Ok(None) => CoreError::SessionNotFound(id.to_owned()),
        Err(e) => CoreError::Database(e),
    }
}
fn resolve_block_update_error(
    connection: &Connection,
    id: &str,
    expected_version: i64,
) -> CoreError {
    match connection
        .query_row(
            "SELECT version, ended_at FROM task_blocks WHERE id = ?1",
            params![id],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?)),
        )
        .optional()
    {
        Ok(Some((version, _))) if version != expected_version => CoreError::VersionConflict,
        Ok(Some((_version, Some(_)))) => CoreError::InvalidState("阻塞已经解除".into()),
        Ok(Some((_version, None))) => CoreError::InvalidState("阻塞记录不可更新".into()),
        Ok(None) => CoreError::BlockNotFound(id.to_owned()),
        Err(e) => CoreError::Database(e),
    }
}

fn map_workspace(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    Ok(Workspace {
        id: row.get(0)?,
        name: row.get(1)?,
        sort_order: row.get(2)?,
        builtin: row.get::<_, i64>(3)? != 0,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        deleted_at: row.get(6)?,
        version: row.get(7)?,
    })
}
fn map_priority(row: &rusqlite::Row<'_>) -> rusqlite::Result<Priority> {
    Ok(Priority {
        id: row.get(0)?,
        workspace_id: row.get(1)?,
        name: row.get(2)?,
        color: row.get(3)?,
        sort_order: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
        deleted_at: row.get(7)?,
        version: row.get(8)?,
    })
}
fn resolve_workspace_update_error(connection: &Connection, id: &str) -> CoreError {
    match connection
        .query_row(
            "SELECT version FROM workspaces WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .optional()
    {
        Ok(Some(_)) => CoreError::VersionConflict,
        Ok(None) => CoreError::WorkspaceNotFound(id.to_owned()),
        Err(e) => CoreError::Database(e),
    }
}
fn resolve_priority_update_error(connection: &Connection, id: &str) -> CoreError {
    match connection
        .query_row(
            "SELECT version FROM priorities WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            |r| r.get::<_, i64>(0),
        )
        .optional()
    {
        Ok(Some(_)) => CoreError::VersionConflict,
        Ok(None) => CoreError::PriorityNotFound(id.to_owned()),
        Err(e) => CoreError::Database(e),
    }
}
fn map_constraint_error(error: rusqlite::Error) -> CoreError {
    if let rusqlite::Error::SqliteFailure(_, Some(message)) = &error {
        if message.contains("ux_task_blocks_one_active") {
            return CoreError::ActiveBlockExists;
        }
        if message.contains("ux_work_sessions_one_active") {
            return CoreError::ActiveSessionExists;
        }
    }
    CoreError::Database(error)
}
