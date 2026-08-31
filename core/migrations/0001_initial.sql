PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    notes TEXT,
    review_notes TEXT,
    estimated_active_minutes INTEGER CHECK (
        estimated_active_minutes IS NULL OR estimated_active_minutes >= 0
    ),
    created_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (
        status IN ('pending', 'in_progress', 'completed')
    ),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_device_id TEXT NOT NULL CHECK (length(trim(created_device_id)) > 0),
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    CHECK (completed_at IS NULL OR started_at IS NULL OR started_at <= completed_at),
    CHECK (status <> 'completed' OR completed_at IS NOT NULL),
    CHECK (status = 'completed' OR completed_at IS NULL)
);

CREATE TABLE IF NOT EXISTS task_blocks (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    reason TEXT NOT NULL CHECK (length(trim(reason)) > 0),
    note TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    deleted_at TEXT,
    CHECK (ended_at IS NULL OR ended_at >= started_at),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS work_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    note TEXT,
    created_at TEXT NOT NULL,
    CHECK (ended_at IS NULL OR ended_at >= started_at),
    FOREIGN KEY (task_id) REFERENCES tasks(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS ix_tasks_active_list
    ON tasks(status, sort_order, created_at) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS ix_tasks_updated_at ON tasks(updated_at);
CREATE INDEX IF NOT EXISTS ix_tasks_deleted_at ON tasks(deleted_at);
CREATE INDEX IF NOT EXISTS ix_task_blocks_task_started ON task_blocks(task_id, started_at);
CREATE INDEX IF NOT EXISTS ix_task_blocks_updated_at ON task_blocks(updated_at);
CREATE INDEX IF NOT EXISTS ix_work_sessions_task_started ON work_sessions(task_id, started_at);
CREATE INDEX IF NOT EXISTS ix_work_sessions_started_at ON work_sessions(started_at);
CREATE UNIQUE INDEX IF NOT EXISTS ux_task_blocks_one_active
    ON task_blocks(task_id) WHERE ended_at IS NULL AND deleted_at IS NULL;
CREATE UNIQUE INDEX IF NOT EXISTS ux_work_sessions_one_active
    ON work_sessions(task_id) WHERE ended_at IS NULL;
