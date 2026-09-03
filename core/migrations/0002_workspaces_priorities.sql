-- 0002: 工作区、优先级分级、等待中状态
-- 重建 tasks 表（新增 workspace_id/priority_id，status 允许 waiting），
-- 期间临时关闭外键，结束后恢复。迁移由 schema_migrations 记录，不会重复执行。

PRAGMA foreign_keys = OFF;

CREATE TABLE IF NOT EXISTS schema_migrations (
    name TEXT PRIMARY KEY NOT NULL
);

CREATE TABLE IF NOT EXISTS workspaces (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    sort_order INTEGER NOT NULL DEFAULT 0,
    builtin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS priorities (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    color TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE IF NOT EXISTS tasks_next (
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
        status IN ('pending', 'in_progress', 'waiting', 'completed')
    ),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_device_id TEXT NOT NULL CHECK (length(trim(created_device_id)) > 0),
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    workspace_id TEXT REFERENCES workspaces(id),
    priority_id TEXT REFERENCES priorities(id),
    CHECK (completed_at IS NULL OR started_at IS NULL OR started_at <= completed_at),
    CHECK (status <> 'completed' OR completed_at IS NOT NULL),
    CHECK (status = 'completed' OR completed_at IS NULL)
);

INSERT INTO tasks_next (
    id, title, notes, review_notes, estimated_active_minutes, created_at,
    started_at, completed_at, status, sort_order, created_device_id,
    updated_at, deleted_at, version
)
SELECT id, title, notes, review_notes, estimated_active_minutes, created_at,
    started_at, completed_at, status, sort_order, created_device_id,
    updated_at, deleted_at, version
FROM tasks;

DROP TABLE tasks;
ALTER TABLE tasks_next RENAME TO tasks;

CREATE INDEX IF NOT EXISTS ix_tasks_active_list
    ON tasks(status, sort_order, created_at) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS ix_tasks_updated_at ON tasks(updated_at);
CREATE INDEX IF NOT EXISTS ix_tasks_deleted_at ON tasks(deleted_at);
CREATE INDEX IF NOT EXISTS ix_tasks_workspace ON tasks(workspace_id);
CREATE INDEX IF NOT EXISTS ix_tasks_priority ON tasks(priority_id);

-- 内置工作区与分级（固定 id，种子幂等）
INSERT OR IGNORE INTO workspaces (id, name, sort_order, builtin, created_at, updated_at) VALUES
    ('daily', '日常', 0, 1, '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z'),
    ('work', '工作', 1, 1, '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z');

INSERT OR IGNORE INTO priorities (id, name, color, sort_order, created_at, updated_at) VALUES
    ('P0', 'P0', '#b0432f', 0, '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z'),
    ('P1', 'P1', '#b16d42', 1, '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z'),
    ('P2', 'P2', '#8f9a90', 2, '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z');

-- 旧任务回填默认归属
UPDATE tasks SET workspace_id = 'daily' WHERE workspace_id IS NULL;
UPDATE tasks SET priority_id = 'P1' WHERE priority_id IS NULL;

PRAGMA foreign_keys = ON;
