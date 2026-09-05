use crate::{domain::*, error::*, persistence::TaskStore};
use chrono::{SecondsFormat, Utc};

/// 面向 GUI、HTTP API、CLI 的稳定业务门面。
/// 适配层只需要把自己的请求 DTO 转换成这里的命令，不需要了解 SQLite。
pub struct TaskService {
    store: TaskStore,
}

impl TaskService {
    pub fn new(store: TaskStore) -> Self {
        Self { store }
    }
    pub fn store(&self) -> &TaskStore {
        &self.store
    }

    pub fn create(&self, command: CreateTaskCommand) -> Result<Task> {
        self.store.create_task(NewTask {
            title: command.title,
            notes: command.notes,
            estimated_active_minutes: command.estimated_active_minutes,
            due_date: command.due_date,
            sort_order: command.sort_order,
            created_device_id: command.created_device_id,
            workspace_id: command.workspace_id,
            priority_id: command.priority_id,
        })
    }

    pub fn update(
        &self,
        id: &str,
        expected_version: i64,
        command: UpdateTaskCommand,
    ) -> Result<Task> {
        self.store.update_task(
            id,
            expected_version,
            &command.title,
            command.notes.as_deref(),
            command.review_notes.as_deref(),
            command.estimated_active_minutes,
            command.due_date.as_deref(),
            command.sort_order,
            command.workspace_id.as_deref(),
            command.priority_id.as_deref(),
            now(),
        )
    }

    pub fn start(&self, id: &str, expected_version: i64) -> Result<Task> {
        self.store
            .set_status(id, expected_version, TaskStatus::InProgress, now())
    }

    pub fn complete(&self, id: &str, expected_version: i64) -> Result<Task> {
        self.store
            .set_status(id, expected_version, TaskStatus::Completed, now())
    }

    pub fn reopen(&self, id: &str, expected_version: i64) -> Result<Task> {
        self.store.reopen_task(id, expected_version, now())
    }

    /// 暂停：结束计时语义由适配层先调用 finish_work，这里把任务退回待处理。
    pub fn pause(&self, id: &str, expected_version: i64) -> Result<Task> {
        self.store
            .set_status(id, expected_version, TaskStatus::Pending, now())
    }

    pub fn delete(&self, id: &str, expected_version: i64) -> Result<()> {
        self.store.soft_delete_task(id, expected_version, now())
    }

    pub fn begin_work(&self, task_id: &str, note: Option<&str>) -> Result<WorkSession> {
        self.store.start_work(task_id, now(), note)
    }

    pub fn finish_work(&self, session_id: &str) -> Result<WorkSession> {
        self.store.end_work(session_id, now())
    }

    pub fn block(&self, task_id: &str, command: BlockTaskCommand) -> Result<TaskBlock> {
        self.store
            .start_block(task_id, &command.reason, command.note.as_deref(), now())
    }

    pub fn unblock(
        &self,
        block_id: &str,
        expected_version: i64,
        resolution_reason: Option<&str>,
    ) -> Result<TaskBlock> {
        let resolution_reason = resolution_reason
            .map(str::trim)
            .filter(|value| !value.is_empty());
        self.store
            .end_block(block_id, expected_version, resolution_reason, now())
    }

    pub fn get(&self, id: &str) -> Result<Option<Task>> {
        self.store.get_task(id)
    }
    pub fn list(&self, include_deleted: bool) -> Result<Vec<Task>> {
        self.store.list_tasks(include_deleted)
    }
    pub fn blocks(&self, task_id: &str, include_deleted: bool) -> Result<Vec<TaskBlock>> {
        self.store.list_blocks(task_id, include_deleted)
    }
    pub fn sessions(&self, task_id: &str) -> Result<Vec<WorkSession>> {
        self.store.list_sessions(task_id)
    }

    // ===== 工作区 =====
    pub fn create_workspace(&self, name: &str) -> Result<Workspace> {
        self.store.create_workspace(name, now())
    }
    pub fn workspaces(&self, include_deleted: bool) -> Result<Vec<Workspace>> {
        self.store.list_workspaces(include_deleted)
    }
    pub fn workspace(&self, id: &str) -> Result<Option<Workspace>> {
        self.store.get_workspace(id)
    }
    pub fn rename_workspace(
        &self,
        id: &str,
        expected_version: i64,
        name: &str,
    ) -> Result<Workspace> {
        self.store
            .rename_workspace(id, expected_version, name, now())
    }
    pub fn delete_workspace(&self, id: &str, expected_version: i64) -> Result<()> {
        self.store
            .soft_delete_workspace(id, expected_version, now())
    }

    // ===== 优先级分级 =====
    pub fn create_priority(
        &self,
        workspace_id: &str,
        name: &str,
        color: Option<&str>,
    ) -> Result<Priority> {
        self.store.create_priority(workspace_id, name, color, now())
    }
    pub fn priorities(&self, include_deleted: bool) -> Result<Vec<Priority>> {
        self.store.list_priorities(include_deleted)
    }
    pub fn priority(&self, id: &str) -> Result<Option<Priority>> {
        self.store.get_priority(id)
    }
    pub fn update_priority(
        &self,
        id: &str,
        expected_version: i64,
        name: &str,
        color: Option<&str>,
    ) -> Result<Priority> {
        self.store
            .update_priority(id, expected_version, name, color, now())
    }
    pub fn delete_priority(&self, id: &str, expected_version: i64) -> Result<()> {
        self.store.soft_delete_priority(id, expected_version, now())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateTaskCommand {
    pub title: String,
    pub notes: Option<String>,
    pub estimated_active_minutes: Option<i64>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    pub created_device_id: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub priority_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateTaskCommand {
    pub title: String,
    pub notes: Option<String>,
    pub review_notes: Option<String>,
    pub estimated_active_minutes: Option<i64>,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub priority_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlockTaskCommand {
    pub reason: String,
    pub note: Option<String>,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
