import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import './style.css';

const app = document.querySelector('#app');
const state = { tasks: [], blocksByTask: {}, workspaces: [], prios: [], activeWs: null, blockingTaskId: null };
const COLLAPSE_KEY = 'cardha…e.v2';
let collapsed = {};
try { collapsed = JSON.parse(localStorage.getItem(COLLAPSE_KEY) || '{}'); } catch {}
let pinned = true;
const PRIO_PALETTE = ['#5c7699', '#7a5ea6', '#3d735e', '#a0632f', '#8a5a7a', '#4e7d8a'];

function isTauri() { return Boolean(window.__TAURI_INTERNALS__); }
function theWindow() { return isTauri() ? getCurrentWindow() : null; }
function escapeHtml(value = '') { return String(value).replace(/[&<>'"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#039;', '"': '&quot;' }[c])); }
function formatDate(value) { return value ? new Intl.DateTimeFormat('zh-CN', { month: 'numeric', day: 'numeric' }).format(new Date(value)) : ''; }
function formatEstimatedHours(minutes) {
  if (minutes === null || minutes === undefined) return '未估时';
  const hours = Number(minutes) / 60;
  const display = Number.isInteger(hours) ? String(hours) : hours.toFixed(2).replace(/0+$/, '').replace(/\.$/, '');
  return `≈${display}h`;
}
function parseEstimatedMinutes(hours) {
  if (hours === '' || hours === null || hours === undefined) return null;
  const value = Number(hours);
  return Number.isFinite(value) && value >= 0 ? Math.round(value * 60) : null;
}
function pillInfo(task) {
  if (task.is_blocked) return { cls: 'blocked', label: '阻塞' };
  return { in_progress: { cls: 'in_progress', label: '进行' }, pending: { cls: 'pending', label: '待办' }, waiting: { cls: 'waiting', label: '等待' }, completed: { cls: 'completed', label: '完成' } }[task.status];
}
const prioColor = (p, i) => p.color || PRIO_PALETTE[i % PRIO_PALETTE.length];

function row(task) {
  const done = task.status === 'completed';
  const block = state.blocksByTask[task.id];
  const pill = pillInfo(task);
  const tip = escapeHtml([task.title, task.is_blocked && block && block.reason ? `阻塞：${block.reason}` : '', task.notes || '', task.estimated_active_minutes != null ? formatEstimatedHours(task.estimated_active_minutes) : ''].filter(Boolean).join(' ｜ '));
  return `<div class="row ${done ? 'done' : ''}" data-id="${task.id}" data-version="${task.version}" title="${tip}">
    <span class="rt">${escapeHtml(task.title)}</span>
    <span class="pill ${pill.cls}">${pill.label}</span>
    <span class="ra">
      ${(task.status === 'pending' || task.status === 'waiting') && !task.is_blocked ? `<button class="nb" data-action="work" data-id="${task.id}" title="开始工作（计时）" type="button">▶</button>` : ''}
      ${!done && !task.is_blocked ? `<button class="nb" data-action="block" data-id="${task.id}" title="标记阻塞" type="button">⏸</button>` : ''}
      ${task.is_blocked && block ? `<button class="nb" data-action="unblock" data-block-id="${block.id}" data-block-version="${block.version}" title="解除阻塞 → 等待中" type="button">▶⏏</button>` : ''}
      ${!done ? `<button class="nb ok" data-action="complete" data-id="${task.id}" data-version="${task.version}" title="完成任务" type="button">✓</button>` : `<button class="nb" data-action="reopen" data-id="${task.id}" data-version="${task.version}" title="重新打开" type="button">↺</button>`}
    </span>
  </div>`;
}

function render() {
  const ws = state.workspaces.find((w) => w.id === state.activeWs) || state.workspaces[0];
  if (ws) state.activeWs = ws.id;
  const wsTasks = state.tasks.filter((t) => t.workspace_id === (ws ? ws.id : null));
  const groups = state.prios.map((p, i) => ({ p, color: prioColor(p, i), tasks: wsTasks.filter((t) => t.priority_id === p.id) }));
  const unsorted = wsTasks.filter((t) => !state.prios.some((p) => p.id === t.priority_id));
  app.innerHTML = `<div class="win"><div class="win-sheet">
    <header class="win-bar" data-tauri-drag-region>
      <span class="win-brand" data-tauri-drag-region>🗒 CardHannis</span>
      <div class="win-tools">
        <button id="btn-new" title="新建任务" type="button">＋</button>
        <button id="btn-pin" class="${pinned ? 'on' : ''}" title="切换窗口置顶" type="button">📌</button>
        <button id="btn-min" title="最小化" type="button">−</button>
        <button id="btn-close" title="关闭" type="button">✕</button>
      </div>
    </header>
    <nav class="ws-row">
      <div class="ws-tabs">${state.workspaces.map((w) => {
        const open = state.tasks.filter((t) => t.workspace_id === w.id && t.status !== 'completed').length;
        return `<button class="ws-tab ${w.id === state.activeWs ? 'active' : ''}" data-ws="${w.id}" type="button">${escapeHtml(w.name)}<b>${open}</b>${w.builtin ? '' : `<span class="ws-x" data-ws-del="${w.id}" data-version="${w.version}" title="删除工作区">×</span>`}</button>`;
      }).join('')}<button class="ws-tool" id="ws-add" title="新建工作区" type="button">＋</button></div>
      <button class="ws-tool" id="collapse-all" title="全部收起/展开" type="button">▤</button>
      <button class="ws-tool" id="add-prio" title="添加分级" type="button">＋P</button>
    </nav>
    <div class="win-body">
      ${[...groups, unsorted.length ? { p: { id: 'none', name: '未分级' }, color: '#a8a89a', tasks: unsorted } : null].filter(Boolean).map(({ p, color, tasks }) => {
        const key = `${state.activeWs}:${p.id}`;
        const closed = collapsed[key];
        return `<section class="sec ${closed ? 'collapsed' : ''}" data-prio="${p.id}">
          <div class="sec-row">
            <button class="sec-head" type="button" data-toggle="${p.id}"><span class="sec-chev">▼</span><span class="sec-dot" style="background:${color}"></span><span>${escapeHtml(p.name)}</span><span class="sec-count">${tasks.length}</span></button>
            ${p.id !== 'none' ? `<span class="sec-tools">
              <button data-gact="grp-add" data-prio="${p.id}" title="在此分级新任务" type="button">＋</button>
              <button data-gact="grp-rename" data-prio="${p.id}" data-version="${p.version}" title="重命名分级" type="button">✎</button>
              <button data-gact="grp-del" class="danger" data-prio="${p.id}" data-version="${p.version}" title="删除分级（需为空）" type="button">✕</button>
            </span>` : ''}
          </div>
          <div class="rows">${tasks.length ? tasks.map((t) => row(t)).join('') : '<div class="row-empty">暂无任务，点 ＋ 添加</div>'}</div>
        </section>`;
      }).join('')}
    </div>
    <footer class="win-foot"><span class="dot"></span><span>${isTauri() ? '本地数据库' : '浏览器预览'}</span><span class="foot-right">${new Date().toLocaleDateString('zh-CN', { month: 'long', day: 'numeric', weekday: 'short' })}</span></footer>
  </div>
  <dialog id="task-dialog">
    <form method="dialog" id="task-form" class="dlg-card">
      <h2>新任务</h2>
      <label>标题<input name="title" required maxlength="200" placeholder="要做点什么？" autofocus /></label>
      <label>预计（小时，可选）<input name="estimated" type="number" min="0" step="0.5" placeholder="例如 2" /></label>
      <label>备注<textarea name="notes" rows="2" placeholder="可选"></textarea></label>
      <div class="dlg-grid">
        <label>分级<select name="priority" id="prio-select"></select></label>
        <label>工作区<select name="workspace" id="ws-select"></select></label>
      </div>
      <div class="dlg-actions"><button class="ghost" value="cancel" type="submit">取消</button><button class="ok" value="default" type="submit">贴上</button></div>
    </form>
  </dialog>
  <dialog id="block-dialog">
    <form method="dialog" id="block-form" class="dlg-card">
      <h2>标记阻塞</h2>
      <p class="block-hint" id="block-task-title"></p>
      <label>原因<textarea name="reason" rows="2" required maxlength="300" placeholder="例如：等待接口文档"></textarea></label>
      <label>补充备注<textarea name="note" rows="2" placeholder="可选"></textarea></label>
      <div class="dlg-actions"><button class="ghost" value="cancel" type="submit">取消</button><button class="ok" value="default" type="submit">确认阻塞</button></div>
    </form>
  </dialog>
  <div id="toast" class="toast" role="status"></div></div>`;

  document.querySelector('#btn-new')?.addEventListener('click', () => openTaskDialog(null));
  document.querySelector('#btn-pin')?.addEventListener('click', togglePin);
  document.querySelector('#btn-min')?.addEventListener('click', () => theWindow()?.minimize());
  document.querySelector('#btn-close')?.addEventListener('click', () => theWindow()?.close());
  document.querySelector('#ws-add')?.addEventListener('click', addWorkspace);
  document.querySelector('#add-prio')?.addEventListener('click', addPriority);
  document.querySelector('#collapse-all')?.addEventListener('click', toggleCollapseAll);
  document.querySelectorAll('.ws-tab').forEach((tab) => tab.addEventListener('click', (e) => {
    const del = e.target.closest('[data-ws-del]');
    if (del) { removeWorkspace(del.dataset.wsDel, Number(del.dataset.version)); return; }
    state.activeWs = tab.dataset.ws;
    render();
  }));
  document.querySelectorAll('[data-toggle]').forEach((head) => head.addEventListener('click', () => {
    const key = `${state.activeWs}:${head.dataset.toggle}`;
    collapsed[key] = !collapsed[key];
    localStorage.setItem(COLLAPSE_KEY, JSON.stringify(collapsed));
    render();
  }));
  document.querySelectorAll('[data-gact]').forEach((b) => b.addEventListener('click', () => handleGroupTool(b)));
  document.querySelector('#task-form')?.addEventListener('submit', handleCreateTask);
  document.querySelector('#block-form')?.addEventListener('submit', handleBlockSubmit);
  document.querySelectorAll('[data-action]').forEach((button) => button.addEventListener('click', () => handleAction(button)));
}

async function addWorkspace() {
  const name = window.prompt('新工作区名称');
  const trimmed = name && name.trim();
  if (!trimmed) return;
  try {
    const created = await call('create_workspace', { name: trimmed });
    await loadMeta();
    state.activeWs = created.id;
    render();
    notify(`已创建工作区「${trimmed}」`);
  } catch (error) { notify(error.message || '创建失败'); }
}
async function removeWorkspace(id, expectedVersion) {
  const ws = state.workspaces.find((w) => w.id === id);
  if (!window.confirm(`删除工作区「${ws ? ws.name : ''}」？`)) return;
  try {
    await call('delete_workspace', { id, expectedVersion });
    await loadMeta();
    if (state.activeWs === id) state.activeWs = state.workspaces[0]?.id || null;
    render();
    notify('工作区已删除');
  } catch (error) { notify(error.message || '删除失败'); }
}
async function addPriority() {
  const name = window.prompt('新分级名称（如：P3 / 紧急）');
  const trimmed = name && name.trim();
  if (!trimmed) return;
  try {
    await call('create_priority', { name: trimmed, color: PRIO_PALETTE[state.prios.length % PRIO_PALETTE.length] });
    await loadMeta();
    render();
    notify(`已添加分级「${trimmed}」`);
  } catch (error) { notify(error.message || '添加失败'); }
}
async function handleGroupTool(button) {
  const prioId = button.dataset.prio;
  const prio = state.prios.find((p) => p.id === prioId);
  const version = Number(button.dataset.version || 0);
  try {
    if (button.dataset.gact === 'grp-add') { openTaskDialog(prioId); return; }
    if (button.dataset.gact === 'grp-rename') {
      const name = window.prompt('重命名分级', prio ? prio.name : '');
      const trimmed = name && name.trim();
      if (!trimmed || !prio) return;
      await call('update_priority', { id: prioId, expectedVersion: prio.version, name: trimmed, color: prio.color });
      await loadMeta(); render(); notify('分级已重命名'); return;
    }
    if (button.dataset.gact === 'grp-del') {
      if (!prio || !window.confirm(`删除空分级「${prio.name}」？`)) return;
      await call('delete_priority', { id: prioId, expectedVersion: prio.version });
      await loadMeta(); render(); notify('分级已删除');
    }
  } catch (error) { notify(error.message || '操作失败'); }
}
function toggleCollapseAll() {
  const keys = state.prios.map((p) => `${state.activeWs}:${p.id}`);
  const anyOpen = keys.some((k) => !collapsed[k]);
  keys.forEach((k) => { collapsed[k] = anyOpen; });
  localStorage.setItem(COLLAPSE_KEY, JSON.stringify(collapsed));
  render();
}

function openTaskDialog(prioId) {
  const form = document.querySelector('#task-form');
  form.reset();
  document.querySelector('#prio-select').innerHTML = state.prios.map((p) => `<option value="${p.id}" ${p.id === prioId ? 'selected' : ''}>${escapeHtml(p.name)}</option>`).join('');
  document.querySelector('#ws-select').innerHTML = state.workspaces.map((w) => `<option value="${w.id}" ${w.id === state.activeWs ? 'selected' : ''}>${escapeHtml(w.name)}</option>`).join('');
  document.querySelector('#task-dialog').showModal();
}

async function togglePin() {
  const w = theWindow();
  if (!w) { pinned = !pinned; render(); notify(pinned ? '（预览）模拟置顶' : '（预览）取消置顶'); return; }
  try {
    pinned = !pinned;
    await w.setAlwaysOnTop(pinned);
    render();
    notify(pinned ? '已置顶' : '已取消置顶');
  } catch (error) { pinned = !pinned; notify(error.message || '置顶失败'); }
}

async function handleCreateTask(event) {
  event.preventDefault();
  if (event.submitter && event.submitter.value === 'cancel') { event.currentTarget.closest('dialog').close(); return; }
  const data = new FormData(event.currentTarget);
  try {
    await call('create_task', {
      input: {
        title: data.get('title'),
        notes: data.get('notes') || null,
        estimatedActiveMinutes: parseEstimatedMinutes(data.get('estimated')),
        workspaceId: data.get('workspace') || state.activeWs,
        priorityId: data.get('priority') || state.prios[0]?.id || null,
      },
    });
    event.currentTarget.closest('dialog').close();
    await loadTasks();
    notify('任务已添加');
  } catch (error) { notify(error.message || '创建任务失败'); }
}

async function handleAction(button) {
  const action = button.dataset.action;
  const id = button.dataset.id;
  const version = Number(button.dataset.version);
  try {
    if (action === 'complete') { await call('complete_task', { id, expectedVersion: version }); notify('任务完成 ✦'); }
    if (action === 'reopen') { await call('reopen_task', { id, expectedVersion: version }); notify('任务已重新打开'); }
    if (action === 'work') { await call('start_work', { taskId: id }); notify('开始计时 ▶'); }
    if (action === 'block') { openBlockDialog(id); return; }
    if (action === 'unblock') { await call('unblock_task', { blockId: button.dataset.blockId, expectedVersion: Number(button.dataset.blockVersion) }); notify('已解除阻塞，进入等待中'); }
    await loadTasks();
  } catch (error) { notify(error.message || '操作失败'); }
}

function openBlockDialog(taskId) {
  state.blockingTaskId = taskId;
  const task = state.tasks.find((item) => item.id === taskId);
  const form = document.querySelector('#block-form');
  form.reason.value = '';
  form.note.value = '';
  document.querySelector('#block-task-title').textContent = task ? task.title : '';
  document.querySelector('#block-dialog').showModal();
}

async function handleBlockSubmit(event) {
  event.preventDefault();
  if (event.submitter && event.submitter.value === 'cancel') { event.currentTarget.closest('dialog').close(); return; }
  const reason = event.currentTarget.reason.value.trim();
  if (!reason || !state.blockingTaskId) return;
  try {
    await call('block_task', { taskId: state.blockingTaskId, reason, note: event.currentTarget.note.value.trim() || null });
    event.currentTarget.closest('dialog').close();
    state.blockingTaskId = null;
    await loadTasks();
    notify('已标记阻塞');
  } catch (error) { notify(error.message || '标记阻塞失败'); }
}

async function call(command, args = {}) { return isTauri() ? invoke(command, args) : previewCommand(command, args); }
async function previewCommand(command, args) {
  if (!state.workspaces.length) seedPreviewMeta();
  if (command === 'list_tasks') return state.tasks;
  if (command === 'list_workspaces') return state.workspaces;
  if (command === 'list_priorities') return state.prios;
  if (command === 'create_workspace') { const ws = { id: crypto.randomUUID(), name: args.name, sort_order: state.workspaces.length, builtin: false, version: 1 }; state.workspaces.push(ws); return ws; }
  if (command === 'delete_workspace') {
    if (state.tasks.some((t) => t.workspace_id === args.id)) throw new Error('工作区还有任务，先移走再删除');
    state.workspaces = state.workspaces.filter((w) => w.id !== args.id);
  }
  if (command === 'create_priority') { const p = { id: crypto.randomUUID(), name: args.name, color: args.color || null, sort_order: state.prios.length, version: 1 }; state.prios.push(p); return p; }
  if (command === 'update_priority') { const p = state.prios.find((x) => x.id === args.id); if (p) { p.name = args.name; p.color = args.color ?? p.color; p.version += 1; } return p; }
  if (command === 'delete_priority') {
    if (state.tasks.some((t) => t.priority_id === args.id)) throw new Error('该分级还有任务，先移走再删除');
    if (state.prios.length <= 1) throw new Error('至少要保留一个分级');
    state.prios = state.prios.filter((p) => p.id !== args.id);
  }
  if (command === 'create_task') {
    const task = { id: crypto.randomUUID(), title: args.input.title, notes: args.input.notes, estimated_active_minutes: args.input.estimatedActiveMinutes, status: 'pending', sort_order: 0, updated_at: new Date().toISOString(), version: 1, is_blocked: false, workspace_id: args.input.workspaceId, priority_id: args.input.priorityId };
    state.tasks.unshift(task);
    return task;
  }
  if (command === 'complete_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { task.status = 'completed'; task.version += 1; task.updated_at = new Date().toISOString(); } }
  if (command === 'reopen_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { task.status = 'pending'; task.completed_at = null; task.version += 1; } }
  if (command === 'delete_task') state.tasks = state.tasks.filter((item) => item.id !== args.id);
  if (command === 'start_work') { const task = state.tasks.find((item) => item.id === args.taskId); if (task) { task.status = 'in_progress'; task.version += 1; } }
  if (command === 'list_blocks') { const task = state.tasks.find((item) => item.id === args.taskId); return task?.activeBlock ? [task.activeBlock] : []; }
  if (command === 'block_task') { const task = state.tasks.find((item) => item.id === args.taskId); if (task) { task.is_blocked = true; task.activeBlock = { id: crypto.randomUUID(), task_id: task.id, started_at: new Date().toISOString(), ended_at: null, reason: args.reason, note: args.note ?? null, created_at: new Date().toISOString(), updated_at: new Date().toISOString(), version: 1, deleted_at: null }; } }
  if (command === 'unblock_task') { for (const task of state.tasks) { if (task.activeBlock?.id === args.blockId) { task.is_blocked = false; delete task.activeBlock; task.status = 'waiting'; task.version += 1; } } }
}
function seedPreviewMeta() {
  state.workspaces = [
    { id: 'daily', name: '日常', sort_order: 0, builtin: true, version: 1 },
    { id: 'work', name: '工作', sort_order: 1, builtin: true, version: 1 },
  ];
  state.prios = [
    { id: 'P0', name: 'P0', color: '#b0432f', sort_order: 0, version: 1 },
    { id: 'P1', name: 'P1', color: '#b16d42', sort_order: 1, version: 1 },
    { id: 'P2', name: 'P2', color: '#8f9a90', sort_order: 2, version: 1 },
  ];
}

async function loadMeta() {
  if (isTauri()) {
    state.workspaces = await call('list_workspaces');
    state.prios = await call('list_priorities');
  } else {
    seedPreviewMeta();
  }
  if (!state.activeWs) state.activeWs = state.workspaces[0]?.id || null;
}

async function loadTasks() {
  state.tasks = await call('list_tasks');
  state.blocksByTask = {};
  await Promise.all(state.tasks.filter((task) => task.is_blocked).map(async (task) => {
    const blocks = await call('list_blocks', { taskId: task.id });
    const active = blocks.find((block) => !block.ended_at);
    if (active) state.blocksByTask[task.id] = active;
  }));
  render();
}


// ===== 右键菜单（删除任务） =====
let ctxMenu = null;
function closeContextMenu() {
  if (ctxMenu) { ctxMenu.remove(); ctxMenu = null; }
}
function openContextMenu(x, y, taskId, version) {
  closeContextMenu();
  const menu = document.createElement('div');
  menu.className = 'ctx-menu';
  menu.innerHTML = `<button type="button" data-ctx="delete">🗑 删除任务</button>`;
  document.body.appendChild(menu);
  menu.style.left = `${Math.min(x, window.innerWidth - menu.offsetWidth - 6)}px`;
  menu.style.top = `${Math.min(y, window.innerHeight - menu.offsetHeight - 6)}px`;
  menu.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-ctx]');
    if (!btn) return;
    closeContextMenu();
    if (btn.dataset.ctx === 'delete') await deleteTask(taskId, version);
  });
  ctxMenu = menu;
}
async function deleteTask(id, expectedVersion) {
  if (!window.confirm('确定删除这个任务吗？')) return;
  try {
    await call('delete_task', { id, expectedVersion });
    await loadTasks();
    notify('任务已删除');
  } catch (error) { notify(error.message || '删除失败'); }
}
document.addEventListener('contextmenu', (e) => {
  const r = e.target.closest('.row');
  if (!r) { closeContextMenu(); return; }
  e.preventDefault();
  openContextMenu(e.clientX, e.clientY, r.dataset.id, Number(r.dataset.version));
});
document.addEventListener('click', (e) => {
  if (ctxMenu && !ctxMenu.contains(e.target)) closeContextMenu();
});
window.addEventListener('blur', closeContextMenu);

function notify(message) {
  const toast = document.querySelector('#toast');
  if (!toast) return;
  toast.textContent = message;
  toast.classList.add('show');
  clearTimeout(notify._t);
  notify._t = setTimeout(() => toast.classList.remove('show'), 2200);
}

render();
(async () => {
  const w = theWindow();
  if (w) { try { pinned = await w.isAlwaysOnTop(); } catch {} }
  await loadMeta();
  await loadTasks();
})();
