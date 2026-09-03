-- 0003: 内置「已完成」工作区 + 任务归属记忆列
-- 完成的任务统一归档到 done 工作区；home_workspace_id 记录任务原本所在工作区，
-- 重新打开时回到原工作区。

INSERT OR IGNORE INTO workspaces (id, name, sort_order, builtin, created_at, updated_at)
VALUES ('done', '已完成', 99, 1, '1970-01-01T00:00:00.000Z', '1970-01-01T00:00:00.000Z');

ALTER TABLE tasks ADD COLUMN home_workspace_id TEXT REFERENCES workspaces(id);

UPDATE tasks SET home_workspace_id = workspace_id WHERE home_workspace_id IS NULL;
