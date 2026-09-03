import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { PhysicalPosition } from '@tauri-apps/api/dpi';
import './style.css';

const app = document.querySelector('#app');
const state = { tasks: [], blocksByTask: {}, sessionsByTask: {}, sessionMinutesByTask: {}, workspaces: [], prios: [], activeWs: null, blockingTaskId: null };
const COLLAPSE_KEY = 'cardha…e.v2';
const OPACITY_KEY = 'cardhannis.sticky.opacity.v1';
const DOCK_KEY = 'cardhannis.sticky.dock.v1';
let collapsed = {};
try { collapsed = JSON.parse(localStorage.getItem(COLLAPSE_KEY) || '{}'); } catch {}
let pinned = true;
const ICONS = {
  logo: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><g transform="translate(32 0)"><path fill="currentColor" d="M48 32C21.5 32 0 53.5 0 80v352c0 26.5 21.5 48 48 48h352c26.5 0 48-21.5 48-48V80c0-26.5-21.5-48-48-48zm16 64h106.668v53.334h-53.334v213.332H224V416H64zm160 0h160v320H277.332v-53.334h53.334V149.334H224z"/></g></svg>',
  start: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M6 18V6h2v12zm4 0l10-6l-10-6z"/></svg>',
  pause: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="currentColor" d="M9 3a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Zm8 0a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1h-2a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z"/></svg>',
  done: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="m9 10l3.258 2.444a1 1 0 0 0 1.353-.142L20 5"/><path d="M21 12a9 9 0 1 1-6.67-8.693"/></g></svg>',
  block: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="m15 2l6 6m0-6l-6 6"/><circle cx="6" cy="19" r="3"/><path d="M12 5H8.5a3.5 3.5 0 1 0 0 7h7a3.5 3.5 0 1 1 0 7H12"/></g></svg>',
  add: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5.25v13.5M18.75 12H5.25"/></svg>',
  pin: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g fill="currentColor" transform="scale(0.6667)"><path d="M30 30H6V6h16V4H6a2 2 0 0 0-2 2v24a2 2 0 0 0 2 2h24a2 2 0 0 0 2-2V14h-2Z"/><path d="m33.57 9.33l-7-7a1 1 0 0 0-1.41 1.41l1.38 1.38l-4 4c-2-.87-4.35.14-5.92 1.68l-.72.71l3.54 3.54l-3.67 3.67l1.41 1.41l3.67-3.67L24.37 20l.71-.72c1.54-1.57 2.55-3.91 1.68-5.92l4-4l1.38 1.38a1 1 0 1 0 1.41-1.41Z"/></g></svg>',
  settings: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2"/><circle cx="12" cy="12" r="3"/></g></svg>',
};
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
function fmtDateTime(value) {
  if (!value) return '—';
  const d = new Date(value);
  const q = (n) => String(n).padStart(2, '0');
  return `${q(d.getMonth() + 1)}-${q(d.getDate())} ${q(d.getHours())}:${q(d.getMinutes())}`;
}
function fmtDuration(minutes) {
  if (!minutes) return '—';
  if (minutes < 60) return `${minutes}m`;
  const h = minutes / 60;
  return `${Number.isInteger(h) ? h : h.toFixed(1)}h`;
}
function prioName(task) {
  return (state.prios.find((p) => p.id === task.priority_id) || {}).name || '—';
}
function pillInfo(task) {
  if (task.is_blocked) return { cls: 'blocked', label: '阻塞' };
  return { in_progress: { cls: 'in_progress', label: '进行' }, completed: { cls: 'completed', label: '完成' } }[task.status] || { cls: 'pending', label: '待办' };
}
const prioColor = (p, i) => p.color || PRIO_PALETTE[i % PRIO_PALETTE.length];

function row(task) {
  const done = task.status === 'completed';
  const block = state.blocksByTask[task.id];
  const pill = pillInfo(task);
  const tip = escapeHtml([task.title, task.is_blocked && block && block.reason ? `阻塞：${block.reason}` : '', task.notes || '', task.estimated_active_minutes != null ? formatEstimatedHours(task.estimated_active_minutes) : ''].filter(Boolean).join(' ｜ '));
  const d = `data-id="${task.id}"`;
  const v = `data-version="${task.version}"`;
  let actions = '';
  if (done) {
    actions = `<button class="nb" data-action="reopen" ${d} ${v} title="重新打开" type="button">↺</button>`;
  } else if (task.is_blocked) {
    actions = `${block ? `<button class="nb" data-action="unblock" data-block-id="${block.id}" data-block-version="${block.version}" title="解除阻塞 → 等待中" type="button">⏏</button>` : ''}
      <button class="nb ok" data-action="complete" ${d} ${v} title="完成任务" type="button">${ICONS.done}</button>`;
  } else if (task.status === 'in_progress') {
    actions = `<button class="nb" data-action="pause" ${d} ${v} title="暂停（回到待办）" type="button">${ICONS.pause}</button>
      <button class="nb" data-action="block" ${d} title="标记阻塞" type="button">${ICONS.block}</button>
      <button class="nb ok" data-action="complete" ${d} ${v} title="完成任务" type="button">${ICONS.done}</button>`;
  } else {
    actions = `<button class="nb" data-action="work" ${d} title="开始工作（计时）" type="button">${ICONS.start}</button>
      <button class="nb" data-action="block" ${d} title="标记阻塞" type="button">${ICONS.block}</button>
      <button class="nb ok" data-action="complete" ${d} ${v} title="完成任务" type="button">${ICONS.done}</button>`;
  }
  const statusSlot = done
    ? `<span class="done-meta" title="分级 · 完成时间 · 实际工作时间">${escapeHtml(prioName(task))} · ${fmtDateTime(task.completed_at)} · ${fmtDuration(state.sessionMinutesByTask[task.id])}</span>`
    : `<span class="pill ${pill.cls}">${pill.label}</span>`;
  return `<div class="row ${done ? 'done' : ''}" data-id="${task.id}" data-version="${task.version}" title="${tip}">
    <span class="rt">${escapeHtml(task.title)}</span>
    ${statusSlot}
    <span class="ra">${actions}</span>
  </div>`;
}

function render() {
  const opacity = Number(localStorage.getItem(OPACITY_KEY) || 100);
  const ws = state.workspaces.find((w) => w.id === state.activeWs) || state.workspaces[0];
  if (ws) state.activeWs = ws.id;
  const wsTasks = state.tasks.filter((t) => t.workspace_id === (ws ? ws.id : null));
  const isDoneWs = ws ? ws.id === 'done' : false;
  const doneRows = isDoneWs ? [...wsTasks].sort((a, b) => (b.updated_at || '').localeCompare(a.updated_at || '')) : [];
  const groups = state.prios.map((p, i) => ({ p, color: prioColor(p, i), tasks: wsTasks.filter((t) => t.priority_id === p.id) }));
  const unsorted = wsTasks.filter((t) => !state.prios.some((p) => p.id === t.priority_id));
  app.innerHTML = `<div class="win"><div class="win-sheet">
    <header class="win-bar">
      <span class="win-brand">${ICONS.logo}CardHannis</span>
      <div class="win-tools">
        <button id="btn-new" title="新建任务" type="button">${ICONS.add}</button>
        <button id="btn-settings" title="设置" type="button">${ICONS.settings}</button>
        <button id="btn-pin" class="${pinned ? 'on' : ''}" title="切换窗口置顶" type="button">${ICONS.pin}</button>
        <button id="btn-min" title="最小化" type="button">−</button>
        <button id="btn-close" title="关闭" type="button">✕</button>
      </div>
    </header>
    <nav class="ws-row">
      <div class="ws-tabs">${state.workspaces.map((w) => {
        const open = w.id === 'done' ? state.tasks.filter((t) => t.workspace_id === w.id).length : state.tasks.filter((t) => t.workspace_id === w.id && t.status !== 'completed').length;
        return `<button class="ws-tab ${w.id === state.activeWs ? 'active' : ''}" data-ws="${w.id}" type="button">${escapeHtml(w.name)}<b>${open}</b></button>`;
      }).join('')}<button class="ws-tool" id="ws-add" title="新建工作区" type="button">＋</button></div>
    </nav>
    <div class="win-body">
      ${isDoneWs ? (doneRows.length ? `<div class="rows">${doneRows.map((t) => row(t)).join('')}</div>` : '<div class="empty-hint">还没有完成的任务</div>') : [...groups, unsorted.length ? { p: { id: 'none', name: '未分级' }, color: '#a8a89a', tasks: unsorted } : null].filter(Boolean).map(({ p, color, tasks }) => {
        const key = `${state.activeWs}:${p.id}`;
        const closed = collapsed[key];
        return `<section class="sec ${closed ? 'collapsed' : ''}" data-prio="${p.id}">
          <div class="sec-row">
            <button class="sec-head" type="button" data-toggle="${p.id}"><span class="sec-chev">▼</span><span class="sec-dot" style="background:${color}"></span><span>${escapeHtml(p.name)}</span><span class="sec-count">${tasks.length}</span></button>
            ${p.id !== 'none' ? `<span class="sec-tools">
              <button class="nb" data-gact="grp-add" data-prio="${p.id}" title="在此分级新任务" type="button">＋</button>
              <button class="nb" data-gact="grp-rename" data-prio="${p.id}" data-version="${p.version}" title="重命名分级" type="button">✎</button>
            </span>` : ''}
          </div>
          <div class="rows">${tasks.length ? tasks.map((t) => row(t)).join('') : '<div class="row-empty">暂无任务，点 ＋ 添加</div>'}</div>
        </section>`;
      }).join('')}
    </div>
    <footer class="win-foot"><span class="dot"></span><span>${isTauri() ? '本地数据库' : '浏览器预览'}</span><span class="foot-right">${new Date().toLocaleDateString('zh-CN', { month: 'long', day: 'numeric', weekday: 'short' })}</span></footer>
  </div>
  </div>
  <dialog id="task-dialog">
    <form method="dialog" id="task-form" class="dlg-card">
      <h2>新任务</h2>
      <label>标题<input name="title" required maxlength="200" placeholder="要做点什么？" autofocus /></label>
      <label>预计（小时，可选）<input name="estimated" type="number" min="0" step="0.5" placeholder="例如 2" /></label>
      <label>备注<textarea name="notes" rows="2" placeholder="可选"></textarea></label>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>取消</button><button class="ok" id="task-ok" type="button">贴上</button></div>
    </form>
  </dialog>
  <dialog id="block-dialog">
    <form method="dialog" id="block-form" class="dlg-card">
      <h2>标记阻塞</h2>
      <p class="block-hint" id="block-task-title"></p>
      <label>原因<textarea name="reason" rows="2" required maxlength="300" placeholder="例如：等待接口文档"></textarea></label>
      <label>补充备注<textarea name="note" rows="2" placeholder="可选"></textarea></label>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>取消</button><button class="ok" id="block-ok" type="button">确认阻塞</button></div>
    </form>
  </dialog>
  <dialog id="prompt-dialog">
    <form method="dialog" class="dlg-card">
      <h2 id="prompt-title"></h2>
      <label><span id="prompt-label"></span><input id="prompt-input" maxlength="60" /></label>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>取消</button><button class="ok" id="prompt-ok" type="button">确定</button></div>
    </form>
  </dialog>
  <dialog id="confirm-dialog">
    <form method="dialog" class="dlg-card">
      <h2 id="confirm-title"></h2>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>取消</button><button class="ok" id="confirm-ok" type="button" style="background:#a5503f">确定</button></div>
    </form>
  </dialog>
  <dialog id="settings-dialog">
    <form method="dialog" class="dlg-card">
      <h2>设置</h2>
      <label>窗口不透明度 <span id="opacity-val">${opacity}%</span>
        <input type="range" id="opacity-range" min="40" max="100" step="5" value="${opacity}" />
      </label>
      <label class="set-check"><input type="checkbox" id="dock-check" ${docked ? 'checked' : ''} /> 贴边自动收起（标题栏 🧲 同开关）</label>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>关闭</button></div>
    </form>
  </dialog>
  <div id="toast" class="toast" role="status"></div>`;

  document.querySelector('.win-sheet').style.opacity = (opacity / 100).toFixed(2);
  document.querySelector('#btn-new')?.addEventListener('click', () => openTaskDialog(null));
  document.querySelector('#btn-settings')?.addEventListener('click', () => document.querySelector('#settings-dialog').showModal());
  document.querySelector('#opacity-range')?.addEventListener('input', (e) => {
    const v = Number(e.target.value);
    localStorage.setItem(OPACITY_KEY, String(v));
    document.querySelector('#opacity-val').textContent = `${v}%`;
    document.querySelector('.win-sheet').style.opacity = (v / 100).toFixed(2);
  });
  document.querySelector('#dock-check')?.addEventListener('change', (e) => { setDock(e.target.checked); });
  document.querySelector('#btn-pin')?.addEventListener('click', togglePin);
  document.querySelector('#btn-min')?.addEventListener('click', () => theWindow()?.minimize());
  document.querySelector('#btn-close')?.addEventListener('click', () => theWindow()?.close());
  document.querySelector('#ws-add')?.addEventListener('click', addWorkspace);
  document.querySelectorAll('.ws-tab').forEach((tab) => tab.addEventListener('click', () => {
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
  document.querySelectorAll('[data-action]').forEach((button) => button.addEventListener('click', () => handleAction(button)));
}

async function addWorkspace() {
  const trimmed = await openPrompt('新建工作区', '名称');
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
  const ok = await openConfirm(`删除工作区「${ws ? ws.name : ''}」？其中任务会一并移除。`);
  if (!ok) return;
  try {
    await call('delete_workspace', { id, expectedVersion });
    await loadMeta();
    if (state.activeWs === id) state.activeWs = state.workspaces[0]?.id || null;
    render();
    notify('工作区已删除');
  } catch (error) { notify(error.message || '删除失败'); }
}
async function addPriority() {
  const trimmed = await openPrompt('添加分级', '名称（如：P3 / 紧急）');
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
      const trimmed = await openPrompt('重命名分级', '名称', prio ? prio.name : '');
      if (!trimmed || !prio) return;
      await call('update_priority', { id: prioId, expectedVersion: prio.version, name: trimmed, color: prio.color });
      await loadMeta(); render(); notify('分级已重命名'); return;
    }
    if (button.dataset.gact === 'grp-del') {
      return;
      await call('delete_priority', { id: prioId, expectedVersion: prio.version });
      await loadMeta(); render(); notify('分级已删除');
    }
  } catch (error) { notify(error.message || '操作失败'); }
}

function openTaskDialog(prioId) {
  state.newTaskTarget = { ws: state.activeWs, prio: prioId || state.prios[0]?.id || null };
  const form = document.querySelector('#task-form');
  form.reset();
  document.querySelector('#task-dialog').showModal();
}

async function submitTask() {
  const form = document.querySelector('#task-form');
  const title = form.querySelector('[name="title"]').value.trim();
  if (!title) { notify('标题不能为空'); return; }
  const target = state.newTaskTarget || { ws: state.activeWs, prio: state.prios[0]?.id || null };
  try {
    await call('create_task', {
      input: {
        title,
        notes: form.querySelector('[name="notes"]').value.trim() || null,
        estimatedActiveMinutes: parseEstimatedMinutes(form.querySelector('[name="estimated"]').value),
        workspaceId: target.ws,
        priorityId: target.prio,
      },
    });
    document.querySelector('#task-dialog').close();
    await loadTasks();
    notify('任务已添加');
  } catch (error) { notify(error.message || '创建任务失败'); }
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


async function handleAction(button) {
  const action = button.dataset.action;
  const id = button.dataset.id;
  const version = Number(button.dataset.version);
  try {
    if (action === 'complete') { await call('complete_task', { id, expectedVersion: version }); notify('任务完成 ✦'); }
    if (action === 'reopen') { await call('reopen_task', { id, expectedVersion: version }); notify('任务已重新打开'); }
    if (action === 'work') { await call('start_work', { taskId: id }); notify('开始计时 ▶'); }
    if (action === 'pause') {
      const session = state.sessionsByTask[id];
      if (session) { try { await call('finish_work', { sessionId: session.id }); } catch {} }
      await call('pause_task', { id, expectedVersion: version });
      notify('已暂停，回到待办');
    }
    if (action === 'block') { openBlockDialog(id); return; }
    if (action === 'unblock') { await call('unblock_task', { blockId: button.dataset.blockId, expectedVersion: Number(button.dataset.blockVersion) }); notify('已解除阻塞，进入等待中'); }
    await loadTasks();
  } catch (error) { notify(error.message || '操作失败'); }
}

function openBlockDialog(taskId) {
  state.blockingTaskId = taskId;
  const task = state.tasks.find((item) => item.id === taskId);
  const form = document.querySelector('#block-form');
  form.querySelector('[name="reason"]').value = '';
  form.querySelector('[name="note"]').value = '';
  document.querySelector('#block-task-title').textContent = task ? task.title : '';
  document.querySelector('#block-dialog').showModal();
}

async function submitBlock() {
  const form = document.querySelector('#block-form');
  const reason = form.querySelector('[name="reason"]').value.trim();
  if (!reason) { notify('阻塞原因不能为空'); return; }
  if (!state.blockingTaskId) return;
  try {
    await call('block_task', { taskId: state.blockingTaskId, reason, note: form.querySelector('[name="note"]').value.trim() || null });
    document.querySelector('#block-dialog').close();
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
  if (command === 'rename_workspace') { const w = state.workspaces.find((x) => x.id === args.id); if (w) { w.name = args.name; w.version += 1; } return w; }
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
    const task = { id: crypto.randomUUID(), title: args.input.title, notes: args.input.notes, estimated_active_minutes: args.input.estimatedActiveMinutes, status: 'pending', sort_order: 0, updated_at: new Date().toISOString(), version: 1, is_blocked: false, workspace_id: args.input.workspaceId, priority_id: args.input.priorityId, home_workspace_id: args.input.workspaceId };
    state.tasks.unshift(task);
    return task;
  }
  if (command === 'complete_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { task.status = 'completed'; task.completed_at = new Date().toISOString(); task.version += 1; task.updated_at = task.completed_at; task.workspace_id = 'done'; } }
  if (command === 'reopen_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { task.status = 'pending'; task.completed_at = null; task.version += 1; task.workspace_id = task.home_workspace_id || 'daily'; } }
  if (command === 'pause_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { task.status = 'pending'; task.version += 1; task.updated_at = new Date().toISOString(); } }
  if (command === 'finish_work') { for (const task of state.tasks) { if (task.activeSession?.id === args.sessionId) delete task.activeSession; } }
  if (command === 'list_sessions') { const task = state.tasks.find((item) => item.id === args.taskId); return task?.activeSession ? [task.activeSession] : []; }
  if (command === 'delete_task') state.tasks = state.tasks.filter((item) => item.id !== args.id);
  if (command === 'start_work') { const task = state.tasks.find((item) => item.id === args.taskId); if (task) { task.status = 'in_progress'; task.version += 1; task.activeSession = { id: crypto.randomUUID(), task_id: task.id, started_at: new Date().toISOString(), ended_at: null }; } }
  if (command === 'list_blocks') { const task = state.tasks.find((item) => item.id === args.taskId); return task?.activeBlock ? [task.activeBlock] : []; }
  if (command === 'block_task') { const task = state.tasks.find((item) => item.id === args.taskId); if (task) { task.is_blocked = true; task.activeBlock = { id: crypto.randomUUID(), task_id: task.id, started_at: new Date().toISOString(), ended_at: null, reason: args.reason, note: args.note ?? null, created_at: new Date().toISOString(), updated_at: new Date().toISOString(), version: 1, deleted_at: null }; } }
  if (command === 'unblock_task') { for (const task of state.tasks) { if (task.activeBlock?.id === args.blockId) { task.is_blocked = false; delete task.activeBlock; task.status = 'pending'; task.version += 1; } } }
}
function seedPreviewMeta() {
  state.workspaces = [
    { id: 'daily', name: '日常', sort_order: 0, builtin: true, version: 1 },
    { id: 'work', name: '工作', sort_order: 1, builtin: true, version: 1 },
    { id: 'done', name: '已完成', sort_order: 99, builtin: true, version: 1 },
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
  } else if (!state.workspaces.length) {
    seedPreviewMeta();
  }
  if (!state.activeWs) state.activeWs = state.workspaces[0]?.id || null;
}

async function loadTasks() {
  state.tasks = await call('list_tasks');
  state.blocksByTask = {};
  state.sessionsByTask = {};
  state.sessionMinutesByTask = {};
  await Promise.all(state.tasks.filter((task) => task.is_blocked).map(async (task) => {
    const blocks = await call('list_blocks', { taskId: task.id });
    const active = blocks.find((block) => !block.ended_at);
    if (active) state.blocksByTask[task.id] = active;
  }));
  await Promise.all(state.tasks.filter((task) => task.status === 'in_progress' && !task.is_blocked).map(async (task) => {
    const sessions = await call('list_sessions', { taskId: task.id });
    const active = sessions.find((session) => !session.ended_at);
    if (active) state.sessionsByTask[task.id] = active;
  }));
  await Promise.all(state.tasks.filter((task) => task.status === 'completed').map(async (task) => {
    const sessions = await call('list_sessions', { taskId: task.id });
    const minutes = sessions.filter((x) => x.ended_at).reduce((acc, x) => acc + Math.max(0, Math.round((Date.parse(x.ended_at) - Date.parse(x.started_at)) / 60000)), 0);
    state.sessionMinutesByTask[task.id] = minutes;
  }));
  render();
}


// ===== 应用内 prompt / confirm（webview 无原生对话框） =====
let promptResolve = null;
let confirmResolve = null;
function openPrompt(title, label, value = '') {
  return new Promise((resolve) => {
    promptResolve = resolve;
    document.querySelector('#prompt-title').textContent = title;
    document.querySelector('#prompt-label').textContent = label;
    const input = document.querySelector('#prompt-input');
    input.value = value;
    document.querySelector('#prompt-dialog').showModal();
    input.focus();
  });
}
function openConfirm(title) {
  return new Promise((resolve) => {
    confirmResolve = resolve;
    document.querySelector('#confirm-title').textContent = title;
    document.querySelector('#confirm-dialog').showModal();
  });
}
document.addEventListener('click', (e) => {
  if (e.target.closest('#prompt-ok')) {
    const v = document.querySelector('#prompt-input').value.trim();
    document.querySelector('#prompt-dialog').close();
    if (promptResolve) { promptResolve(v || null); promptResolve = null; }
    return;
  }
  if (e.target.closest('#prompt-dialog [data-close]')) {
    if (promptResolve) { promptResolve(null); promptResolve = null; }
    return;
  }
  if (e.target.closest('#confirm-ok')) {
    document.querySelector('#confirm-dialog').close();
    if (confirmResolve) { confirmResolve(true); confirmResolve = null; }
    return;
  }
  if (e.target.closest('#confirm-dialog [data-close]')) {
    if (confirmResolve) { confirmResolve(false); confirmResolve = null; }
    return;
  }
  if (e.target.closest('#task-ok')) { submitTask(); return; }
  if (e.target.closest('#block-ok')) { submitBlock(); return; }
});
document.querySelector('#prompt-dialog')?.addEventListener('close', () => { if (promptResolve) { promptResolve(null); promptResolve = null; } });
document.querySelector('#confirm-dialog')?.addEventListener('close', () => { if (confirmResolve) { confirmResolve(false); confirmResolve = null; } });

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
function openWorkspaceMenu(x, y, ws) {
  closeContextMenu();
  const menu = document.createElement('div');
  menu.className = 'ctx-menu';
  menu.innerHTML = `<button type="button" data-ctx="ws-rename">✎ 重命名工作区</button><button type="button" data-ctx="ws-del">🗑 删除工作区</button>`;
  document.body.appendChild(menu);
  menu.style.left = `${Math.min(x, window.innerWidth - menu.offsetWidth - 6)}px`;
  menu.style.top = `${Math.min(y, window.innerHeight - menu.offsetHeight - 6)}px`;
  menu.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-ctx]');
    if (!btn) return;
    closeContextMenu();
    if (btn.dataset.ctx === 'ws-rename') {
      const name = await openPrompt('重命名工作区', '名称', ws.name);
      if (!name) return;
      try {
        await call('rename_workspace', { id: ws.id, expectedVersion: ws.version, name });
        await loadMeta(); render(); notify('工作区已重命名');
      } catch (error) { notify(error.message || '重命名失败'); }
    } else if (btn.dataset.ctx === 'ws-del') {
      await removeWorkspace(ws.id, ws.version);
    }
  });
  ctxMenu = menu;
}
function openBodyMenu(x, y) {
  closeContextMenu();
  const menu = document.createElement('div');
  menu.className = 'ctx-menu';
  menu.innerHTML = `<button type="button" data-ctx="prio-add">＋ 添加分级</button>`;
  document.body.appendChild(menu);
  menu.style.left = `${Math.min(x, window.innerWidth - menu.offsetWidth - 6)}px`;
  menu.style.top = `${Math.min(y, window.innerHeight - menu.offsetHeight - 6)}px`;
  menu.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-ctx]');
    if (!btn) return;
    closeContextMenu();
    await addPriority();
  });
  ctxMenu = menu;
}
function openPrioMenu(x, y, prioId) {
  closeContextMenu();
  const menu = document.createElement('div');
  menu.className = 'ctx-menu';
  menu.innerHTML = `<button type="button" data-ctx="prio-del">🗑 删除分级</button>`;
  document.body.appendChild(menu);
  menu.style.left = `${Math.min(x, window.innerWidth - menu.offsetWidth - 6)}px`;
  menu.style.top = `${Math.min(y, window.innerHeight - menu.offsetHeight - 6)}px`;
  menu.addEventListener('click', async (e) => {
    const btn = e.target.closest('[data-ctx]');
    if (!btn) return;
    closeContextMenu();
    const prio = state.prios.find((p) => p.id === prioId);
    if (!prio) return;
    const ok = await openConfirm(`删除空分级「${prio.name}」？`);
    if (!ok) return;
    try {
      await call('delete_priority', { id: prioId, expectedVersion: prio.version });
      await loadMeta();
      render();
      notify('分级已删除');
    } catch (error) { notify(error.message || '删除失败'); }
  });
  ctxMenu = menu;
}
async function deleteTask(id, expectedVersion) {
  const ok = await openConfirm('确定删除这个任务吗？');
  if (!ok) return;
  try {
    await call('delete_task', { id, expectedVersion });
    await loadTasks();
    notify('任务已删除');
  } catch (error) { notify(error.message || '删除失败'); }
}
document.addEventListener('contextmenu', (e) => {
  const r = e.target.closest('.row');
  if (r) {
    e.preventDefault();
    openContextMenu(e.clientX, e.clientY, r.dataset.id, Number(r.dataset.version));
    return;
  }
  const wtab = e.target.closest('.ws-tab');
  if (wtab && !wtab.classList.contains('active') === false || wtab) {
    const wsId = wtab.dataset.ws;
    const ws = state.workspaces.find((w) => w.id === wsId);
    if (ws && !ws.builtin) {
      e.preventDefault();
      openWorkspaceMenu(e.clientX, e.clientY, ws);
    }
    return;
  }
  if (e.target.closest('.win-body') && !e.target.closest('.row') && !e.target.closest('.sec-row')) {
    e.preventDefault();
    openBodyMenu(e.clientX, e.clientY);
    return;
  }
  const head = e.target.closest('.sec-row');
  if (head) {
    const prioId = head.closest('.sec').dataset.prio;
    if (prioId && prioId !== 'none') {
      e.preventDefault();
      openPrioMenu(e.clientX, e.clientY, prioId);
    }
    return;
  }
  closeContextMenu();
});
document.addEventListener('click', (e) => {
  if (ctxMenu && !ctxMenu.contains(e.target)) closeContextMenu();
});
window.addEventListener('blur', closeContextMenu);

document.addEventListener('click', (e) => {
  const closer = e.target.closest('[data-close]');
  if (closer) closer.closest('dialog')?.close();
});

// ===== 贴边自动收起 =====
let docked = localStorage.getItem(DOCK_KEY) !== '0';
let dockEdge = null;
let savedPos = null;

async function setDock(on) {
  localStorage.setItem(DOCK_KEY, on ? '1' : '0');
  const w = theWindow();
  if (!w) { docked = on; render(); return; }
  docked = on;
  if (on) {
    await computeEdge(w);
    notify('已开启贴边收起：鼠标移开窗口会收进屏幕边');
  } else {
    if (savedPos) { try { await w.setPosition(savedPos); } catch {} }
    savedPos = null;
    notify('已关闭贴边收起');
  }
  render();
}
function toggleDock() { setDock(!docked); }

async function computeEdge(w) {
  try {
    const mon = await w.currentMonitor();
    const pos = await w.outerPosition();
    const size = await w.outerSize();
    if (!mon) return;
    const mx = mon.position.x, my = mon.position.y, mw = mon.size.width, mh = mon.size.height;
    const dLeft = pos.x - mx;
    const dRight = (mx + mw) - (pos.x + size.width);
    const dTop = pos.y - my;
    const dBottom = (my + mh) - (pos.y + size.height);
    const min = Math.min(dLeft, dRight, dTop, dBottom);
    dockEdge = min === dLeft ? 'left' : min === dRight ? 'right' : min === dTop ? 'top' : 'bottom';
    savedPos = pos;
  } catch {}
}

async function tuckWindow() {
  const w = theWindow();
  if (!w || !docked || savedPos === null) return;
  if (dragState || document.querySelector('dialog[open]')) return;
  try {
    const mon = await w.currentMonitor();
    const size = await w.outerSize();
    if (!mon) return;
    const scale = mon.scaleFactor || 1;
    const sliver = Math.round(12 * scale);
    let { x, y } = savedPos;
    if (dockEdge === 'left') x = mon.position.x - size.width + sliver;
    if (dockEdge === 'right') x = mon.position.x + mon.size.width - sliver;
    if (dockEdge === 'top') y = mon.position.y - size.height + sliver;
    if (dockEdge === 'bottom') y = mon.position.y + mon.size.height - sliver;
    await w.setPosition(new PhysicalPosition(x, y));
  } catch {}
}

async function restoreWindow() {
  const w = theWindow();
  if (!w || !docked || savedPos === null) return;
  try { await w.setPosition(savedPos); } catch {}
}

document.addEventListener('mouseleave', () => { if (docked) tuckWindow(); });
document.addEventListener('mouseenter', () => { if (docked) restoreWindow(); });


// ===== 自绘拖拽（避免 macOS 边缘半屏吸附） =====
let dragState = null;
document.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  const bar = e.target.closest('.win-bar');
  if (!bar || e.target.closest('button')) return;
  const w = theWindow();
  if (!w) return;
  e.preventDefault();
  w.outerPosition().then((pos) => {
    dragState = { sx: e.screenX, sy: e.screenY, px: pos.x, py: pos.y };
  }).catch(() => {});
});
document.addEventListener('mousemove', (e) => {
  if (!dragState) return;
  const w = theWindow();
  if (!w) return;
  const scale = window.devicePixelRatio || 1;
  w.setPosition(new PhysicalPosition(
    Math.round(dragState.px + (e.screenX - dragState.sx) * scale),
    Math.round(dragState.py + (e.screenY - dragState.sy) * scale),
  )).catch(() => {});
});
document.addEventListener('mouseup', () => {
  if (!dragState) return;
  dragState = null;
  if (docked) { const w = theWindow(); if (w) computeEdge(w); }
});

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
