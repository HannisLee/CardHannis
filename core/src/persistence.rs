use crate::{domain::*, error::*};
use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use std::{path::Path, sync::Mutex};
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
            self.get_priority(priority_id)?
                .ok_or_else(|| CoreError::PriorityNotFound(priority_id.clone()))?;
        }
        let id = Uuid::new_v4().to_string();
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "INSERT INTO tasks (id, title, notes, estimated_active_minutes, created_at, sort_order, created_device_id, updated_at, workspace_id, priority_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?5, ?8, ?9)",
            params![id, input.title.trim(), input.notes, input.estimated_active_minutes, created_at, input.sort_order, input.created_device_id.trim(), input.workspace_id, input.priority_id],
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
            "SELECT t.id, t.title, t.notes, t.review_notes, t.estimated_active_minutes, t.created_at, t.started_at, t.completed_at, t.status, t.sort_order, t.created_device_id, t.updated_at, t.deleted_at, t.version, EXISTS (SELECT 1 FROM task_blocks b WHERE b.task_id = t.id AND b.ended_at IS NULL AND b.deleted_at IS NULL), t.workspace_id, t.priority_id FROM tasks t WHERE t.deleted_at IS NULL ORDER BY t.sort_order, t.created_at"
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
        sort_order: i64,
        workspace_id: Option<&str>,
        priority_id: Option<&str>,
        updated_at: impl Into<String>,
    ) -> Result<Task> {
        validate_task_fields(title, estimated_active_minutes)?;
        let connection = self.connection.lock().expect("database mutex poisoned");
        let changed = connection.execute(
            "UPDATE tasks SET title = ?1, notes = ?2, review_notes = ?3, estimated_active_minutes = ?4, sort_order = ?5, workspace_id = ?9, priority_id = ?10, updated_at = ?6, version = version + 1 WHERE id = ?7 AND version = ?8 AND deleted_at IS NULL",
            params![title.trim(), notes, review_notes, estimated_active_minutes, sort_order, updated_at.into(), id, expected_version, workspace_id, priority_id],
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
        let completed_at = (status == TaskStatus::Completed).then(|| updated_at.clone());
        let started_at = match status {
            TaskStatus::Pending => None,
            TaskStatus::Waiting => task.started_at.clone(),
            TaskStatus::InProgress | TaskStatus::Completed => {
                task.started_at.or(Some(updated_at.clone()))
            }
        };
        connection.execute(
            "UPDATE tasks SET status = ?1, started_at = ?2, completed_at = ?3, updated_at = ?4, version = version + 1 WHERE id = ?5 AND version = ?6 AND deleted_at IS NULL",
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
        if matches!(task.status, TaskStatus::Pending | TaskStatus::Waiting) {
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
            "INSERT INTO task_blocks (id, task_id, started_at, reason, note, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?3, ?3)",
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
        ended_at: impl Into<String>,
    ) -> Result<TaskBlock> {
        let ended_at = ended_at.into();
        let connection = self.connection.lock().expect("database mutex poisoned");
        let tx = connection.unchecked_transaction()?;
        let changed = tx.execute("UPDATE task_blocks SET ended_at = ?1, updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND ended_at IS NULL AND deleted_at IS NULL", params![ended_at, block_id, expected_version])?;
        if changed == 0 {
            let error = resolve_block_update_error(&tx, block_id, expected_version);
            return Err(error);
        }
        // 解除阻塞后任务回到等待中（未完成且未删除时）
        tx.execute(
            "UPDATE tasks SET status = 'waiting', updated_at = ?1, version = version + 1 WHERE id = (SELECT task_id FROM task_blocks WHERE id = ?2) AND deleted_at IS NULL AND status <> 'completed'",
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
            "UPDATE tasks SET status = 'pending', completed_at = NULL, updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND deleted_at IS NULL",
            params![updated_at, id, expected_version],
        )?;
        drop(connection);
        self.get_task(id)?
            .ok_or_else(|| CoreError::TaskNotFound(id.to_owned()))
    }

    pub fn list_blocks(&self, task_id: &str, include_deleted: bool) -> Result<Vec<TaskBlock>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let sql = if include_deleted {
            "SELECT id, task_id, started_at, ended_at, reason, note, created_at, updated_at, version, deleted_at FROM task_blocks WHERE task_id = ?1 ORDER BY started_at"
        } else {
            "SELECT id, task_id, started_at, ended_at, reason, note, created_at, updated_at, version, deleted_at FROM task_blocks WHERE task_id = ?1 AND deleted_at IS NULL ORDER BY started_at"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map(params![task_id], map_block)?;
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

    // ===== 工作区 =====
    pub fn create_workspace(&self, name: &str, created_at: impl Into<String>) -> Result<Workspace> {
        if name.trim().is_empty() {
            return Err(CoreError::InvalidInput("工作区名称不能为空".into()));
        }
        let created_at = created_at.into();
        let id = Uuid::new_v4().to_string();
        let sort_order = self.next_workspace_sort_order()?;
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "INSERT INTO workspaces (id, name, sort_order, builtin, created_at, updated_at) VALUES (?1, ?2, ?3, 0, ?4, ?4)",
            params![id, name.trim(), sort_order, created_at],
        )?;
        drop(connection);
        self.get_workspace(&id)?
            .ok_or_else(|| CoreError::WorkspaceNotFound(id))
    }

    fn next_workspace_sort_order(&self) -> Result<i64> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let max = connection
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) FROM workspaces WHERE deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(-1);
        Ok(max + 1)
    }

    pub fn list_workspaces(&self, include_deleted: bool) -> Result<Vec<Workspace>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let sql = if include_deleted {
            "SELECT id, name, sort_order, builtin, created_at, updated_at, deleted_at, version FROM workspaces ORDER BY sort_order, created_at"
        } else {
            "SELECT id, name, sort_order, builtin, created_at, updated_at, deleted_at, version FROM workspaces WHERE deleted_at IS NULL ORDER BY sort_order, created_at"
        };
        let mut statement = connection.prepare(sql)?;
        let rows = statement.query_map([], map_workspace)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
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
        let changed = connection.execute(
            "UPDATE workspaces SET deleted_at = ?1, updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND deleted_at IS NULL",
            params![deleted_at, id, expected_version],
        )?;
        if changed == 0 {
            return Err(resolve_workspace_update_error(&connection, id));
        }
        Ok(())
    }

    // ===== 优先级分级 =====
    pub fn create_priority(
        &self,
        name: &str,
        color: Option<&str>,
        created_at: impl Into<String>,
    ) -> Result<Priority> {
        if name.trim().is_empty() {
            return Err(CoreError::InvalidInput("分级名称不能为空".into()));
        }
        let created_at = created_at.into();
        let id = Uuid::new_v4().to_string();
        let sort_order = self.next_priority_sort_order()?;
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "INSERT INTO priorities (id, name, color, sort_order, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, name.trim(), color, sort_order, created_at],
        )?;
        drop(connection);
        self.get_priority(&id)?
            .ok_or_else(|| CoreError::PriorityNotFound(id))
    }

    fn next_priority_sort_order(&self) -> Result<i64> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let max = connection
            .query_row(
                "SELECT COALESCE(MAX(sort_order), -1) FROM priorities WHERE deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap_or(-1);
        Ok(max + 1)
    }

    pub fn list_priorities(&self, include_deleted: bool) -> Result<Vec<Priority>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let sql = if include_deleted {
            "SELECT id, name, color, sort_order, created_at, updated_at, deleted_at, version FROM priorities ORDER BY sort_order, created_at"
        } else {
            "SELECT id, name, color, sort_order, created_at, updated_at, deleted_at, version FROM priorities WHERE deleted_at IS NULL ORDER BY sort_order, created_at"
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
                "SELECT id, name, color, sort_order, created_at, updated_at, deleted_at, version FROM priorities WHERE id = ?1 AND deleted_at IS NULL",
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
        let remaining: i64 = connection.query_row(
            "SELECT COUNT(*) FROM priorities WHERE id <> ?1 AND deleted_at IS NULL",
            params![id],
            |r| r.get(0),
        )?;
        if remaining == 0 {
            return Err(CoreError::InvalidState("至少要保留一个分级".into()));
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

const TASK_BY_ID_SQL: &str = "SELECT t.id, t.title, t.notes, t.review_notes, t.estimated_active_minutes, t.created_at, t.started_at, t.completed_at, t.status, t.sort_order, t.created_device_id, t.updated_at, t.deleted_at, t.version, EXISTS (SELECT 1 FROM task_blocks b WHERE b.task_id = t.id AND b.ended_at IS NULL AND b.deleted_at IS NULL), t.workspace_id, t.priority_id FROM tasks t WHERE t.id = ?1";
const TASK_SELECT_SQL: &str = "SELECT t.id, t.title, t.notes, t.review_notes, t.estimated_active_minutes, t.created_at, t.started_at, t.completed_at, t.status, t.sort_order, t.created_device_id, t.updated_at, t.deleted_at, t.version, EXISTS (SELECT 1 FROM task_blocks b WHERE b.task_id = t.id AND b.ended_at IS NULL AND b.deleted_at IS NULL), t.workspace_id, t.priority_id FROM tasks t ORDER BY t.sort_order, t.created_at";
const BLOCK_SELECT_SQL: &str = "SELECT id, task_id, started_at, ended_at, reason, note, created_at, updated_at, version, deleted_at FROM task_blocks WHERE id = ?1";
const SESSION_SELECT_SQL: &str =
    "SELECT id, task_id, started_at, ended_at, note, created_at FROM work_sessions WHERE id = ?1";

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn validate_new_task(input: &NewTask) -> Result<()> {
    validate_task_fields(&input.title, input.estimated_active_minutes)?;
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
        created_at: row.get(5)?,
        started_at: row.get(6)?,
        completed_at: row.get(7)?,
        status: TaskStatus::parse(&row.get::<_, String>(8)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
        })?,
        sort_order: row.get(9)?,
        created_device_id: row.get(10)?,
        updated_at: row.get(11)?,
        deleted_at: row.get(12)?,
        version: row.get(13)?,
        is_blocked: row.get(14)?,
        workspace_id: row.get(15)?,
        priority_id: row.get(16)?,
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
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        version: row.get(8)?,
        deleted_at: row.get(9)?,
    })
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
        name: row.get(1)?,
        color: row.get(2)?,
        sort_order: row.get(3)?,
        created_at: row.get(4)?,
        updated_at: row.get(5)?,
        deleted_at: row.get(6)?,
        version: row.get(7)?,
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
