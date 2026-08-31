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
            sort_order: command.sort_order,
            created_device_id: command.created_device_id,
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
            command.sort_order,
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

    pub fn unblock(&self, block_id: &str, expected_version: i64) -> Result<TaskBlock> {
        self.store.end_block(block_id, expected_version, now())
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
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateTaskCommand {
    pub title: String,
    pub notes: Option<String>,
    pub estimated_active_minutes: Option<i64>,
    #[serde(default)]
    pub sort_order: i64,
    pub created_device_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UpdateTaskCommand {
    pub title: String,
    pub notes: Option<String>,
    pub review_notes: Option<String>,
    pub estimated_active_minutes: Option<i64>,
    #[serde(default)]
    pub sort_order: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlockTaskCommand {
    pub reason: String,
    pub note: Option<String>,
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}
