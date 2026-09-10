# CardHannis Supabase 样例数据

这些 CSV 语义对齐当前桌面端本地数据模型；远端表使用 Postgres 原生 `timestamptz`、`date` 和 `boolean` 类型。请在 Supabase 中按下面的顺序导入，否则外键会失败：

1. `public.workspaces` ← `01_workspaces.csv`
2. `public.priorities` ← `02_priorities.csv`
3. `public.tasks` ← `03_tasks.csv`
4. `public.task_blocks` ← `04_task_blocks.csv`
5. `public.work_sessions` ← `05_work_sessions.csv`

CSV 中的空字符串表示 `NULL`。如果 Supabase 导入器没有自动把空字符串转为 `NULL`，导入后可执行：

```sql
update public.workspaces set deleted_at = null where deleted_at = '';
update public.priorities set deleted_at = null where deleted_at = '';
update public.tasks
set notes = nullif(notes, ''),
    review_notes = nullif(review_notes, ''),
    estimated_active_minutes = nullif(estimated_active_minutes, ''),
    started_at = nullif(started_at, ''),
    completed_at = nullif(completed_at, ''),
    deleted_at = nullif(deleted_at, ''),
    priority_id = nullif(priority_id, ''),
    home_workspace_id = nullif(home_workspace_id, ''),
    due_date = nullif(due_date, '')
where id in (
  '11111111-1111-4111-8111-111111111111',
  '22222222-2222-4222-8222-222222222222',
  '33333333-3333-4333-8333-333333333333',
  '44444444-4444-4444-8444-444444444444',
  '55555555-5555-4555-8555-555555555555'
);
update public.task_blocks
set ended_at = nullif(ended_at, ''),
    note = nullif(note, ''),
    resolution_reason = nullif(resolution_reason, ''),
    deleted_at = nullif(deleted_at, '');
update public.work_sessions
set ended_at = nullif(ended_at, ''),
    note = nullif(note, '');
```

注意：样例中的 `user_id` 是当前 Supabase Auth 用户的 ID，用于 RLS 隔离。`done` 是唯一内置工作区；`priorities.workspace_id` 表示分级属于哪个工作区；已完成任务的 `workspace_id` 是 `done`，但 `priority_id` 仍来自 `home_workspace_id` 对应的源工作区。
