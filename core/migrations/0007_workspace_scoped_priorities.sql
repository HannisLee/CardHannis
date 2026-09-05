-- 0007: 分级改为工作区私有，不再跨工作区共享。
-- 已完成任务保留来源工作区（home_workspace_id）的分级。

ALTER TABLE priorities ADD COLUMN workspace_id TEXT REFERENCES workspaces(id);

CREATE TEMP TABLE priority_workspace_map (
    old_priority_id TEXT NOT NULL,
    workspace_id TEXT NOT NULL,
    new_priority_id TEXT NOT NULL,
    PRIMARY KEY (old_priority_id, workspace_id)
);

-- 复用原有分级 ID 作为 daily 工作区的本地分级。
INSERT INTO priority_workspace_map (old_priority_id, workspace_id, new_priority_id)
SELECT id, 'daily', id
FROM priorities
WHERE deleted_at IS NULL;

-- 为其他工作区复制独立分级。包含已软删工作区，以便历史已完成任务仍能解析分级名称。
INSERT INTO priority_workspace_map (old_priority_id, workspace_id, new_priority_id)
SELECT
    p.id,
    w.id,
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4' ||
    substr(lower(hex(randomblob(2))), 2) || '-' ||
    substr('89ab', abs(random()) % 4 + 1, 1) ||
    substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6)))
FROM priorities p
CROSS JOIN workspaces w
WHERE p.deleted_at IS NULL
  AND w.id NOT IN ('daily', 'done');

UPDATE priorities
SET workspace_id = 'daily'
WHERE workspace_id IS NULL;

INSERT INTO priorities (
    id, name, color, sort_order, created_at, updated_at, deleted_at, version, workspace_id
)
SELECT
    m.new_priority_id,
    p.name,
    p.color,
    p.sort_order,
    p.created_at,
    p.updated_at,
    p.deleted_at,
    p.version,
    m.workspace_id
FROM priority_workspace_map m
JOIN priorities p ON p.id = m.old_priority_id
WHERE m.workspace_id <> 'daily';

-- 已删除工作区的克隆分级只用于历史关联，不应重新出现在活动分级列表中。
UPDATE priorities
SET deleted_at = COALESCE(
    deleted_at,
    (SELECT w.deleted_at FROM workspaces w WHERE w.id = priorities.workspace_id)
)
WHERE workspace_id IN (
    SELECT id FROM workspaces WHERE deleted_at IS NOT NULL
);

-- 普通任务按当前工作区迁移；已完成任务按原工作区迁移。
UPDATE tasks
SET priority_id = COALESCE((
    SELECT m.new_priority_id
    FROM priority_workspace_map m
    WHERE m.old_priority_id = tasks.priority_id
      AND m.workspace_id = COALESCE(
          CASE WHEN tasks.workspace_id = 'done' THEN tasks.home_workspace_id ELSE tasks.workspace_id END,
          'daily'
      )
), priority_id)
WHERE priority_id IN (SELECT DISTINCT old_priority_id FROM priority_workspace_map);

DROP TABLE priority_workspace_map;

CREATE INDEX IF NOT EXISTS ix_priorities_workspace
    ON priorities(workspace_id, sort_order, created_at);
