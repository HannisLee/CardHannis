-- CardHannis Supabase schema
-- 用于桌面端自动同步；已有项目执行前请先备份。
-- RLS 已启用但故意不创建公开读写策略。实现同步时必须先接入 Supabase Auth
-- 和按用户隔离的 RLS，不要恢复旧的 anon 公开读写策略。

create table if not exists public.workspaces (
    id text primary key,
    name text not null check (length(trim(name)) > 0),
    sort_order bigint not null default 0,
    builtin boolean not null default false,
    user_id uuid not null default auth.uid(),
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,
    version bigint not null default 1 check (version > 0)
);

create table if not exists public.priorities (
    id text primary key,
    workspace_id text not null references public.workspaces(id) on delete cascade,
    user_id uuid not null default auth.uid(),
    name text not null check (length(trim(name)) > 0),
    color text,
    sort_order bigint not null default 0,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,
    version bigint not null default 1 check (version > 0)
);

create table if not exists public.tasks (
    id text primary key,
    title text not null check (length(trim(title)) > 0),
    notes text,
    review_notes text,
    estimated_active_minutes bigint check (
        estimated_active_minutes is null or estimated_active_minutes >= 0
    ),
    created_at timestamptz not null default now(),
    started_at timestamptz,
    completed_at timestamptz,
    status text not null default 'pending' check (
        status in ('pending', 'in_progress', 'waiting', 'completed')
    ),
    sort_order bigint not null default 0,
    created_device_id text not null check (length(trim(created_device_id)) > 0),
    user_id uuid not null default auth.uid(),
    updated_at timestamptz not null default now(),
    deleted_at timestamptz,
    version bigint not null default 1 check (version > 0),
    workspace_id text references public.workspaces(id),
    priority_id text references public.priorities(id),
    home_workspace_id text references public.workspaces(id),
    due_date date,
    check (completed_at is null or started_at is null or started_at <= completed_at),
    check (status <> 'completed' or completed_at is not null),
    check (status = 'completed' or completed_at is null)
);

create table if not exists public.task_blocks (
    id text primary key,
    task_id text not null references public.tasks(id) on delete cascade,
    started_at timestamptz not null,
    ended_at timestamptz,
    reason text not null check (length(trim(reason)) > 0),
    note text,
    resolution_reason text,
    created_at timestamptz not null default now(),
    updated_at timestamptz not null default now(),
    version bigint not null default 1 check (version > 0),
    deleted_at timestamptz,
    check (ended_at is null or ended_at >= started_at)
);

create table if not exists public.work_sessions (
    id text primary key,
    task_id text not null references public.tasks(id) on delete cascade,
    user_id uuid not null default auth.uid(),
    started_at timestamptz not null,
    ended_at timestamptz,
    note text,
    created_at timestamptz not null default now(),
    check (ended_at is null or ended_at >= started_at)
);

-- Upgrade existing installations that were created before per-user sync.
alter table public.workspaces add column if not exists user_id uuid default auth.uid();
alter table public.priorities add column if not exists user_id uuid default auth.uid();
alter table public.tasks add column if not exists user_id uuid default auth.uid();
alter table public.task_blocks add column if not exists user_id uuid default auth.uid();
alter table public.work_sessions add column if not exists user_id uuid default auth.uid();
alter table public.workspaces alter column user_id set not null;
alter table public.priorities alter column user_id set not null;
alter table public.tasks alter column user_id set not null;
alter table public.task_blocks alter column user_id set not null;
alter table public.work_sessions alter column user_id set not null;

create index if not exists ix_tasks_updated_at on public.tasks(updated_at);
create index if not exists ix_tasks_active_list
    on public.tasks(status, sort_order, created_at) where deleted_at is null;
create index if not exists ix_task_blocks_updated_at on public.task_blocks(updated_at);
create index if not exists ix_task_blocks_task_started
    on public.task_blocks(task_id, started_at);
create index if not exists ix_work_sessions_task_started
    on public.work_sessions(task_id, started_at);
create unique index if not exists ux_task_blocks_one_active
    on public.task_blocks(task_id) where ended_at is null and deleted_at is null;
create unique index if not exists ux_work_sessions_one_active
    on public.work_sessions(task_id) where ended_at is null;
create index if not exists ix_priorities_workspace
    on public.priorities(workspace_id, sort_order, created_at);
create index if not exists ix_workspaces_user on public.workspaces(user_id);
create index if not exists ix_priorities_user on public.priorities(user_id);
create index if not exists ix_tasks_user on public.tasks(user_id);
create index if not exists ix_task_blocks_user on public.task_blocks(user_id);
create index if not exists ix_work_sessions_user on public.work_sessions(user_id);

alter table public.workspaces enable row level security;
alter table public.priorities enable row level security;
alter table public.tasks enable row level security;
alter table public.task_blocks enable row level security;
alter table public.work_sessions enable row level security;

do $$
declare
    table_name text;
begin
    foreach table_name in array array['workspaces', 'priorities', 'tasks', 'task_blocks', 'work_sessions']
    loop
        execute format('drop policy if exists "cardhannis owner select" on public.%I', table_name);
        execute format('drop policy if exists "cardhannis owner insert" on public.%I', table_name);
        execute format('drop policy if exists "cardhannis owner update" on public.%I', table_name);
        execute format('drop policy if exists "cardhannis owner delete" on public.%I', table_name);
        execute format('create policy "cardhannis owner select" on public.%I for select to authenticated using (user_id = auth.uid())', table_name);
        execute format('create policy "cardhannis owner insert" on public.%I for insert to authenticated with check (user_id = auth.uid())', table_name);
        execute format('create policy "cardhannis owner update" on public.%I for update to authenticated using (user_id = auth.uid()) with check (user_id = auth.uid())', table_name);
        execute format('create policy "cardhannis owner delete" on public.%I for delete to authenticated using (user_id = auth.uid())', table_name);
    end loop;
end
$$;

revoke all on public.workspaces, public.priorities, public.tasks, public.task_blocks, public.work_sessions from anon;
grant select, insert, update, delete on public.workspaces, public.priorities, public.tasks, public.task_blocks, public.work_sessions to authenticated;
notify pgrst, 'reload schema';
