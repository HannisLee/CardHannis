use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("数据库错误: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("任务不存在: {0}")]
    TaskNotFound(String),
    #[error("工作区不存在: {0}")]
    WorkspaceNotFound(String),
    #[error("优先级不存在: {0}")]
    PriorityNotFound(String),
    #[error("阻塞记录不存在: {0}")]
    BlockNotFound(String),
    #[error("工作会话不存在: {0}")]
    SessionNotFound(String),
    #[error("版本冲突")]
    VersionConflict,
    #[error("任务状态无效: {0}")]
    InvalidStatus(String),
    #[error("当前状态不允许执行此操作: {0}")]
    InvalidState(String),
    #[error("参数无效: {0}")]
    InvalidInput(String),
    #[error("任务已有进行中的阻塞")]
    ActiveBlockExists,
    #[error("任务已有进行中的工作会话")]
    ActiveSessionExists,
}

pub type Result<T> = std::result::Result<T, CoreError>;
