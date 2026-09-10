-- 0008: 仅「已完成」是内置工作区；日常/工作变为用户可删除的普通工作区。
UPDATE workspaces
SET builtin = 0
WHERE id <> 'done';
