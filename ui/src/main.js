import { invoke } from '@tauri-apps/api/core';
import './style.css';

const app = document.querySelector('#app');
const state = { tasks: [], filter: 'all' };

function isTauri() { return Boolean(window.__TAURI_INTERNALS__); }
function escapeHtml(value = '') { return value.replace(/[&<>'"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#039;', '"': '&quot;' }[c])); }
function formatDate(value) { return value ? new Intl.DateTimeFormat('zh-CN', { month: 'short', day: 'numeric' }).format(new Date(value)) : ''; }
function statusText(task) { return task.is_blocked ? '阻塞中' : task.status === 'in_progress' ? '进行中' : task.status === 'completed' ? '已完成' : '待处理'; }
function visibleTasks() { return state.tasks.filter((task) => state.filter === 'active' ? task.status !== 'completed' : state.filter === 'blocked' ? task.is_blocked : state.filter === 'completed' ? task.status === 'completed' : true); }

function navItem(filter, label, count) { return `<button class="nav-item ${state.filter === filter ? 'selected' : ''}" data-filter="${filter}" type="button"><span>${label}</span><b>${count}</b></button>`; }
function emptyState() { return `<div class="empty-state"><div class="empty-icon">✦</div><h3>这里还没有任务</h3><p>把脑中的下一步写下来，开始一个轻盈的清单。</p><button id="empty-new-task" class="primary-button" type="button">新建第一个任务</button></div>`; }
function taskCard(task) {
  const done = task.status === 'completed';
  return `<article class="task-card ${done ? 'done' : ''} ${task.is_blocked ? 'blocked' : ''}">
    <button class="check-button ${done ? 'checked' : ''}" data-action="complete" data-id="${task.id}" data-version="${task.version}" aria-label="${done ? '已完成' : '完成任务'}" type="button">${done ? '✓' : ''}</button>
    <div class="task-main"><div class="task-title-row"><h3>${escapeHtml(task.title)}</h3><span class="status-pill ${task.is_blocked ? 'blocked' : task.status}">${statusText(task)}</span></div>
      ${task.notes ? `<p>${escapeHtml(task.notes)}</p>` : ''}
      <div class="task-meta"><span>${task.estimated_active_minutes ? `预计 ${task.estimated_active_minutes} 分钟` : '未估时'}</span><span>更新于 ${formatDate(task.updated_at)}</span>${task.is_blocked ? '<span class="block-meta">需要处理依赖</span>' : ''}</div>
    </div>
    <div class="task-actions">${!done && !task.is_blocked ? `<button class="icon-button" data-action="work" data-id="${task.id}" title="开始工作" type="button">▶</button>` : ''}<button class="icon-button danger" data-action="delete" data-id="${task.id}" data-version="${task.version}" title="删除" type="button">⌫</button></div>
  </article>`;
}

function render() {
  const tasks = visibleTasks();
  const activeCount = state.tasks.filter((task) => task.status !== 'completed').length;
  const blockedCount = state.tasks.filter((task) => task.is_blocked).length;
  const completedCount = state.tasks.filter((task) => task.status === 'completed').length;
  app.innerHTML = `<main class="shell">
    <aside class="sidebar"><div class="brand"><span class="brand-mark">C</span><span>CardHannis</span></div><div class="sidebar-section-label">工作台</div><nav class="nav-list">${navItem('all', '全部任务', state.tasks.length)}${navItem('active', '待完成', activeCount)}${navItem('blocked', '阻塞中', blockedCount)}${navItem('completed', '已完成', completedCount)}</nav><div class="sidebar-footer"><div class="sync-dot"></div><span>${isTauri() ? '本地数据库已连接' : '浏览器预览模式'}</span></div></aside>
    <section class="content"><header class="topbar"><div><div class="eyebrow">MONDAY, AUGUST 31</div><h1>今天做什么？</h1></div><button class="avatar" type="button" aria-label="用户菜单">H</button></header>
      <section class="focus-card"><div class="focus-copy"><span class="focus-kicker">今日焦点</span><h2>${activeCount ? `还有 ${activeCount} 个任务等待推进` : '今天的任务已经完成'}</h2><p>${blockedCount ? `${blockedCount} 个任务正在等待外部依赖。` : '保持节奏，完成一个，再开始下一个。'}</p></div><div class="focus-orbit"><strong>${completedCount}</strong><small>已完成</small></div></section>
      <section class="tasks-section"><div class="section-heading"><div><span class="eyebrow">TASKS</span><h2>任务列表</h2></div><button id="new-task" class="primary-button" type="button"><span>＋</span> 新建任务</button></div><div class="task-list">${tasks.length ? tasks.map(taskCard).join('') : emptyState()}</div></section>
    </section></main><div id="toast" class="toast" role="status"></div>
    <dialog id="new-task-dialog"><form method="dialog" id="new-task-form" class="dialog-card"><button class="dialog-close" value="cancel" aria-label="关闭">×</button><span class="eyebrow">NEW TASK</span><h2>新建任务</h2><label>任务标题<input name="title" required maxlength="200" placeholder="例如：整理下周计划" autofocus /></label><label>备注<textarea name="notes" rows="3" placeholder="可选"></textarea></label><label>预计分钟数<input name="estimated" type="number" min="0" step="5" placeholder="可选" /></label><div class="dialog-actions"><button class="secondary-button" value="cancel">取消</button><button class="primary-button" value="default">创建任务</button></div></form></dialog>`;
  document.querySelectorAll('[data-filter]').forEach((button) => button.addEventListener('click', () => { state.filter = button.dataset.filter; render(); }));
  document.querySelector('#new-task')?.addEventListener('click', () => document.querySelector('#new-task-dialog').showModal());
  document.querySelector('#empty-new-task')?.addEventListener('click', () => document.querySelector('#new-task-dialog').showModal());
  document.querySelector('#new-task-form')?.addEventListener('submit', handleCreateTask);
  document.querySelectorAll('[data-action]').forEach((button) => button.addEventListener('click', () => handleAction(button.dataset.action, button.dataset.id, Number(button.dataset.version))));
}

async function handleCreateTask(event) {
  event.preventDefault();
  const data = new FormData(event.currentTarget);
  try {
    await call('create_task', { input: { title: data.get('title'), notes: data.get('notes') || null, estimatedActiveMinutes: data.get('estimated') ? Number(data.get('estimated')) : null } });
    event.currentTarget.closest('dialog').close(); await loadTasks(); notify('任务已创建');
  } catch (error) { notify(error.message || '创建任务失败'); }
}

async function handleAction(action, id, version) {
  try {
    if (action === 'complete') { const task = state.tasks.find((item) => item.id === id); if (task?.status === 'completed') return; await call('complete_task', { id, expectedVersion: version }); notify('任务已完成'); }
    if (action === 'work') { await call('start_work', { taskId: id }); notify('已开始计时'); }
    if (action === 'delete') { if (!window.confirm('确定删除这个任务吗？')) return; await call('delete_task', { id, expectedVersion: version }); notify('任务已删除'); }
    await loadTasks();
  } catch (error) { notify(error.message || '操作失败'); }
}

async function call(command, args = {}) { return isTauri() ? invoke(command, args) : previewCommand(command, args); }
async function previewCommand(command, args) {
  if (command === 'list_tasks') return state.tasks;
  if (command === 'create_task') state.tasks.unshift({ id: crypto.randomUUID(), title: args.title, notes: args.notes, estimated_active_minutes: args.estimatedActiveMinutes, status: 'pending', sort_order: 0, updated_at: new Date().toISOString(), version: 1, is_blocked: false });
  if (command === 'complete_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { task.status = 'completed'; task.version += 1; task.updated_at = new Date().toISOString(); } }
  if (command === 'delete_task') state.tasks = state.tasks.filter((item) => item.id !== args.id);
  if (command === 'start_work') { const task = state.tasks.find((item) => item.id === args.taskId); if (task) { task.status = 'in_progress'; task.version += 1; } }
}
async function loadTasks() { state.tasks = await call('list_tasks'); render(); }
function notify(message) { const toast = document.querySelector('#toast'); if (!toast) return; toast.textContent = message; toast.classList.add('show'); setTimeout(() => toast.classList.remove('show'), 2200); }

render();
loadTasks().catch((error) => notify(error.message || '无法加载任务'));
