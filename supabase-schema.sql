-- CardHannis Supabase schema
-- Run this in Supabase SQL Editor before using WebUI sync.
-- This MVP uses the publishable/anon key. The policies below are intentionally
-- open for a single trusted project; add Supabase Auth and per-user RLS before
-- using this database for multiple users or sensitive data.

create table if not exists public.tasks (
    id text primary key,
    title text not null check (length(trim(title)) > 0),
    notes text,
    review_notes text,
    estimated_active_minutes bigint check (estimated_active_minutes is null or estimated_active_minutes >= 0),
    created_at text not null,
    started_at text,
    completed_at text,
    status text not null default 'pending' check (status in ('pending', 'in_progress', 'completed')),
    sort_order bigint not null default 0,
    created_device_id text not null check (length(trim(created_device_id)) > 0),
    updated_at text not null,
    deleted_at text,
    version bigint not null default 1 check (version > 0)
);

create table if not exists public.task_blocks (
    id text primary key,
    task_id text not null references public.tasks(id) on delete cascade,
    started_at text not null,
    ended_at text,
    reason text not null check (length(trim(reason)) > 0),
    note text,
    created_at text not null,
    updated_at text not null,
    version bigint not null default 1 check (version > 0),
    deleted_at text
);

create table if not exists public.work_sessions (
    id text primary key,
    task_id text not null references public.tasks(id) on delete cascade,
    started_at text not null,
    ended_at text,
    note text,
    created_at text not null
);

create index if not exists ix_tasks_updated_at on public.tasks(updated_at);
create index if not exists ix_task_blocks_updated_at on public.task_blocks(updated_at);
create unique index if not exists ux_task_blocks_one_active on public.task_blocks(task_id) where ended_at is null and deleted_at is null;
create unique index if not exists ux_work_sessions_one_active on public.work_sessions(task_id) where ended_at is null;

alter table public.tasks enable row level security;
alter table public.task_blocks enable row level security;
alter table public.work_sessions enable row level security;

drop policy if exists "cardhannis tasks sync" on public.tasks;
create policy "cardhannis tasks sync" on public.tasks for all to anon, authenticated using (true) with check (true);
drop policy if exists "cardhannis blocks sync" on public.task_blocks;
create policy "cardhannis blocks sync" on public.task_blocks for all to anon, authenticated using (true) with check (true);
drop policy if exists "cardhannis sessions sync" on public.work_sessions;
create policy "cardhannis sessions sync" on public.work_sessions for all to anon, authenticated using (true) with check (true);
