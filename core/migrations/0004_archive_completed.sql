-- 0004: 存量已完成任务归档到 done 工作区
UPDATE tasks SET workspace_id = 'done' WHERE status = 'completed' AND workspace_id <> 'done';
