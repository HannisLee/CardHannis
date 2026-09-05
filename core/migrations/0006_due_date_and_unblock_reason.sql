-- 0006: 任务预计完成日期与解除阻塞原因
-- due_date 保存 YYYY-MM-DD；resolution_reason 记录解除阻塞时的可选原因。
ALTER TABLE tasks ADD COLUMN due_date TEXT;
ALTER TABLE task_blocks ADD COLUMN resolution_reason TEXT;
