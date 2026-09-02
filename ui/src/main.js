import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import './style.css';

const app = document.querySelector('#app');
const state = { tasks: [], blocksByTask: {}, blockingTaskId: null };
const COLLAPSE_KEY = 'cardha…s.v1';
let collapsed = {};
try { collapsed = JSON.parse(localStorage.getItem(COLLAPSE_KEY) || '{}'); } catch {}
let pinned = true;

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

const SECTIONS = [
  { key: 'in_progress', label: '进行中', paper: '#e3efd9', match: (t) => t.status === 'in_progress' && !t.is_blocked },
  { key: 'blocked', label: '阻塞中', paper: '#f7e0cd', match: (t) => t.is_blocked && t.status !== 'completed' },
  { key: 'pending', label: '待处理', paper: '#fdf3cf', match: (t) => t.status === 'pending' && !t.is_blocked },
  { key: 'completed', label: '已完成', paper: '#ebe9df', match: (t) => t.status === 'completed' },
];
function pillInfo(task) {
  if (task.is_blocked) return { cls: 'blocked', label: '阻塞' };
  return { in_progress: { cls: 'in_progress', label: '进行' }, pending: { cls: 'pending', label: '待办' }, completed: { cls: 'completed', label: '完成' } }[task.status];
}

function note(task) {
  const done = task.status === 'completed';
  const block = state.blocksByTask[task.id];
  const pill = pillInfo(task);
  const tip = escapeHtml([task.title, task.is_blocked && block && block.reason ? `阻塞：${block.reason}` : '', task.notes || '', task.estimated_active_minutes != null ? formatEstimatedHours(task.estimated_active_minutes) : ''].filter(Boolean).join(' ｜ '));
  return `<div class="row ${done ? 'done' : ''}" title="${tip}">
    <span class="rt">${escapeHtml(task.title)}</span>
    <span class="pill ${pill.cls}">${pill.label}</span>
    <span class="ra">
      ${task.status === 'pending' && !task.is_blocked ? `<button class="nb" data-action="work" data-id="${task.id}" title="开始工作（计时）" type="button">▶</button>` : ''}
      ${!done && !task.is_blocked ? `<button class="nb" data-action="block" data-id="${task.id}" title="标记阻塞" type="button">⏸</button>` : ''}
      ${task.is_blocked && block ? `<button class="nb" data-action="unblock" data-block-id="${block.id}" data-block-version="${block.version}" title="解除阻塞" type="button">▶⏏</button>` : ''}
      ${!done ? `<button class="nb ok" data-action="complete" data-id="${task.id}" data-version="${task.version}" title="完成任务" type="button">✓</button>` : ''}
      <button class="nb danger" data-action="delete" data-id="${task.id}" data-version="${task.version}" title="删除任务" type="button">⌫</button>
    </span>
  </div>`;
}

function render() {
  const groups = SECTIONS.map((sec) => ({ sec, tasks: state.tasks.filter(sec.match) })).filter((g) => g.tasks.length);
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
    <div class="win-body">
      ${groups.length ? groups.map(({ sec, tasks }) => `<section class="sec ${collapsed[sec.key] ? 'collapsed' : ''}" data-section="${sec.key}">
        <button class="sec-head" type="button"><span class="sec-chev">▼</span><span>${sec.label}</span><span class="sec-count">${tasks.length}</span></button>
        <div class="rows">${tasks.map((t) => note(t)).join('')}</div>
      </section>`).join('') : '<div class="empty-hint">还没有便签<br>点右上「＋」撕一张下来</div>'}
    </div>
    <footer class="win-foot"><span class="dot"></span><span>${isTauri() ? '本地数据库' : '浏览器预览'}</span><span class="foot-right">${new Date().toLocaleDateString('zh-CN', { month: 'long', day: 'numeric', weekday: 'short' })}</span></footer>
  </div>
  <dialog id="task-dialog">
    <form method="dialog" id="task-form" class="dlg-card">
      <h2>新任务</h2>
      <label>标题<input name="title" required maxlength="200" placeholder="要做点什么？" autofocus /></label>
      <label>预计（小时，可选）<input name="estimated" type="number" min="0" step="0.5" placeholder="例如 2" /></label>
      <label>备注<textarea name="notes" rows="2" placeholder="可选"></textarea></label>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>取消</button><button class="ok" value="default" type="submit">贴上</button></div>
    </form>
  </dialog>
  <dialog id="block-dialog">
    <form method="dialog" id="block-form" class="dlg-card">
      <h2>标记阻塞</h2>
      <p class="block-hint" id="block-task-title"></p>
      <label>原因<textarea name="reason" rows="2" required maxlength="300" placeholder="例如：等待接口文档"></textarea></label>
      <label>补充备注<textarea name="note" rows="2" placeholder="可选"></textarea></label>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>取消</button><button class="ok" value="default" type="submit">确认阻塞</button></div>
    </form>
  </dialog>
  <div id="toast" class="toast" role="status"></div></div>`;
  document.querySelector('#btn-new')?.addEventListener('click', openTaskDialog);
  document.querySelector('#btn-pin')?.addEventListener('click', togglePin);
  document.querySelector('#btn-min')?.addEventListener('click', () => theWindow()?.minimize());
  document.querySelector('#btn-close')?.addEventListener('click', () => theWindow()?.close());
  document.querySelectorAll('.sec-head').forEach((head) => head.addEventListener('click', () => {
    const key = head.closest('.sec').dataset.section;
    collapsed[key] = !collapsed[key];
    localStorage.setItem(COLLAPSE_KEY, JSON.stringify(collapsed));
    render();
  }));
  document.querySelector('#task-form')?.addEventListener('submit', handleCreateTask);
  document.querySelector('#block-form')?.addEventListener('submit', handleBlockSubmit);
  document.querySelectorAll('[data-action]').forEach((button) => button.addEventListener('click', () => handleAction(button)));
}

function openTaskDialog() {
  const form = document.querySelector('#task-form');
  form.reset();
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
    await call('create_task', { input: { title: data.get('title'), notes: data.get('notes') || null, estimatedActiveMinutes: parseEstimatedMinutes(data.get('estimated')) } });
    event.currentTarget.closest('dialog').close();
    await loadTasks();
    notify('便签已贴上');
  } catch (error) { notify(error.message || '创建任务失败'); }
}

async function handleAction(button) {
  const action = button.dataset.action;
  const id = button.dataset.id;
  const version = Number(button.dataset.version);
  try {
    if (action === 'complete') { await call('complete_task', { id, expectedVersion: version }); notify('任务完成 ✦'); }
    if (action === 'work') { await call('start_work', { taskId: id }); notify('开始计时 ▶'); }
    if (action === 'delete') { if (!window.confirm('确定删除这个任务吗？')) return; await call('delete_task', { id, expectedVersion: version }); notify('便签已撕掉'); }
    if (action === 'block') { openBlockDialog(id); return; }
    if (action === 'unblock') { await call('unblock_task', { blockId: button.dataset.blockId, expectedVersion: Number(button.dataset.blockVersion) }); notify('已解除阻塞'); }
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
  if (command === 'list_tasks') return state.tasks;
  if (command === 'create_task') { state.tasks.unshift({ id: crypto.randomUUID(), title: args.input.title, notes: args.input.notes, estimated_active_minutes: args.input.estimatedActiveMinutes, status: 'pending', sort_order: 0, updated_at: new Date().toISOString(), version: 1, is_blocked: false }); return state.tasks[0]; }
  if (command === 'complete_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { task.status = 'completed'; task.version += 1; task.updated_at = new Date().toISOString(); } }
  if (command === 'delete_task') state.tasks = state.tasks.filter((item) => item.id !== args.id);
  if (command === 'start_work') { const task = state.tasks.find((item) => item.id === args.taskId); if (task) { task.status = 'in_progress'; task.version += 1; } }
  if (command === 'list_blocks') { const task = state.tasks.find((item) => item.id === args.taskId); return task?.activeBlock ? [task.activeBlock] : []; }
  if (command === 'block_task') { const task = state.tasks.find((item) => item.id === args.taskId); if (task) { task.is_blocked = true; task.activeBlock = { id: crypto.randomUUID(), task_id: task.id, started_at: new Date().toISOString(), ended_at: null, reason: args.reason, note: args.note ?? null, created_at: new Date().toISOString(), updated_at: new Date().toISOString(), version: 1, deleted_at: null }; } }
  if (command === 'unblock_task') { for (const task of state.tasks) { if (task.activeBlock?.id === args.blockId) { task.is_blocked = false; delete task.activeBlock; } } }
}

const SAMPLES_KEY = 'cardha…es.v1';
async function seedSamples() {
  try {
    const a = await call('create_task', { input: { title: '晨间复盘 + 规划今日三件事', notes: null, estimatedActiveMinutes: 30 } });
    if (a?.id) await call('start_work', { taskId: a.id });
    await call('create_task', { input: { title: '给妈妈准备生日礼物', notes: null, estimatedActiveMinutes: 60 } });
    const c = await call('create_task', { input: { title: '预约牙科洁牙', notes: null, estimatedActiveMinutes: null } });
    if (c?.id) await call('block_task', { taskId: c.id, reason: '诊所周末号源约满，等放号', note: null });
    const d = await call('create_task', { input: { title: '读完《置身事内》最后两章', notes: null, estimatedActiveMinutes: 180 } });
    if (d?.id) await call('complete_task', { id: d.id, expectedVersion: d.version });
  } catch {}
}

async function loadTasks() {
  state.tasks = await call('list_tasks');
  if (!localStorage.getItem(SAMPLES_KEY)) { localStorage.setItem(SAMPLES_KEY, '1'); if (!state.tasks.length) { await seedSamples(); state.tasks = await call('list_tasks'); } }
  state.blocksByTask = {};
  await Promise.all(state.tasks.filter((task) => task.is_blocked).map(async (task) => {
    const blocks = await call('list_blocks', { taskId: task.id });
    const active = blocks.find((block) => !block.ended_at);
    if (active) state.blocksByTask[task.id] = active;
  }));
  render();
}

function notify(message) {
  const toast = document.querySelector('#toast');
  if (!toast) return;
  toast.textContent = message;
  toast.classList.add('show');
  clearTimeout(notify._t);
  notify._t = setTimeout(() => toast.classList.remove('show'), 2200);
}

document.addEventListener('click', (e) => { const closer = e.target.closest('[data-close]'); if (closer) closer.closest('dialog').close(); });

render();
(async () => { const w = theWindow(); if (w) { try { pinned = await w.isAlwaysOnTop(); render(); } catch {} } })();
loadTasks().catch((error) => notify(error.message || '无法加载任务'));
