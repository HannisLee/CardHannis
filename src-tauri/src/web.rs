use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::State,
    response::Html,
    routing::{get, post},
};
use cardhannis_core::{TaskService, TaskStatus};
use chrono::{DateTime, Utc};
use serde_json::json;
use tokio::sync::oneshot;

const WEB_PORT: u16 = 1421;
const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub struct WebConsoleState {
    service: Arc<TaskService>,
    database_path: PathBuf,
    port: AtomicU16,
    running: AtomicBool,
    last_activity: AtomicU64,
    shutdown: std::sync::Mutex<Option<oneshot::Sender<()>>>,
}

impl WebConsoleState {
    pub fn new(service: Arc<TaskService>, database_path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            service,
            database_path,
            port: AtomicU16::new(WEB_PORT),
            running: AtomicBool::new(false),
            last_activity: AtomicU64::new(0),
            shutdown: std::sync::Mutex::new(None),
        })
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port.load(Ordering::SeqCst))
    }

    fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.last_activity.store(now, Ordering::SeqCst);
    }

    fn idle_for(&self) -> Duration {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let last = self.last_activity.load(Ordering::SeqCst);
        Duration::from_millis(now.saturating_sub(last))
    }

    fn stop(&self) {
        if let Some(sender) = self.shutdown.lock().unwrap().take() {
            let _ = sender.send(());
        }
        self.running.store(false, Ordering::SeqCst);
    }

    pub async fn start(self: &Arc<Self>) -> Result<(), String> {
        if self.is_running() {
            self.touch();
            return Ok(());
        }

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", WEB_PORT))
            .await
            .map_err(|error| format!("无法启动 Web 控制台: {error}"))?;

        let (sender, receiver) = oneshot::channel();
        *self.shutdown.lock().unwrap() = Some(sender);
        self.running.store(true, Ordering::SeqCst);
        self.touch();

        let state = self.clone();
        let router = Router::new()
            .route("/", get(index))
            .route("/api/status", get(status))
            .route("/api/touch", post(touch))
            .route("/api/shutdown", post(shutdown))
            .with_state(state);

        let server = axum::serve(listener, router).with_graceful_shutdown(async move {
            let _ = receiver.await;
        });

        let server_state = self.clone();
        tauri::async_runtime::spawn(async move {
            let _ = server.await;
            server_state.running.store(false, Ordering::SeqCst);
        });

        let watcher_state = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                if !watcher_state.is_running() {
                    break;
                }
                if watcher_state.idle_for() >= IDLE_TIMEOUT {
                    watcher_state.stop();
                    break;
                }
            }
        });

        Ok(())
    }
}

async fn index(State(state): State<Arc<WebConsoleState>>) -> Html<&'static str> {
    state.touch();
    Html(WEB_PAGE)
}

async fn status(State(state): State<Arc<WebConsoleState>>) -> Json<serde_json::Value> {
    state.touch();

    let tasks = state
        .service
        .list(false)
        .map_err(|error| error.to_string())
        .unwrap_or_default();
    let completed_count = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::Completed)
        .count();
    let in_progress_count = tasks
        .iter()
        .filter(|task| task.status == TaskStatus::InProgress)
        .count();

    let mut active_minutes = 0i64;
    for task in &tasks {
        let sessions = state
            .service
            .sessions(&task.id)
            .map_err(|error| error.to_string())
            .unwrap_or_default();
        for session in sessions {
            let started_at = DateTime::parse_from_rfc3339(&session.started_at)
                .map(|time| time.timestamp())
                .unwrap_or(0);
            let ended_at = session
                .ended_at
                .as_ref()
                .map(|value| {
                    DateTime::parse_from_rfc3339(value)
                        .map(|time| time.timestamp())
                        .unwrap_or(0)
                })
                .unwrap_or_else(|| Utc::now().timestamp());
            active_minutes += ((ended_at - started_at) / 60).max(0);
        }
    }

    Json(json!({
        "taskCount": tasks.len(),
        "completedCount": completed_count,
        "inProgressCount": in_progress_count,
        "activeMinutes": active_minutes,
        "databasePath": state.database_path.display().to_string(),
        "serverUrl": state.url(),
        "autoCloseMinutes": IDLE_TIMEOUT.as_secs() / 60,
        "lastActivity": state.last_activity.load(Ordering::SeqCst),
    }))
}

async fn touch(State(state): State<Arc<WebConsoleState>>) -> Json<serde_json::Value> {
    state.touch();
    Json(json!({ "lastActivity": state.last_activity.load(Ordering::SeqCst) }))
}

async fn shutdown(State(state): State<Arc<WebConsoleState>>) -> Json<serde_json::Value> {
    state.touch();
    let state_for_shutdown = state.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        state_for_shutdown.stop();
    });
    Json(json!({ "ok": true }))
}

const WEB_PAGE: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>CardHannis Web 设置</title>
<style>
:root { color-scheme: light; font-family: -apple-system, BlinkMacSystemFont, "SF Pro Text", "PingFang SC", sans-serif; }
* { box-sizing: border-box; }
body { margin: 0; min-height: 100vh; background: #f5f2e9; color: #33403a; }
header { padding: 28px 32px 20px; background: #325549; color: #f2f6ef; }
header h1 { margin: 0 0 6px; font-size: 22px; }
header p { margin: 0; opacity: .78; font-size: 13px; }
main { max-width: 760px; margin: 0 auto; padding: 26px 24px 48px; display: grid; gap: 16px; }
.card { border-radius: 14px; background: #fffdf6; box-shadow: 0 10px 28px rgba(45,52,44,.1), inset 0 0 0 1px rgba(122,112,82,.15); padding: 18px 20px; }
.card h2 { margin: 0 0 14px; font-size: 15px; color: #325549; }
dl { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 20px; margin: 0; }
dt { font-size: 11px; color: #8b8f80; margin-bottom: 3px; }
dd { margin: 0; font-size: 14px; font-weight: 650; }
.actions { display: flex; justify-content: flex-end; gap: 10px; }
button { border: 0; border-radius: 8px; padding: 9px 14px; font-size: 13px; font-weight: 650; cursor: pointer; }
.primary { background: #31574a; color: #fff; }
.ghost { background: #eceadf; color: #6b6f63; }
.note { font-size: 12px; color: #8b8f80; margin-top: 12px; }
@media (max-width: 620px) { dl { grid-template-columns: 1fr; } }
</style>
</head>
<body>
<header>
  <h1>CardHannis Web 设置</h1>
  <p>个人设置控制台 · 默认关闭 · 5 分钟无操作自动关闭</p>
</header>
<main>
  <section class="card">
    <h2>数据概览</h2>
    <dl>
      <div><dt>任务总数</dt><dd id="task-count">—</dd></div>
      <div><dt>已完成</dt><dd id="completed-count">—</dd></div>
      <div><dt>进行中</dt><dd id="in-progress-count">—</dd></div>
      <div><dt>累计活动时长</dt><dd id="active-minutes">—</dd></div>
    </dl>
  </section>
  <section class="card">
    <h2>服务状态</h2>
    <dl>
      <div><dt>服务地址</dt><dd id="server-url">—</dd></div>
      <div><dt>数据库路径</dt><dd id="database-path">—</dd></div>
      <div><dt>自动关闭</dt><dd id="auto-close">—</dd></div>
      <div><dt>剩余时间</dt><dd id="remaining">—</dd></div>
    </dl>
    <p class="note">这里只做本机设置和数据查看，不提供任务基础操作。</p>
  </section>
  <section class="card">
    <h2>同步设置</h2>
    <p class="note">后续版本将在这里提供数据库同步、备份和导出配置。</p>
  </section>
  <section class="card actions">
    <button class="ghost" id="refresh">刷新</button>
    <button class="primary" id="shutdown">关闭 Web 控制台</button>
  </section>
</main>
<script>
const $ = (id) => document.getElementById(id);
let autoCloseAt = 0;

function formatDuration(minutes) {
  if (minutes < 60) return `${minutes}m`;
  const hours = minutes / 60;
  return `${Number.isInteger(hours) ? hours : hours.toFixed(1)}h`;
}
let touchTimer = null;

function render(data) {
  $('task-count').textContent = data.taskCount;
  $('completed-count').textContent = data.completedCount;
  $('in-progress-count').textContent = data.inProgressCount;
  $('active-minutes').textContent = formatDuration(data.activeMinutes);
  $('server-url').textContent = data.serverUrl;
  $('database-path').textContent = data.databasePath;
  $('auto-close').textContent = `${data.autoCloseMinutes} 分钟无操作`;
  autoCloseAt = data.lastActivity + data.autoCloseMinutes * 60 * 1000;
}

async function touch() {
  if (touchTimer) return;
  touchTimer = setTimeout(() => (touchTimer = null), 1000);
  const response = await fetch('/api/touch', { method: 'POST' });
  const data = await response.json();
  autoCloseAt = data.lastActivity + 5 * 60 * 1000;
}

async function refresh() {
  const response = await fetch('/api/status');
  const data = await response.json();
  render(data);
}

setInterval(() => {
  if (!autoCloseAt) return;
  const remaining = Math.max(0, autoCloseAt - Date.now());
  if (!remaining) {
    $('remaining').textContent = '已自动关闭';
    return;
  }
  const minutes = Math.floor(remaining / 60000);
  const seconds = Math.floor((remaining % 60000) / 1000);
  $('remaining').textContent = `${minutes}:${String(seconds).padStart(2, '0')}`;
}, 1000);

document.addEventListener('click', touch);
document.addEventListener('keydown', touch);
$('refresh').addEventListener('click', refresh);
$('shutdown').addEventListener('click', async () => {
  await fetch('/api/shutdown', { method: 'POST' });
  document.body.innerHTML = '<header><h1>Web 控制台已关闭</h1><p>可以在桌面端重新打开。</p></header>';
});

refresh();
</script>
</body>
</html>"##;
