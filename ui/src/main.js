import { invoke } from '@tauri-apps/api/core';
import { cursorPosition, getCurrentWindow } from '@tauri-apps/api/window';
import { LogicalPosition } from '@tauri-apps/api/dpi';
import './style.css';

const app = document.querySelector('#app');
const state = { tasks: [], blocksByTask: {}, sessionsByTask: {}, finishedSessionMinutesByTask: {}, sessionMinutesByTask: {}, workspaces: [], prios: [], activeWs: null, blockingTaskId: null, unblockingTaskId: null };
const COLLAPSE_KEY = 'cardha…e.v2';
const OPACITY_KEY = 'cardhannis.sticky.opacity.v1';
// v2 将用户确认的旧版 +2px 视觉大小固化为新的零点。
const FONT_SIZE_KEY = 'cardhannis.ui.font-delta.v2';
const FONT_SIZE_MIN = -2;
const FONT_SIZE_MAX = 1.5;
let unfocusedOpacity = normalizeOpacity(localStorage.getItem(OPACITY_KEY));
let fontSizeDelta = normalizeFontSizeDelta(localStorage.getItem(FONT_SIZE_KEY));
let mouseInside = false;
let mouseInTitleBar = false;
let zeroOpacityExpanded = false;
function normalizeOpacity(value) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return 100;
  return Math.min(100, Math.max(0, Math.round(parsed / 5) * 5));
}
function applyContentOpacity() {
  const expanded = unfocusedOpacity === 0 ? zeroOpacityExpanded : mouseInside;
  const opacity = expanded ? 100 : unfocusedOpacity;
  document.documentElement.style.setProperty('--content-opacity', (opacity / 100).toFixed(2));
  document.documentElement.classList.toggle('content-hidden', opacity === 0);
}
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
  clock: '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><g fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" stroke-width="2"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></g></svg>',
};
const PRIO_PALETTE = ['#5c7699', '#7a5ea6', '#3d735e', '#a0632f', '#8a5a7a', '#4e7d8a'];

function isTauri() { return Boolean(window.__TAURI_INTERNALS__); }
function theWindow() { return isTauri() ? getCurrentWindow() : null; }
function escapeHtml(value = '') { return String(value).replace(/[&<>'"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', "'": '&#039;', '"': '&quot;' }[c])); }
function errorMessage(error, fallback) {
  if (typeof error === 'string' && error.trim()) return error;
  if (error && typeof error.message === 'string' && error.message.trim()) return error.message;
  return fallback;
}
function normalizeFontSizeDelta(value) {
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) return 0;
  return Math.min(FONT_SIZE_MAX, Math.max(FONT_SIZE_MIN, Math.round(parsed * 2) / 2));
}
function formatFontSizeDelta(value) {
  if (value === 0) return '默认';
  return `${value > 0 ? '+' : ''}${value}px`;
}
function applyFontSize() {
  document.documentElement.style.setProperty('--ui-font-delta', `${fontSizeDelta}px`);
}
applyFontSize();
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
  if (minutes === null || minutes === undefined || Number.isNaN(Number(minutes))) return '—';
  const totalMinutes = Math.round(Number(minutes));
  if (totalMinutes < 60) return `${totalMinutes}m`;
  const hours = totalMinutes / 60;
  return `${Number.isInteger(hours) ? hours : hours.toFixed(1)}h`;
}
function sessionDurationMs(session, fallbackEndMs = Date.now()) {
  const startedAt = Date.parse(session.started_at);
  if (!Number.isFinite(startedAt)) return 0;
  const endedAt = session.ended_at
    ? Date.parse(session.ended_at)
    : fallbackEndMs;
  if (!Number.isFinite(endedAt)) return 0;
  return Math.max(0, endedAt - startedAt);
}
function activeMinutes(task, session) {
  if (!session) return 0;
  const fallback = task.status === 'completed' && task.completed_at
    ? Date.parse(task.completed_at)
    : Date.now();
  return Math.floor(sessionDurationMs(session, fallback) / 60000);
}
function totalSessionMinutes(task, sessions) {
  return Math.floor(sessions.reduce((acc, session) => acc + sessionDurationMs(
    session,
    task.status === 'completed' && task.completed_at ? Date.parse(task.completed_at) : Date.now(),
  ), 0) / 60000);
}
function prioName(task) {
  return (state.prios.find((p) => p.id === task.priority_id) || {}).name || '—';
}
function workspacePriorities(workspaceId = state.activeWs) {
  return state.prios.filter((priority) => priority.workspace_id === workspaceId);
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
    actions = `${block ? `<button class="nb" data-action="unblock" data-task-id="${task.id}" data-block-id="${block.id}" data-block-version="${block.version}" title="解除阻塞 → 待办" type="button">⏏</button>` : ''}
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
  const activeTimer = !done && task.status === 'in_progress' && !task.is_blocked
    ? `<span class="active-timer" data-active-timer="${task.id}" title="累计活动时间（含历史会话）">${ICONS.clock}<span>${fmtDuration(state.sessionMinutesByTask[task.id] || 0)}</span></span>` : '';
  const statusSlot = done
    ? `<span class="done-meta" title="分级 · 完成时间 · 实际工作时间">${escapeHtml(prioName(task))} · ${fmtDateTime(task.completed_at)} · ${fmtDuration(state.sessionMinutesByTask[task.id])}</span>`
    : `<span class="meta-pills">${activeTimer}<span class="pill ${pill.cls}">${pill.label}</span></span>`;
  return `<div class="row ${done ? 'done' : ''}" data-id="${task.id}" data-version="${task.version}" title="${tip}">
    <span class="rt">${escapeHtml(task.title)}</span>
    ${statusSlot}
    <span class="ra">${actions}</span>
  </div>`;
}

function render() {
  const opacity = normalizeOpacity(localStorage.getItem(OPACITY_KEY));
  const ws = state.workspaces.find((w) => w.id === state.activeWs) || state.workspaces[0];
  if (ws) state.activeWs = ws.id;
  const wsTasks = state.tasks.filter((t) => t.workspace_id === (ws ? ws.id : null));
  const isDoneWs = ws ? ws.id === 'done' : false;
  const doneRows = isDoneWs ? [...wsTasks].sort((a, b) => (b.updated_at || '').localeCompare(a.updated_at || '')) : [];
  const localPrios = workspacePriorities(ws?.id);
  const groups = localPrios.map((p, i) => ({ p, color: prioColor(p, i), tasks: wsTasks.filter((t) => t.priority_id === p.id) }));
  const unsorted = wsTasks.filter((t) => !localPrios.some((p) => p.id === t.priority_id));
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
    <div class="win-content">
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
  </div>
  <dialog id="task-dialog">
    <form method="dialog" id="task-form" class="dlg-card">
      <h2>新任务</h2>
      <label>标题<input name="title" required maxlength="200" placeholder="要做点什么？" autofocus /></label>
      <div class="dlg-grid">
        <label>预计小时<input name="estimated" type="number" min="0" step="0.5" placeholder="2" /></label>
        <label>完成日期<input name="dueDate" type="date" /></label>
      </div>
      <label>备注<textarea name="notes" rows="2" placeholder="可选"></textarea></label>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>取消</button><button class="ok" id="task-ok" type="button">贴上</button></div>
    </form>
  </dialog>
  <dialog id="block-dialog">
    <form method="dialog" id="block-form" class="dlg-card">
      <h2>标记阻塞</h2>
      <p class="block-hint" id="block-task-title"></p>
      <label>原因<textarea name="reason" rows="3" required maxlength="300" placeholder="例如：等待接口文档"></textarea></label>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>取消</button><button class="ok" id="block-ok" type="button">确认阻塞</button></div>
    </form>
  </dialog>
  <dialog id="unblock-dialog">
    <form method="dialog" id="unblock-form" class="dlg-card">
      <h2>解除阻塞</h2>
      <p class="block-hint" id="unblock-task-title"></p>
      <label>解除原因<textarea name="resolutionReason" rows="3" maxlength="300" placeholder="可选，例如：依赖已就绪"></textarea></label>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>取消</button><button class="ok" id="unblock-ok" type="button">解除阻塞</button></div>
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
      <label class="set-slider"><span class="set-slider-head"><span>鼠标离开时不透明度</span><output id="opacity-val">${opacity}%</output></span>
        <input type="range" id="opacity-range" min="0" max="100" step="5" value="${opacity}" />
      </label>
      <label class="set-slider"><span class="set-slider-head"><span>界面字体大小</span><output id="font-size-val">${formatFontSizeDelta(fontSizeDelta)}</output></span>
        <input type="range" id="font-size-range" min="${FONT_SIZE_MIN}" max="${FONT_SIZE_MAX}" step="0.5" value="${fontSizeDelta}" />
      </label>
      <div class="set-web">
        <span>Web 设置</span>
        <button id="btn-web" type="button">前往</button>
      </div>
      <div class="dlg-actions"><button class="ghost" type="button" data-close>关闭</button></div>
    </form>
  </dialog>
  <div id="toast" class="toast" role="status"></div>`;

  unfocusedOpacity = opacity;
  applyContentOpacity();
  document.querySelector('.win')?.addEventListener('mouseenter', () => {
    mouseInside = true;
    if (unfocusedOpacity !== 0) applyContentOpacity();
  });
  document.querySelector('.win')?.addEventListener('mouseleave', () => {
    mouseInside = false;
    mouseInTitleBar = false;
    zeroOpacityExpanded = false;
    applyContentOpacity();
  });
  document.querySelector('.win-bar')?.addEventListener('mouseenter', () => {
    mouseInside = true;
    mouseInTitleBar = true;
    if (unfocusedOpacity === 0) zeroOpacityExpanded = true;
    applyContentOpacity();
  });
  document.querySelector('.win-bar')?.addEventListener('mouseleave', () => {
    mouseInTitleBar = false;
  });
  document.querySelector('#btn-new')?.addEventListener('click', () => openTaskDialog(null));
  document.querySelector('#btn-settings')?.addEventListener('click', () => document.querySelector('#settings-dialog').showModal());
  document.querySelector('#opacity-range')?.addEventListener('input', (e) => {
    const v = normalizeOpacity(e.target.value);
    localStorage.setItem(OPACITY_KEY, String(v));
    document.querySelector('#opacity-val').textContent = `${v}%`;
    unfocusedOpacity = v;
    zeroOpacityExpanded = v === 0 && mouseInTitleBar;
    applyContentOpacity();
  });
  document.querySelector('#font-size-range')?.addEventListener('input', (e) => {
    fontSizeDelta = normalizeFontSizeDelta(e.target.value);
    localStorage.setItem(FONT_SIZE_KEY, String(fontSizeDelta));
    document.querySelector('#font-size-val').textContent = formatFontSizeDelta(fontSizeDelta);
    applyFontSize();
  });
  document.querySelector('#btn-web')?.addEventListener('click', async () => {
    try {
      await call('open_web_console');
      notify('已打开 Web 设置');
    } catch (error) { notify(errorMessage(error, '无法打开 Web 设置')); }
  });
  document.querySelector('#btn-pin')?.addEventListener('click', togglePin);
  document.querySelector('#btn-min')?.addEventListener('click', () => theWindow()?.hide());
  document.querySelector('#btn-close')?.addEventListener('click', () => theWindow()?.hide());
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
    if (!state.workspaces.some((workspace) => workspace.id === created.id)) {
      throw new Error('工作区已创建，但刷新列表失败');
    }
    state.activeWs = created.id;
    await loadTasks();
    render();
    requestAnimationFrame(() => {
      document.querySelector(`.ws-tab[data-ws="${CSS.escape(created.id)}"]`)?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
    });
    notify(`已创建工作区「${trimmed}」`);
  } catch (error) { notify(errorMessage(error, '创建失败')); }
}
async function removeWorkspace(id, expectedVersion) {
  const ws = state.workspaces.find((w) => w.id === id);
  const taskCount = state.tasks.filter((task) => task.workspace_id === id).length;
  if (taskCount > 0) {
    notify(`工作区还有 ${taskCount} 个任务，请先移走或删除`);
    return;
  }
  const ok = await openConfirm(`删除空工作区「${ws ? ws.name : ''}」？`);
  if (!ok) return;
  try {
    await call('delete_workspace', { id, expectedVersion });
    await loadMeta();
    if (state.activeWs === id) state.activeWs = state.workspaces[0]?.id || null;
    render();
    notify('工作区已删除');
  } catch (error) { notify(errorMessage(error, '删除失败')); }
}
async function addPriority() {
  if (!state.activeWs || state.activeWs === 'done') return;
  const trimmed = await openPrompt('添加分级', '名称（如：P3 / 紧急）');
  if (!trimmed) return;
  try {
    const localPrios = workspacePriorities();
    await call('create_priority', { workspaceId: state.activeWs, name: trimmed, color: PRIO_PALETTE[localPrios.length % PRIO_PALETTE.length] });
    await loadMeta();
    render();
    notify(`已添加分级「${trimmed}」`);
  } catch (error) { notify(errorMessage(error, '添加失败')); }
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
      await call('delete_priority', { id: prioId, expectedVersion: prio.version });
      await loadMeta(); render(); notify('分级已删除');
    }
  } catch (error) { notify(errorMessage(error, '操作失败')); }
}

function openTaskDialog(prioId) {
  state.newTaskTarget = { ws: state.activeWs, prio: prioId || workspacePriorities()[0]?.id || null };
  const form = document.querySelector('#task-form');
  form.reset();
  document.querySelector('#task-dialog').showModal();
}

async function submitTask() {
  const form = document.querySelector('#task-form');
  const title = form.querySelector('[name="title"]').value.trim();
  if (!title) { notify('标题不能为空'); return; }
  const target = state.newTaskTarget || { ws: state.activeWs, prio: workspacePriorities()[0]?.id || null };
  try {
    await call('create_task', {
      input: {
        title,
        notes: form.querySelector('[name="notes"]').value.trim() || null,
        estimatedActiveMinutes: parseEstimatedMinutes(form.querySelector('[name="estimated"]').value),
        dueDate: form.querySelector('[name="dueDate"]').value || null,
        workspaceId: target.ws,
        priorityId: target.prio,
      },
    });
    document.querySelector('#task-dialog').close();
    await loadTasks();
    notify('任务已添加');
  } catch (error) { notify(errorMessage(error, '创建任务失败')); }
}

async function togglePin() {
  const w = theWindow();
  if (!w) { pinned = !pinned; render(); notify(pinned ? '（预览）模拟置顶' : '（预览）取消置顶'); return; }
  try {
    pinned = !pinned;
    await w.setAlwaysOnTop(pinned);
    render();
    notify(pinned ? '已置顶' : '已取消置顶');
  } catch (error) { pinned = !pinned; notify(errorMessage(error, '置顶失败')); }
}


async function handleAction(button) {
  const action = button.dataset.action;
  const id = button.dataset.id;
  const version = Number(button.dataset.version);
  try {
    if (action === 'complete') {
      const task = state.tasks.find((item) => item.id === id);
      if (task?.is_blocked) {
        notify('请先解除阻塞，再完成任务');
        return;
      }
      await call('complete_task', { id, expectedVersion: version });
      notify('任务完成 ✦');
    }
    if (action === 'reopen') { await call('reopen_task', { id, expectedVersion: version }); notify('任务已重新打开'); }
    if (action === 'work') { await call('start_work', { taskId: id }); notify('开始计时 ▶'); }
    if (action === 'pause') {
      await call('pause_task', { id, expectedVersion: version });
      notify('已暂停，回到待办');
    }
    if (action === 'block') { openBlockDialog(id); return; }
    if (action === 'unblock') { openUnblockDialog(button.dataset.taskId || id); return; }
    await loadTasks();
  } catch (error) { notify(errorMessage(error, '操作失败')); }
}

function openBlockDialog(taskId) {
  state.blockingTaskId = taskId;
  const task = state.tasks.find((item) => item.id === taskId);
  const form = document.querySelector('#block-form');
  form.querySelector('[name="reason"]').value = '';
  document.querySelector('#block-task-title').textContent = task ? task.title : '';
  document.querySelector('#block-dialog').showModal();
}

async function submitBlock() {
  const form = document.querySelector('#block-form');
  const reason = form.querySelector('[name="reason"]').value.trim();
  if (!reason) { notify('阻塞原因不能为空'); return; }
  if (!state.blockingTaskId) return;
  try {
    await call('block_task', { taskId: state.blockingTaskId, reason, note: null });
    document.querySelector('#block-dialog').close();
    state.blockingTaskId = null;
    await loadTasks();
    notify('已标记阻塞');
  } catch (error) { notify(errorMessage(error, '标记阻塞失败')); }
}

function openUnblockDialog(taskId) {
  const task = state.tasks.find((item) => item.id === taskId);
  const block = state.blocksByTask[taskId];
  if (!task || !block) { notify('未找到活动阻塞记录'); return; }
  state.unblockingTaskId = taskId;
  const form = document.querySelector('#unblock-form');
  form.querySelector('[name="resolutionReason"]').value = '';
  document.querySelector('#unblock-task-title').textContent = task.title;
  document.querySelector('#unblock-dialog').showModal();
}

async function submitUnblock() {
  const taskId = state.unblockingTaskId;
  const block = state.blocksByTask[taskId];
  if (!taskId || !block) return;
  const form = document.querySelector('#unblock-form');
  const reason = form.querySelector('[name="resolutionReason"]').value.trim() || null;
  try {
    await call('unblock_task', {
      blockId: block.id,
      expectedVersion: block.version,
      resolutionReason: reason,
    });
    document.querySelector('#unblock-dialog').close();
    state.unblockingTaskId = null;
    await loadTasks();
    notify('已解除阻塞，回到待办');
  } catch (error) { notify(errorMessage(error, '解除阻塞失败')); }
}

async function call(command, args = {}) { return isTauri() ? invoke(command, args) : previewCommand(command, args); }
async function previewCommand(command, args) {
  if (!state.workspaces.length) seedPreviewMeta();
  if (command === 'list_tasks') return state.tasks;
  if (command === 'list_workspaces') return state.workspaces;
  if (command === 'list_priorities') return state.prios;
  if (command === 'create_workspace') {
    const ws = { id: crypto.randomUUID(), name: args.name, sort_order: state.workspaces.length, builtin: false, version: 1 };
    state.workspaces.push(ws);
    [['P0', '#b0432f'], ['P1', '#b16d42'], ['P2', '#8f9a90']].forEach(([name, color], sortOrder) => {
      state.prios.push({ id: crypto.randomUUID(), workspace_id: ws.id, name, color, sort_order: sortOrder, version: 1 });
    });
    return ws;
  }
  if (command === 'rename_workspace') { const w = state.workspaces.find((x) => x.id === args.id); if (w) { w.name = args.name; w.version += 1; } return w; }
  if (command === 'delete_workspace') {
    if (state.tasks.some((t) => t.workspace_id === args.id)) throw new Error('工作区还有任务，先移走再删除');
    state.workspaces = state.workspaces.filter((w) => w.id !== args.id);
    state.prios = state.prios.filter((p) => p.workspace_id !== args.id);
  }
  if (command === 'create_priority') {
    const localPrios = state.prios.filter((priority) => priority.workspace_id === args.workspaceId);
    const p = { id: crypto.randomUUID(), workspace_id: args.workspaceId, name: args.name, color: args.color || null, sort_order: localPrios.length, version: 1 };
    state.prios.push(p);
    return p;
  }
  if (command === 'update_priority') { const p = state.prios.find((x) => x.id === args.id); if (p) { p.name = args.name; p.color = args.color ?? p.color; p.version += 1; } return p; }
  if (command === 'delete_priority') {
    if (state.tasks.some((t) => t.priority_id === args.id)) throw new Error('该分级还有任务，先移走再删除');
    const priority = state.prios.find((p) => p.id === args.id);
    if (state.prios.filter((p) => p.workspace_id === priority?.workspace_id).length <= 1) throw new Error('每个工作区至少要保留一个分级');
    state.prios = state.prios.filter((p) => p.id !== args.id);
  }
  if (command === 'create_task') {
    const task = { id: crypto.randomUUID(), title: args.input.title, notes: args.input.notes, estimated_active_minutes: args.input.estimatedActiveMinutes, due_date: args.input.dueDate || null, status: 'pending', sessions: [], sort_order: 0, updated_at: new Date().toISOString(), version: 1, is_blocked: false, workspace_id: args.input.workspaceId, priority_id: args.input.priorityId, home_workspace_id: args.input.workspaceId };
    state.tasks.unshift(task);
    return task;
  }
  if (command === 'complete_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { if (task.activeSession) task.activeSession.ended_at = new Date().toISOString(); task.status = 'completed'; task.completed_at = new Date().toISOString(); task.version += 1; task.updated_at = task.completed_at; task.workspace_id = 'done'; delete task.activeSession; } }
  if (command === 'reopen_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { task.status = 'pending'; task.completed_at = null; task.version += 1; task.workspace_id = task.home_workspace_id || 'daily'; } }
  if (command === 'pause_task') { const task = state.tasks.find((item) => item.id === args.id); if (task) { if (task.activeSession) task.activeSession.ended_at = new Date().toISOString(); task.status = 'pending'; task.version += 1; task.updated_at = new Date().toISOString(); delete task.activeSession; } }
  if (command === 'finish_work') { for (const task of state.tasks) { const session = (task.sessions || []).find((item) => item.id === args.sessionId); if (session) session.ended_at = new Date().toISOString(); if (task.activeSession?.id === args.sessionId) delete task.activeSession; } }
  if (command === 'list_sessions') { const task = state.tasks.find((item) => item.id === args.taskId); return task ? [...(task.sessions || [])] : []; }
  if (command === 'delete_task') state.tasks = state.tasks.filter((item) => item.id !== args.id);
  if (command === 'start_work') { const task = state.tasks.find((item) => item.id === args.taskId); if (task) { task.status = 'in_progress'; task.version += 1; task.sessions = task.sessions || []; task.activeSession = { id: crypto.randomUUID(), task_id: task.id, started_at: new Date().toISOString(), ended_at: null }; task.sessions.push(task.activeSession); } }
  if (command === 'open_web_console') throw new Error('Web 设置仅桌面端可用');
  if (command === 'list_blocks') { const task = state.tasks.find((item) => item.id === args.taskId); return task?.activeBlock ? [task.activeBlock] : []; }
  if (command === 'block_task') { const task = state.tasks.find((item) => item.id === args.taskId); if (task) { if (task.activeSession) task.activeSession.ended_at = new Date().toISOString(); task.is_blocked = true; task.activeBlock = { id: crypto.randomUUID(), task_id: task.id, started_at: new Date().toISOString(), ended_at: null, reason: args.reason, note: args.note ?? null, created_at: new Date().toISOString(), updated_at: new Date().toISOString(), version: 1, deleted_at: null }; } }
  if (command === 'unblock_task') { for (const task of state.tasks) { if (task.activeBlock?.id === args.blockId) { task.is_blocked = false; delete task.activeBlock; task.status = 'pending'; task.version += 1; } } }
}
function seedPreviewMeta() {
  state.workspaces = [
    { id: 'daily', name: '日常', sort_order: 0, builtin: true, version: 1 },
    { id: 'work', name: '工作', sort_order: 1, builtin: true, version: 1 },
    { id: 'done', name: '已完成', sort_order: 99, builtin: true, version: 1 },
  ];
  state.prios = [
    { id: 'P0', workspace_id: 'daily', name: 'P0', color: '#b0432f', sort_order: 0, version: 1 },
    { id: 'P1', workspace_id: 'daily', name: 'P1', color: '#b16d42', sort_order: 1, version: 1 },
    { id: 'P2', workspace_id: 'daily', name: 'P2', color: '#8f9a90', sort_order: 2, version: 1 },
    { id: 'work-P0', workspace_id: 'work', name: 'P0', color: '#b0432f', sort_order: 0, version: 1 },
    { id: 'work-P1', workspace_id: 'work', name: 'P1', color: '#b16d42', sort_order: 1, version: 1 },
    { id: 'work-P2', workspace_id: 'work', name: 'P2', color: '#8f9a90', sort_order: 2, version: 1 },
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
  state.finishedSessionMinutesByTask = {};
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
    const finishedMinutes = Math.floor(sessions
      .filter((session) => session.ended_at)
      .reduce((acc, session) => acc + sessionDurationMs(session), 0) / 60000);
    state.finishedSessionMinutesByTask[task.id] = finishedMinutes;
    state.sessionMinutesByTask[task.id] = finishedMinutes + activeMinutes(task, active);
  }));
  await Promise.all(state.tasks.filter((task) => task.status === 'completed').map(async (task) => {
    const sessions = await call('list_sessions', { taskId: task.id });
    state.sessionMinutesByTask[task.id] = totalSessionMinutes(task, sessions);
  }));
  render();
}


// 每分钟只刷新计时文本，不整页重绘，避免影响打开中的弹窗。
setInterval(() => {
  document.querySelectorAll('[data-active-timer]').forEach((element) => {
    const task = state.tasks.find((item) => item.id === element.dataset.activeTimer);
    if (!task) return;
    const finished = state.finishedSessionMinutesByTask[task.id] || 0;
    const current = activeMinutes(task, state.sessionsByTask[task.id]);
    element.querySelector('span').textContent = fmtDuration(finished + current);
  });
}, 60000);

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
  if (e.target.closest('#unblock-ok')) { submitUnblock(); return; }
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
      } catch (error) { notify(errorMessage(error, '重命名失败')); }
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
    const taskCount = state.tasks.filter((task) => task.priority_id === prioId).length;
    if (taskCount > 0) {
      notify(`该分级仍被 ${taskCount} 个任务使用（含已完成）`);
      return;
    }
    const ok = await openConfirm(`删除空分级「${prio.name}」？`);
    if (!ok) return;
    try {
      await call('delete_priority', { id: prioId, expectedVersion: prio.version });
      await loadMeta();
      render();
      notify('分级已删除');
    } catch (error) { notify(errorMessage(error, '删除失败')); }
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
  } catch (error) { notify(errorMessage(error, '删除失败')); }
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

// ===== 全局鼠标位置驱动的便签透明度 =====
async function updateMouseInside() {
  const w = theWindow();
  if (!w) return;
  try {
    const [cursor, position, size, scale] = await Promise.all([
      cursorPosition(),
      w.outerPosition(),
      w.outerSize(),
      w.scaleFactor(),
    ]);
    const inside = cursor.x >= position.x
      && cursor.x < position.x + size.width
      && cursor.y >= position.y
      && cursor.y < position.y + size.height;
    const titleBar = document.querySelector('.win-bar')?.getBoundingClientRect();
    const inTitleBar = Boolean(inside && titleBar
      && cursor.x >= position.x + titleBar.left * scale
      && cursor.x < position.x + titleBar.right * scale
      && cursor.y >= position.y + titleBar.top * scale
      && cursor.y < position.y + titleBar.bottom * scale);
    if (unfocusedOpacity === 0) {
      if (!inside) zeroOpacityExpanded = false;
      else if (inTitleBar) zeroOpacityExpanded = true;
    } else {
      zeroOpacityExpanded = false;
    }
    if (inside !== mouseInside || inTitleBar !== mouseInTitleBar) {
      mouseInside = inside;
      mouseInTitleBar = inTitleBar;
      applyContentOpacity();
    }
  } catch {}
}

setInterval(updateMouseInside, 100);
updateMouseInside();

// ===== 自绘拖拽（跨平台，避免 macOS 边缘半屏吸附） =====
let dragState = null;
document.addEventListener('mousedown', (e) => {
  if (e.button !== 0) return;
  const bar = e.target.closest('.win-bar');
  if (!bar || e.target.closest('button')) return;
  const w = theWindow();
  if (!w) return;
  e.preventDefault();
  Promise.all([w.outerPosition(), w.scaleFactor()]).then(([pos, scale]) => {
    const logical = pos.toLogical(scale);
    dragState = { sx: e.screenX, sy: e.screenY, px: logical.x, py: logical.y };
  }).catch(() => {});
});
document.addEventListener('mousemove', (e) => {
  if (!dragState) return;
  const w = theWindow();
  if (!w) return;
  w.setPosition(new LogicalPosition(
    dragState.px + (e.screenX - dragState.sx),
    dragState.py + (e.screenY - dragState.sy),
  )).catch(() => {});
});
document.addEventListener('mouseup', () => {
  if (!dragState) return;
  dragState = null;
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
  if (w) {
    try { pinned = await w.isAlwaysOnTop(); } catch {}
  }
  await loadMeta();
  await loadTasks();
})();
