-- 0005: 等待中并入待处理（用户决策：同一状态）
UPDATE tasks SET status = 'pending' WHERE status = 'waiting';
