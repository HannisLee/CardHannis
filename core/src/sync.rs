use crate::{
    domain::{Priority, Task, TaskBlock, TaskStatus, WorkSession, Workspace},
    error::{CoreError, Result},
};
use chrono::{DateTime, Utc};
use std::collections::HashMap;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct SyncSnapshot {
    pub workspaces: Vec<Workspace>,
    pub priorities: Vec<Priority>,
    pub tasks: Vec<Task>,
    pub task_blocks: Vec<TaskBlock>,
    pub work_sessions: Vec<WorkSession>,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct SyncCounts {
    pub workspaces: usize,
    pub priorities: usize,
    pub tasks: usize,
    pub task_blocks: usize,
    pub work_sessions: usize,
}

impl SyncSnapshot {
    pub fn counts(&self) -> SyncCounts {
        SyncCounts {
            workspaces: self.workspaces.len(),
            priorities: self.priorities.len(),
            tasks: self.tasks.len(),
            task_blocks: self.task_blocks.len(),
            work_sessions: self.work_sessions.len(),
        }
    }

    /// `TaskStore` 的迁移会创建固定的工作区和分级。它们不是用户创建的
    /// 数据，因此刚初始化的设备首次连接到已有远端时不能把这些种子上传。
    pub fn is_bootstrap_seed(&self) -> bool {
        const SEED_TIME: &str = "1970-01-01T00:00:00.000Z";

        self.tasks.is_empty()
            && self.task_blocks.is_empty()
            && self.work_sessions.is_empty()
            && !self.workspaces.is_empty()
            && self.workspaces.iter().all(|workspace| {
                workspace.created_at == SEED_TIME
                    && workspace.updated_at == SEED_TIME
                    && workspace.deleted_at.is_none()
                    && workspace.version == 1
            })
            && self.priorities.iter().all(|priority| {
                priority.created_at == SEED_TIME
                    && priority.updated_at == SEED_TIME
                    && priority.deleted_at.is_none()
                    && priority.version == 1
            })
    }

    fn has_records(&self) -> bool {
        !self.workspaces.is_empty()
            || !self.priorities.is_empty()
            || !self.tasks.is_empty()
            || !self.task_blocks.is_empty()
            || !self.work_sessions.is_empty()
    }
}

trait SyncRecord {
    fn sync_id(&self) -> &str;
    fn sync_task_id(&self) -> Option<&str>;
    fn sync_started_at(&self) -> &str;
    fn sync_updated_at(&self) -> &str;
    fn sync_version(&self) -> i64;
    fn sync_is_active(&self) -> bool;
    fn sync_close(&mut self, boundary: &str);
}

impl SyncRecord for Workspace {
    fn sync_id(&self) -> &str {
        &self.id
    }
    fn sync_task_id(&self) -> Option<&str> {
        None
    }
    fn sync_started_at(&self) -> &str {
        &self.created_at
    }
    fn sync_updated_at(&self) -> &str {
        &self.updated_at
    }
    fn sync_version(&self) -> i64 {
        self.version
    }
    fn sync_is_active(&self) -> bool {
        false
    }
    fn sync_close(&mut self, _boundary: &str) {}
}

impl SyncRecord for Priority {
    fn sync_id(&self) -> &str {
        &self.id
    }
    fn sync_task_id(&self) -> Option<&str> {
        Some(&self.workspace_id)
    }
    fn sync_started_at(&self) -> &str {
        &self.created_at
    }
    fn sync_updated_at(&self) -> &str {
        &self.updated_at
    }
    fn sync_version(&self) -> i64 {
        self.version
    }
    fn sync_is_active(&self) -> bool {
        false
    }
    fn sync_close(&mut self, _boundary: &str) {}
}

impl SyncRecord for Task {
    fn sync_id(&self) -> &str {
        &self.id
    }
    fn sync_task_id(&self) -> Option<&str> {
        None
    }
    fn sync_started_at(&self) -> &str {
        self.started_at.as_deref().unwrap_or(&self.created_at)
    }
    fn sync_updated_at(&self) -> &str {
        &self.updated_at
    }
    fn sync_version(&self) -> i64 {
        self.version
    }
    fn sync_is_active(&self) -> bool {
        false
    }
    fn sync_close(&mut self, _boundary: &str) {}
}

impl SyncRecord for TaskBlock {
    fn sync_id(&self) -> &str {
        &self.id
    }
    fn sync_task_id(&self) -> Option<&str> {
        Some(&self.task_id)
    }
    fn sync_started_at(&self) -> &str {
        &self.started_at
    }
    fn sync_updated_at(&self) -> &str {
        &self.updated_at
    }
    fn sync_version(&self) -> i64 {
        self.version
    }
    fn sync_is_active(&self) -> bool {
        self.ended_at.is_none() && self.deleted_at.is_none()
    }
    fn sync_close(&mut self, boundary: &str) {
        self.ended_at = Some(boundary.to_owned());
        self.updated_at = later_time(&self.updated_at, boundary);
    }
}

impl SyncRecord for WorkSession {
    fn sync_id(&self) -> &str {
        &self.id
    }
    fn sync_task_id(&self) -> Option<&str> {
        Some(&self.task_id)
    }
    fn sync_started_at(&self) -> &str {
        &self.started_at
    }
    fn sync_updated_at(&self) -> &str {
        &self.created_at
    }
    fn sync_version(&self) -> i64 {
        parse_time(&self.created_at).timestamp()
    }
    fn sync_is_active(&self) -> bool {
        self.ended_at.is_none()
    }
    fn sync_close(&mut self, boundary: &str) {
        self.ended_at = Some(boundary.to_owned());
        self.created_at = later_time(&self.created_at, boundary);
    }
}

pub fn merge_sync_snapshots(local: SyncSnapshot, remote: SyncSnapshot) -> SyncSnapshot {
    // A new local database contains migration seeds such as daily/work/P0.
    // If a remote snapshot already exists, those records are implementation
    // defaults rather than local user intent. Excluding them here makes the
    // first sync a download of the remote state; later syncs remain two-way.
    let local = if local.is_bootstrap_seed() && remote.has_records() {
        SyncSnapshot::default()
    } else {
        local
    };
    let mut merged = SyncSnapshot {
        workspaces: merge_by_id(local.workspaces, remote.workspaces),
        priorities: merge_by_id(local.priorities, remote.priorities),
        tasks: merge_by_id(local.tasks, remote.tasks),
        task_blocks: merge_by_id(local.task_blocks, remote.task_blocks),
        work_sessions: merge_by_id(local.work_sessions, remote.work_sessions),
    };
    normalize_snapshot(&mut merged);
    merged
}

fn merge_by_id<T: SyncRecord + Clone>(local: Vec<T>, remote: Vec<T>) -> Vec<T> {
    let mut merged: HashMap<String, T> = local
        .into_iter()
        .map(|item| (item.sync_id().to_owned(), item))
        .collect();
    for incoming in remote {
        let id = incoming.sync_id().to_owned();
        let replace = merged
            .get(&id)
            .map(|current| record_stamp(&incoming) > record_stamp(current))
            .unwrap_or(true);
        if replace {
            merged.insert(id, incoming);
        }
    }
    merged.into_values().collect()
}

fn record_stamp(record: &impl SyncRecord) -> (DateTime<Utc>, i64, String) {
    (
        parse_time(record.sync_updated_at()),
        record.sync_version(),
        record.sync_id().to_owned(),
    )
}

fn parse_time(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .unwrap_or_else(|_| DateTime::from_timestamp(0, 0).unwrap_or_default())
}

fn later_time(left: &str, right: &str) -> String {
    if parse_time(left) >= parse_time(right) {
        left.to_owned()
    } else {
        right.to_owned()
    }
}

fn normalize_snapshot(snapshot: &mut SyncSnapshot) {
    let tasks: HashMap<String, Task> = snapshot
        .tasks
        .iter()
        .map(|task| (task.id.clone(), task.clone()))
        .collect();

    close_invalid_activity(&mut snapshot.task_blocks, &tasks);
    close_invalid_activity(&mut snapshot.work_sessions, &tasks);
    resolve_active_conflicts(&mut snapshot.task_blocks);
    resolve_active_conflicts(&mut snapshot.work_sessions);

    let blocked_task_ids: Vec<String> = snapshot
        .task_blocks
        .iter()
        .filter(|block| block.ended_at.is_none() && block.deleted_at.is_none())
        .map(|block| block.task_id.clone())
        .collect();
    for task in &mut snapshot.tasks {
        task.is_blocked = blocked_task_ids.contains(&task.id);
    }
}

fn close_invalid_activity<T: SyncRecord>(records: &mut [T], tasks: &HashMap<String, Task>) {
    for record in records.iter_mut() {
        if !record.sync_is_active() {
            continue;
        }
        let Some(task) = tasks.get(record.sync_task_id().unwrap_or_default()) else {
            continue;
        };
        if task.deleted_at.is_none() && task.status != TaskStatus::Completed {
            continue;
        }
        let boundary = task
            .completed_at
            .clone()
            .or_else(|| task.deleted_at.clone())
            .unwrap_or_else(|| task.updated_at.clone());
        record.sync_close(&boundary);
    }
}

fn resolve_active_conflicts<T: SyncRecord>(records: &mut [T]) {
    let mut active: HashMap<String, Vec<usize>> = HashMap::new();
    for (index, record) in records.iter().enumerate() {
        if !record.sync_is_active() {
            continue;
        }
        active
            .entry(record.sync_task_id().unwrap_or_default().to_owned())
            .or_default()
            .push(index);
    }

    for indexes in active.into_values() {
        if indexes.len() <= 1 {
            continue;
        }
        let mut ordered = indexes;
        ordered.sort_by(|left, right| {
            records[*left]
                .sync_started_at()
                .cmp(records[*right].sync_started_at())
                .then_with(|| records[*left].sync_id().cmp(records[*right].sync_id()))
        });
        let winner_started = records[*ordered.last().unwrap()]
            .sync_started_at()
            .to_owned();
        for index in &ordered[..ordered.len() - 1] {
            records[*index].sync_close(&winner_started);
        }
    }
}

pub(crate) fn normalize_for_storage(snapshot: &mut SyncSnapshot) {
    normalize_snapshot(snapshot);
}

pub(crate) fn ensure_valid_snapshot(snapshot: &SyncSnapshot) -> Result<()> {
    for workspace in &snapshot.workspaces {
        if workspace.name.trim().is_empty() || workspace.version <= 0 {
            return Err(CoreError::InvalidInput("同步的工作区数据无效".into()));
        }
    }
    for priority in &snapshot.priorities {
        if priority.name.trim().is_empty() || priority.version <= 0 {
            return Err(CoreError::InvalidInput("同步的分级数据无效".into()));
        }
    }
    for task in &snapshot.tasks {
        if task.title.trim().is_empty()
            || task.version <= 0
            || task
                .estimated_active_minutes
                .is_some_and(|minutes| minutes < 0)
        {
            return Err(CoreError::InvalidInput("同步的任务数据无效".into()));
        }
        if task.status == TaskStatus::Completed && task.completed_at.is_none() {
            return Err(CoreError::InvalidInput(
                "同步的已完成任务缺少完成时间".into(),
            ));
        }
        if task.status != TaskStatus::Completed && task.completed_at.is_some() {
            return Err(CoreError::InvalidInput(
                "同步的未完成任务不能有完成时间".into(),
            ));
        }
    }
    for block in &snapshot.task_blocks {
        if block.reason.trim().is_empty() || block.version <= 0 {
            return Err(CoreError::InvalidInput("同步的阻塞数据无效".into()));
        }
    }
    Ok(())
}
