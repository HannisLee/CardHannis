const state = { tasks: [], filter: 'all', editingTask: null, activeSessionByTask: {} };
const byId = (id) => document.getElementById(id);

function formatDate(value) { return value ? new Intl.DateTimeFormat('zh-CN', { month: 'short', day: 'numeric' }).format(new Date(value)) : ''; }
function escapeHtml(value = '') { return value.replace(/[&<>'"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#039;', '"': '&quot;' }[c])); }
function statusText(task) { return task.is_blocked ? '阻塞中' : task.status === 'in_progress' ? '进行中' : task.status === 'completed' ? '已完成' : '待处理'; }
function formatEstimatedHours(minutes) {
  if (minutes === null || minutes === undefined) return '未估时';
  const hours = Number(minutes) / 60;
  const display = Number.isInteger(hours) ? String(hours) : hours.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
  return `预计 ${display} 小时`;
}
function parseEstimatedMinutes(hours) {
  if (hours === '' || hours === null || hours === undefined) return null;
  const value = Number(hours);
  return Number.isFinite(value) && value >= 0 ? Math.round(value * 60) : null;
}
function notify(message, isError = false) { const toast = byId('toast'); toast.textContent = message; toast.classList.toggle('error', isError); toast.classList.add('show'); setTimeout(() => toast.classList.remove('show'), 2300); }

async function api(path, options = {}) {
  const response = await fetch(path, { headers: { 'Content-Type': 'application/json' }, ...options });
  if (!response.ok) { let message = `请求失败：${response.status}`; try { const data = await response.json(); message = data.detail || message; } catch {} throw new Error(message); }
  return response.status === 204 ? null : response.json();
}

function visibleTasks() { return state.tasks.filter((task) => state.filter === 'active' ? task.status !== 'completed' : state.filter === 'blocked' ? task.is_blocked : state.filter === 'completed' ? task.status === 'completed' : true); }

function renderFilters() {
  const counts = { all: state.tasks.length, active: state.tasks.filter((t) => t.status !== 'completed').length, blocked: state.tasks.filter((t) => t.is_blocked).length, completed: state.tasks.filter((t) => t.status === 'completed').length };
  byId('filters').innerHTML = [ ['all','全部任务'],['active','待完成'],['blocked','阻塞中'],['completed','已完成'] ].map(([key,label]) => `<button class="nav-item ${state.filter === key ? 'selected' : ''}" data-filter="${key}" type="button"><span>${label}</span><b>${counts[key]}</b></button>`).join('');
}

function taskCard(task) {
  const done = task.status === 'completed';
  const session = state.activeSessionByTask[task.id];
  return `<article class="task-card ${done ? 'done' : ''} ${task.is_blocked ? 'blocked' : ''}">
    <button class="check-button ${done ? 'checked' : ''}" data-action="complete" data-id="${task.id}" aria-label="${done ? '已完成' : '完成任务'}" type="button">${done ? '✓' : ''}</button>
    <div class="task-main"><div class="task-title-row"><h3>${escapeHtml(task.title)}</h3><span class="status-pill ${task.is_blocked ? 'blocked' : task.status}">${statusText(task)}</span></div>
      ${task.notes ? `<p>${escapeHtml(task.notes)}</p>` : ''}
      <div class="task-meta"><span>${formatEstimatedHours(task.estimated_active_minutes)}</span><span>更新于 ${formatDate(task.updated_at)}</span>${session ? `<span class="working-meta">正在计时：${formatDate(session.started_at)} 开始</span>` : ''}${task.is_blocked ? '<span class="block-meta">需要处理依赖</span>' : ''}</div>
      ${task.is_blocked ? `<div class="block-actions"><button class="secondary-button small" data-action="unblock" data-block-id="${task.active_block_id}" data-block-version="${task.active_block_version}" type="button">解除阻塞</button></div>` : ''}
    </div>
    <div class="task-actions">
      ${!done ? (session ? `<button class="icon-button" data-action="stop-work" data-session-id="${session.id}" title="结束工作" type="button">■</button>` : `<button class="icon-button" data-action="start-work" data-id="${task.id}" title="开始工作" type="button">▶</button>`) : ''}
      ${!done && !session ? `<button class="icon-button" data-action="block" data-id="${task.id}" title="标记阻塞" type="button">⏸</button>` : ''}
      ${!done ? `<button class="icon-button" data-action="edit" data-id="${task.id}" title="编辑" type="button">✎</button>` : ''}
      <button class="icon-button danger" data-action="delete" data-id="${task.id}" title="删除" type="button">⌫</button>
    </div>
  </article>`;
}

function render() {
  renderFilters();
  const tasks = visibleTasks();
  const activeCount = state.tasks.filter((task) => task.status !== 'completed').length;
  const blockedCount = state.tasks.filter((task) => task.is_blocked).length;
  const completedCount = state.tasks.filter((task) => task.status === 'completed').length;
  byId('completed-count').textContent = completedCount;
  byId('focus-title').textContent = activeCount ? `还有 ${activeCount} 个任务等待推进` : '今天的任务已经完成';
  byId('focus-description').textContent = blockedCount ? `${blockedCount} 个任务正在等待外部依赖。` : '保持节奏，完成一个，再开始下一个。';
  byId('tasks').innerHTML = tasks.length ? tasks.map(taskCard).join('') : `<div class="empty-state"><div class="empty-icon">✦</div><h3>这里还没有任务</h3><p>把脑中的下一步写下来，开始一个轻盈的清单。</p><button id="empty-new-task" class="primary-button" type="button">新建第一个任务</button></div>`;
}

async function loadTasks() {
  state.tasks = await api('/api/tasks');
  await loadActiveSessions();
  render();
}

async function loadActiveSessions() {
  state.activeSessionByTask = {};
  await Promise.all(state.tasks.filter((task) => task.status !== 'completed').map(async (task) => {
    const sessions = await api(`/api/tasks/${task.id}/sessions`);
    const active = sessions.find((session) => !session.ended_at);
    if (active) state.activeSessionByTask[task.id] = active;
  }));
}

function openTaskDialog(task = null) {
  state.editingTask = task;
  const form = byId('task-form');
  byId('task-form-title').textContent = task ? 'EDIT TASK' : 'NEW TASK';
  byId('task-form-heading').textContent = task ? '编辑任务' : '新建任务';
  form.title.value = task?.title ?? '';
  form.notes.value = task?.notes ?? '';
  form.review_notes.value = task?.review_notes ?? '';
  form.estimated_active_hours.value = task?.estimated_active_minutes == null ? '' : (task.estimated_active_minutes / 60).toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
  byId('task-dialog').showModal();
}

byId('new-task').addEventListener('click', () => openTaskDialog());
byId('today').textContent = new Intl.DateTimeFormat('zh-CN', { weekday: 'long', month: 'long', day: 'numeric' }).format(new Date());

byId('task-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const form = event.currentTarget;
  const payload = {
    title: form.title.value,
    notes: form.notes.value || null,
    review_notes: form.review_notes.value || null,
    estimated_active_minutes: parseEstimatedMinutes(form.estimated_active_hours.value),
    sort_order: state.editingTask?.sort_order ?? 0,
  };
  try {
    if (state.editingTask) await api(`/api/tasks/${state.editingTask.id}`, { method: 'PATCH', body: JSON.stringify({ ...payload, expected_version: state.editingTask.version }) });
    else await api('/api/tasks', { method: 'POST', body: JSON.stringify({ ...payload, created_device_id: 'web-ui' }) });
    byId('task-dialog').close();
    await loadTasks();
    notify(state.editingTask ? '任务已更新' : '任务已创建');
  } catch (error) { notify(error.message, true); }
});

document.addEventListener('click', async (event) => {
  const button = event.target.closest('[data-action], [data-filter]');
  if (!button) return;
  if (button.dataset.filter) { state.filter = button.dataset.filter; render(); return; }
  if (button.id === 'empty-new-task') { openTaskDialog(); return; }
  const task = state.tasks.find((item) => item.id === button.dataset.id);
  try {
    switch (button.dataset.action) {
      case 'complete': if (task?.status === 'completed') return; await api(`/api/tasks/${button.dataset.id}/complete?expected_version=${task.version}`, { method: 'POST' }); notify('任务已完成'); break;
      case 'edit': openTaskDialog(task); return;
      case 'delete': if (!window.confirm('确定删除这个任务吗？')) return; await api(`/api/tasks/${button.dataset.id}?expected_version=${task.version}`, { method: 'DELETE' }); notify('任务已删除'); break;
      case 'start-work': await api(`/api/tasks/${button.dataset.id}/sessions`, { method: 'POST', body: JSON.stringify({ note: null }) }); notify('已开始计时'); break;
      case 'stop-work': await api(`/api/sessions/${button.dataset.sessionId}/end`, { method: 'POST' }); notify('已结束本次工作'); break;
      case 'block': { const reason = window.prompt('阻塞原因是什么？', ''); if (!reason?.trim()) return; await api(`/api/tasks/${button.dataset.id}/blocks`, { method: 'POST', body: JSON.stringify({ reason, note: null }) }); notify('已标记阻塞'); break; }
      case 'unblock': await api(`/api/blocks/${button.dataset.blockId}/end?expected_version=${button.dataset.blockVersion}`, { method: 'POST' }); notify('已解除阻塞'); break;
    }
    await loadTasks();
  } catch (error) { notify(error.message, true); }
});

loadTasks().catch((error) => notify(error.message, true));
