use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::Html,
    routing::{get, post},
};
use cardhannis_core::{SyncSnapshot, TaskService, TaskStatus};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::oneshot;

const WEB_PORT: u16 = 1421;
const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const DEFAULT_SUPABASE_SCHEMA: &str = "public";
const DEFAULT_WORKSPACES_TABLE: &str = "workspaces";
const DEFAULT_PRIORITIES_TABLE: &str = "priorities";
const DEFAULT_TASKS_TABLE: &str = "tasks";
const DEFAULT_TASK_BLOCKS_TABLE: &str = "task_blocks";
const DEFAULT_WORK_SESSIONS_TABLE: &str = "work_sessions";
const DEFAULT_SYNC_INTERVAL_MINUTES: u64 = 5;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SupabaseTableSettings {
    #[serde(default = "default_workspaces_table")]
    workspaces: String,
    #[serde(default = "default_priorities_table")]
    priorities: String,
    #[serde(default = "default_tasks_table")]
    tasks: String,
    #[serde(default = "default_task_blocks_table")]
    task_blocks: String,
    #[serde(default = "default_work_sessions_table")]
    work_sessions: String,
}

impl Default for SupabaseTableSettings {
    fn default() -> Self {
        Self {
            workspaces: DEFAULT_WORKSPACES_TABLE.to_owned(),
            priorities: DEFAULT_PRIORITIES_TABLE.to_owned(),
            tasks: DEFAULT_TASKS_TABLE.to_owned(),
            task_blocks: DEFAULT_TASK_BLOCKS_TABLE.to_owned(),
            work_sessions: DEFAULT_WORK_SESSIONS_TABLE.to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SupabaseSettings {
    #[serde(default)]
    url: String,
    #[serde(default)]
    key: String,
    #[serde(default = "default_supabase_schema")]
    schema: String,
    #[serde(default)]
    auth_email: String,
    #[serde(default)]
    auth_password: String,
    #[serde(default = "default_auto_sync")]
    auto_sync: bool,
    #[serde(default = "default_sync_interval_minutes")]
    sync_interval_minutes: u64,
    #[serde(default)]
    tables: SupabaseTableSettings,
}

impl Default for SupabaseSettings {
    fn default() -> Self {
        Self {
            url: String::new(),
            key: String::new(),
            schema: DEFAULT_SUPABASE_SCHEMA.to_owned(),
            auth_email: String::new(),
            auth_password: String::new(),
            auto_sync: true,
            sync_interval_minutes: DEFAULT_SYNC_INTERVAL_MINUTES,
            tables: SupabaseTableSettings::default(),
        }
    }
}

fn default_auto_sync() -> bool {
    true
}

fn default_sync_interval_minutes() -> u64 {
    DEFAULT_SYNC_INTERVAL_MINUTES
}

fn default_supabase_schema() -> String {
    DEFAULT_SUPABASE_SCHEMA.to_owned()
}

fn default_workspaces_table() -> String {
    DEFAULT_WORKSPACES_TABLE.to_owned()
}

fn default_priorities_table() -> String {
    DEFAULT_PRIORITIES_TABLE.to_owned()
}

fn default_tasks_table() -> String {
    DEFAULT_TASKS_TABLE.to_owned()
}

fn default_task_blocks_table() -> String {
    DEFAULT_TASK_BLOCKS_TABLE.to_owned()
}

fn default_work_sessions_table() -> String {
    DEFAULT_WORK_SESSIONS_TABLE.to_owned()
}

fn load_supabase_settings(path: &Path) -> SupabaseSettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn normalize_supabase_settings(mut settings: SupabaseSettings) -> SupabaseSettings {
    settings.url = settings.url.trim().trim_end_matches('/').to_owned();
    settings.key = settings.key.trim().to_owned();
    settings.schema = settings.schema.trim().to_owned();
    settings.auth_email = settings.auth_email.trim().to_owned();
    settings.auth_password = settings.auth_password.trim().to_owned();
    settings.tables.workspaces = settings.tables.workspaces.trim().to_owned();
    settings.tables.priorities = settings.tables.priorities.trim().to_owned();
    settings.tables.tasks = settings.tables.tasks.trim().to_owned();
    settings.tables.task_blocks = settings.tables.task_blocks.trim().to_owned();
    settings.tables.work_sessions = settings.tables.work_sessions.trim().to_owned();
    settings
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn validate_supabase_settings(settings: &SupabaseSettings) -> Result<(), String> {
    if settings.url.is_empty() || settings.key.is_empty() {
        return Err("请填写 Supabase Project URL 和 publishable/anon key".to_owned());
    }
    if !settings.url.starts_with("https://") {
        return Err("Supabase Project URL 必须使用 HTTPS".to_owned());
    }
    let host = settings.url.strip_prefix("https://").unwrap_or_default();
    if host.is_empty() || host.contains(['/', '?', '#', ' ']) {
        return Err("Supabase Project URL 格式不正确".to_owned());
    }
    if settings.key.starts_with("sb_secret_") {
        return Err("不要填写 secret/service_role key，请使用 publishable/anon key".to_owned());
    }
    if settings.auth_email.is_empty() || settings.auth_password.is_empty() {
        return Err("请填写 Supabase Auth 邮箱和密码".to_owned());
    }
    if settings.sync_interval_minutes == 0 || settings.sync_interval_minutes > 1440 {
        return Err("自动同步间隔必须在 1 到 1440 分钟之间".to_owned());
    }
    if !valid_identifier(&settings.schema) {
        return Err("Supabase schema 名称不能为空，只能包含字母、数字或下划线".to_owned());
    }
    for (label, table) in [
        ("workspaces", &settings.tables.workspaces),
        ("priorities", &settings.tables.priorities),
        ("tasks", &settings.tables.tasks),
        ("task_blocks", &settings.tables.task_blocks),
        ("work_sessions", &settings.tables.work_sessions),
    ] {
        if !valid_identifier(table) {
            return Err(format!("{label} 表名不能为空，只能包含字母、数字或下划线"));
        }
    }
    Ok(())
}

fn save_supabase_settings(path: &Path, settings: &SupabaseSettings) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建设置目录失败: {error}"))?;
    }
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = PathBuf::from(temporary);
    let content = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("序列化 Supabase 设置失败: {error}"))?;
    fs::write(&temporary, content).map_err(|error| format!("保存 Supabase 设置失败: {error}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("设置 Supabase 配置文件权限失败: {error}"))?;
    }
    fs::rename(&temporary, path).map_err(|error| format!("保存 Supabase 设置失败: {error}"))
}

#[derive(Clone, Debug, Serialize)]
struct SyncRuntimeStatus {
    connected: bool,
    syncing: bool,
    auto_sync: bool,
    interval_minutes: u64,
    last_synced_at: Option<String>,
    next_sync_at: Option<u64>,
    last_error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SyncOutcome {
    ok: bool,
    message: String,
    local_before: cardhannis_core::SyncCounts,
    remote_before: cardhannis_core::SyncCounts,
    merged: cardhannis_core::SyncCounts,
    synced_at: String,
}

pub struct WebConsoleState {
    service: Arc<TaskService>,
    database_path: PathBuf,
    settings_path: PathBuf,
    settings: Mutex<SupabaseSettings>,
    port: AtomicU16,
    running: AtomicBool,
    sync_running: AtomicBool,
    last_activity: AtomicU64,
    sync_status: Mutex<SyncRuntimeStatus>,
    shutdown: std::sync::Mutex<Option<oneshot::Sender<()>>>,
}

impl WebConsoleState {
    pub fn new(service: Arc<TaskService>, database_path: PathBuf) -> Arc<Self> {
        let settings_path = database_path.with_file_name("supabase.json");
        let settings = load_supabase_settings(&settings_path);
        let sync_status = SyncRuntimeStatus {
            connected: false,
            syncing: false,
            auto_sync: settings.auto_sync,
            interval_minutes: settings.sync_interval_minutes,
            last_synced_at: None,
            next_sync_at: None,
            last_error: None,
        };
        Arc::new(Self {
            service,
            database_path,
            settings_path,
            settings: Mutex::new(settings),
            port: AtomicU16::new(WEB_PORT),
            running: AtomicBool::new(false),
            sync_running: AtomicBool::new(false),
            last_activity: AtomicU64::new(0),
            sync_status: Mutex::new(sync_status),
            shutdown: std::sync::Mutex::new(None),
        })
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port.load(Ordering::SeqCst))
    }

    fn now_millis() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn sync_status(&self) -> SyncRuntimeStatus {
        self.sync_status.lock().unwrap().clone()
    }

    fn update_sync_status(
        &self,
        connected: Option<bool>,
        syncing: Option<bool>,
        error: Option<String>,
    ) {
        let mut status = self.sync_status.lock().unwrap();
        if let Some(connected) = connected {
            status.connected = connected;
        }
        status.syncing = syncing.unwrap_or(false);
        status.last_error = error;
    }

    pub async fn auto_sync_loop(self: Arc<Self>) {
        loop {
            let settings = normalize_supabase_settings(self.settings.lock().unwrap().clone());
            if !settings.auto_sync {
                {
                    let mut status = self.sync_status.lock().unwrap();
                    status.auto_sync = false;
                    status.next_sync_at = None;
                }
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            }

            let interval = Duration::from_secs(settings.sync_interval_minutes * 60);
            {
                let mut status = self.sync_status.lock().unwrap();
                status.auto_sync = true;
                status.interval_minutes = settings.sync_interval_minutes;
                status.next_sync_at = Some(Self::now_millis() + interval.as_millis() as u64);
            }
            if let Err(error) = perform_supabase_sync(self.clone()).await {
                self.update_sync_status(Some(false), Some(false), Some(error));
            }
            {
                let mut status = self.sync_status.lock().unwrap();
                status.next_sync_at = Some(Self::now_millis() + interval.as_millis() as u64);
            }
            tokio::time::sleep(interval).await;
        }
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
            .route(
                "/api/settings/supabase",
                get(get_supabase_settings).put(update_supabase_settings),
            )
            .route("/api/sync", post(sync))
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
        "syncStatus": state.sync_status(),
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

fn supabase_settings_json(
    state: &WebConsoleState,
    settings: &SupabaseSettings,
) -> serde_json::Value {
    json!({
        "configured": !settings.url.is_empty() && !settings.key.is_empty(),
        "url": settings.url,
        "key": settings.key,
        "schema": settings.schema,
        "authEmail": settings.auth_email,
        "authPassword": settings.auth_password,
        "autoSync": settings.auto_sync,
        "syncIntervalMinutes": settings.sync_interval_minutes,
        "tables": settings.tables,
        "settingsPath": state.settings_path.display().to_string(),
    })
}

async fn get_supabase_settings(
    State(state): State<Arc<WebConsoleState>>,
) -> Json<serde_json::Value> {
    state.touch();
    let settings = state.settings.lock().unwrap().clone();
    Json(supabase_settings_json(&state, &settings))
}

async fn update_supabase_settings(
    State(state): State<Arc<WebConsoleState>>,
    Json(input): Json<SupabaseSettings>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.touch();
    let settings = normalize_supabase_settings(input);
    if let Err(error) = validate_supabase_settings(&settings) {
        return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": error }))));
    }
    if let Err(error) = save_supabase_settings(&state.settings_path, &settings) {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        ));
    }
    *state.settings.lock().unwrap() = settings.clone();
    {
        let mut status = state.sync_status.lock().unwrap();
        status.auto_sync = settings.auto_sync;
        status.interval_minutes = settings.sync_interval_minutes;
        if !settings.auto_sync {
            status.next_sync_at = None;
        }
    }
    Ok(Json(supabase_settings_json(&state, &settings)))
}

async fn sync(
    State(state): State<Arc<WebConsoleState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    state.touch();
    match perform_supabase_sync(state.clone()).await {
        Ok(outcome) => Ok(Json(
            serde_json::to_value(outcome).unwrap_or_else(|_| json!({ "ok": true })),
        )),
        Err(error) => Err((StatusCode::BAD_GATEWAY, Json(json!({ "error": error })))),
    }
}

#[derive(Debug, Deserialize)]
struct SupabaseAuthUser {
    id: String,
}

#[derive(Debug, Deserialize)]
struct SupabaseAuthResponse {
    access_token: String,
    user: SupabaseAuthUser,
}

#[derive(Debug)]
struct AuthenticatedSupabase {
    access_token: String,
    user_id: String,
}

async fn authenticate_supabase(
    client: &reqwest::Client,
    settings: &SupabaseSettings,
) -> Result<AuthenticatedSupabase, String> {
    let response = client
        .post(format!(
            "{}/auth/v1/token?grant_type=password",
            settings.url
        ))
        .header("apikey", &settings.key)
        .json(&json!({
            "email": settings.auth_email,
            "password": settings.auth_password,
        }))
        .send()
        .await
        .map_err(|error| format!("无法连接 Supabase Auth: {error}"))?;
    let status = response.status();
    let payload: Value = response
        .json()
        .await
        .map_err(|error| format!("Supabase Auth 响应无法解析: {error}"))?;
    if !status.is_success() {
        let message = payload
            .get("msg")
            .or_else(|| payload.get("error"))
            .or_else(|| payload.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("登录 Supabase 失败");
        return Err(message.to_owned());
    }
    let auth: SupabaseAuthResponse =
        serde_json::from_value(payload).map_err(|_| "Supabase Auth 响应格式不正确".to_owned())?;
    Ok(AuthenticatedSupabase {
        access_token: auth.access_token,
        user_id: auth.user.id,
    })
}

fn normalize_remote_timestamp(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|time| {
            time.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Millis, true)
        })
        .unwrap_or_else(|_| value.to_owned())
}

fn normalize_remote_record(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.remove("user_id");
        for field in [
            "created_at",
            "updated_at",
            "started_at",
            "ended_at",
            "completed_at",
            "deleted_at",
        ] {
            if let Some(timestamp) = object.get(field).and_then(Value::as_str) {
                let normalized = normalize_remote_timestamp(timestamp);
                object.insert(field.to_owned(), Value::String(normalized));
            }
        }
    }
    value
}

async fn fetch_remote_table(
    client: &reqwest::Client,
    settings: &SupabaseSettings,
    auth: &AuthenticatedSupabase,
    table: &str,
) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    let mut offset = 0;
    loop {
        let url = format!(
            "{}/rest/v1/{}?select=*&limit=1000&offset={}&order=id.asc",
            settings.url, table, offset
        );
        let response = client
            .get(url)
            .header("apikey", &settings.key)
            .header("Authorization", format!("Bearer {}", auth.access_token))
            .header("Accept-Profile", &settings.schema)
            .send()
            .await
            .map_err(|error| format!("读取 Supabase 表 {table} 失败: {error}"))?;
        let status = response.status();
        let payload: Value = response
            .json()
            .await
            .map_err(|error| format!("Supabase 表 {table} 响应无法解析: {error}"))?;
        if !status.is_success() {
            let message = payload
                .get("message")
                .or_else(|| payload.get("error"))
                .and_then(Value::as_str)
                .unwrap_or("读取 Supabase 数据失败");
            return Err(format!("读取表 {table} 失败: {message}"));
        }
        let page: Vec<Value> = serde_json::from_value(payload)
            .map_err(|_| format!("Supabase 表 {table} 返回格式不正确"))?;
        let page_len = page.len();
        rows.extend(page);
        if page_len < 1000 {
            break;
        }
        offset += page_len;
    }
    Ok(rows)
}

fn deserialize_remote_records<T: serde::de::DeserializeOwned>(
    rows: Vec<Value>,
) -> Result<Vec<T>, String> {
    rows.into_iter()
        .map(|row| {
            serde_json::from_value(normalize_remote_record(row))
                .map_err(|error| format!("远端数据与本地模型不匹配: {error}"))
        })
        .collect()
}

fn prepare_remote_row(mut value: Value, user_id: &str) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.remove("is_blocked");
        object.insert("user_id".to_owned(), Value::String(user_id.to_owned()));
    }
    value
}

async fn upsert_remote_table(
    client: &reqwest::Client,
    settings: &SupabaseSettings,
    auth: &AuthenticatedSupabase,
    table: &str,
    rows: Vec<Value>,
    user_id: &str,
) -> Result<(), String> {
    for chunk in rows.chunks(500) {
        let rows: Vec<Value> = chunk
            .iter()
            .map(|row| prepare_remote_row(row.clone(), user_id))
            .collect();
        let response = client
            .post(format!("{}/rest/v1/{table}", settings.url))
            .header("apikey", &settings.key)
            .header("Authorization", format!("Bearer {}", auth.access_token))
            .header("Accept-Profile", &settings.schema)
            .header("Content-Profile", &settings.schema)
            .header("Prefer", "resolution=merge-duplicates,return=minimal")
            .json(&rows)
            .send()
            .await
            .map_err(|error| format!("上传 Supabase 表 {table} 失败: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            let payload: Value = response.json().await.unwrap_or(Value::Null);
            let message = payload
                .get("message")
                .or_else(|| payload.get("error"))
                .and_then(Value::as_str)
                .unwrap_or("上传 Supabase 数据失败");
            return Err(format!("上传表 {table} 失败: {message}"));
        }
    }
    Ok(())
}

async fn perform_supabase_sync(state: Arc<WebConsoleState>) -> Result<SyncOutcome, String> {
    if state
        .sync_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("同步正在进行中".to_owned());
    }
    state.update_sync_status(None, Some(true), None);
    let result = sync_supabase_once(&state).await;
    state.sync_running.store(false, Ordering::SeqCst);
    match result {
        Ok(outcome) => {
            state.update_sync_status(Some(true), Some(false), None);
            Ok(outcome)
        }
        Err(error) => {
            state.update_sync_status(Some(false), Some(false), Some(error.clone()));
            Err(error)
        }
    }
}

async fn sync_supabase_once(state: &WebConsoleState) -> Result<SyncOutcome, String> {
    let settings = normalize_supabase_settings(state.settings.lock().unwrap().clone());
    validate_supabase_settings(&settings)?;
    let local = state
        .service
        .sync_snapshot()
        .map_err(|error| format!("读取本地数据失败: {error}"))?;
    let local_before = local.counts();

    let client = reqwest::Client::new();
    let auth = authenticate_supabase(&client, &settings).await?;
    let remote = SyncSnapshot {
        workspaces: deserialize_remote_records(
            fetch_remote_table(&client, &settings, &auth, &settings.tables.workspaces).await?,
        )?,
        priorities: deserialize_remote_records(
            fetch_remote_table(&client, &settings, &auth, &settings.tables.priorities).await?,
        )?,
        tasks: deserialize_remote_records(
            fetch_remote_table(&client, &settings, &auth, &settings.tables.tasks).await?,
        )?,
        task_blocks: deserialize_remote_records(
            fetch_remote_table(&client, &settings, &auth, &settings.tables.task_blocks).await?,
        )?,
        work_sessions: deserialize_remote_records(
            fetch_remote_table(&client, &settings, &auth, &settings.tables.work_sessions).await?,
        )?,
    };
    let remote_before = remote.counts();
    let merged = cardhannis_core::merge_sync_snapshots(local, remote);
    let merged_counts = merged.counts();

    state
        .service
        .apply_sync_snapshot(merged.clone())
        .map_err(|error| format!("写入本地数据库失败: {error}"))?;

    let workspace_rows =
        serde_json::to_value(&merged.workspaces).unwrap_or(Value::Array(Vec::new()));
    let priority_rows =
        serde_json::to_value(&merged.priorities).unwrap_or(Value::Array(Vec::new()));
    let task_rows = serde_json::to_value(&merged.tasks).unwrap_or(Value::Array(Vec::new()));
    let block_rows = serde_json::to_value(&merged.task_blocks).unwrap_or(Value::Array(Vec::new()));
    let session_rows =
        serde_json::to_value(&merged.work_sessions).unwrap_or(Value::Array(Vec::new()));
    let rows = [
        (&settings.tables.workspaces, workspace_rows),
        (&settings.tables.priorities, priority_rows),
        (&settings.tables.tasks, task_rows),
        (&settings.tables.task_blocks, block_rows),
        (&settings.tables.work_sessions, session_rows),
    ];
    for (table, rows) in rows {
        let rows = rows.as_array().cloned().unwrap_or_default();
        upsert_remote_table(&client, &settings, &auth, table, rows, &auth.user_id).await?;
    }

    let synced_at = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    {
        let mut status = state.sync_status.lock().unwrap();
        status.last_synced_at = Some(synced_at.clone());
    }
    Ok(SyncOutcome {
        ok: true,
        message: "本地与 Supabase 已同步".to_owned(),
        local_before,
        remote_before,
        merged: merged_counts,
        synced_at,
    })
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
main { max-width: 760px; margin: 0 auto; padding: 26px 24px 78px; display: grid; gap: 16px; }
.card { border-radius: 14px; background: #fffdf6; box-shadow: 0 10px 28px rgba(45,52,44,.1), inset 0 0 0 1px rgba(122,112,82,.15); padding: 18px 20px; }
.card h2 { margin: 0 0 14px; font-size: 15px; color: #325549; }
dl { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 20px; margin: 0; }
dt { font-size: 11px; color: #8b8f80; margin-bottom: 3px; }
dd { margin: 0; font-size: 14px; font-weight: 650; }
.actions { display: flex; justify-content: flex-end; gap: 10px; }
.form-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px 14px; }
.form-grid label { display: grid; gap: 5px; font-size: 11px; color: #8b8f80; }
.form-grid .full { grid-column: 1 / -1; }
.form-grid input { border: 1px solid #d9d3c2; border-radius: 7px; background: #fff; color: #33403a; padding: 8px 10px; font-size: 13px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; min-width: 0; }
.form-grid input:focus { outline: 2px solid rgba(49,87,74,.22); border-color: #31574a; }
.checkbox { display: flex !important; align-items: center; gap: 7px !important; }
.checkbox input { width: auto; }
.form-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 10px; margin-top: 14px; }
button { border: 0; border-radius: 8px; padding: 9px 14px; font-size: 13px; font-weight: 650; cursor: pointer; }
button:disabled { opacity: .55; cursor: not-allowed; }
.primary { background: #31574a; color: #fff; }
.ghost { background: #eceadf; color: #6b6f63; }
.note { font-size: 12px; color: #8b8f80; margin-top: 12px; }
.status { margin: 12px 0 0; padding: 9px 11px; border-radius: 8px; background: #f0efe6; color: #687066; font-size: 12px; }
.status[data-state="ok"] { background: #e7f0e8; color: #31574a; }
.status[data-state="error"] { background: #f7e8e3; color: #9d4a33; }
.sync-indicator { position: fixed; left: 18px; bottom: 18px; z-index: 10; display: inline-flex; align-items: center; gap: 7px; padding: 8px 12px; border-radius: 999px; background: #fffdf6; box-shadow: 0 8px 22px rgba(45,52,44,.16), inset 0 0 0 1px rgba(122,112,82,.18); color: #687066; font-size: 12px; font-weight: 700; }
.sync-indicator::before { content: ""; width: 7px; height: 7px; border-radius: 50%; background: #b9b29b; }
.sync-indicator[data-state="connected"] { color: #31574a; }
.sync-indicator[data-state="connected"]::before { background: #3f8f63; }
.sync-indicator[data-state="error"] { color: #9d4a33; }
.sync-indicator[data-state="error"]::before { background: #c25b40; }
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
    <h2>Supabase 同步设置</h2>
    <form id="supabase-form">
      <div class="form-grid">
        <label class="full">Project URL<input id="supabase-url" type="url" required placeholder="https://xxxxxxxx.supabase.co" autocomplete="off" spellcheck="false" /></label>
        <label class="full">Publishable / Anon Key<input id="supabase-key" type="password" required placeholder="sb_publishable_... 或旧版 anon key" autocomplete="off" spellcheck="false" /></label>
        <label class="checkbox"><input id="show-key" type="checkbox" />显示 Key</label>
        <label>Schema<input id="supabase-schema" required autocomplete="off" spellcheck="false" /></label>
        <label class="full">Auth Email<input id="auth-email" type="email" required autocomplete="off" spellcheck="false" /></label>
        <label class="full">Auth Password<input id="auth-password" type="password" required autocomplete="off" spellcheck="false" /></label>
        <label class="checkbox"><input id="show-auth-password" type="checkbox" />显示 Auth Password</label>
        <label>自动同步间隔（分钟）<input id="sync-interval" type="number" min="1" max="1440" step="1" required /></label>
        <label class="checkbox"><input id="auto-sync" type="checkbox" />每 5 分钟自动拉取并上传</label>
        <label>工作区表<input id="table-workspaces" required autocomplete="off" spellcheck="false" /></label>
        <label>分级表<input id="table-priorities" required autocomplete="off" spellcheck="false" /></label>
        <label>任务表<input id="table-tasks" required autocomplete="off" spellcheck="false" /></label>
        <label>阻塞表<input id="table-task-blocks" required autocomplete="off" spellcheck="false" /></label>
        <label>工作会话表<input id="table-work-sessions" required autocomplete="off" spellcheck="false" /></label>
      </div>
      <div class="form-actions">
        <button class="ghost" type="button" id="reset-tables">恢复默认表名</button>
        <button class="primary" type="submit" id="save-supabase">保存设置</button>
        <button class="primary" type="button" id="sync-now">同步</button>
      </div>
    </form>
    <p class="note">只填写 publishable/anon key，不要填写 service_role/secret key。当前版本不会自动同步；「同步」按钮只返回预留状态。</p>
    <p class="note">配置文件：<span id="settings-path">—</span></p>
    <p class="status" id="supabase-status" data-state="idle">正在读取 Supabase 设置…</p>
  </section>
  <section class="card actions">
    <button class="ghost" id="refresh">刷新</button>
    <button class="primary" id="shutdown">关闭 Web 控制台</button>
  </section>
</main>
<div class="sync-indicator" id="sync-indicator" data-state="idle">连接数据库…</div>
<script>
const $ = (id) => document.getElementById(id);
let autoCloseAt = 0;

function formatDuration(minutes) {
  if (minutes < 60) return `${minutes}m`;
  const hours = minutes / 60;
  return `${Number.isInteger(hours) ? hours : hours.toFixed(1)}h`;
}
let touchTimer = null;

function renderSyncIndicator(status) {
  const indicator = $('sync-indicator');
  if (!status) return;
  if (status.connected) {
    indicator.textContent = '已连接数据库';
    indicator.dataset.state = 'connected';
  } else if (status.lastError) {
    indicator.textContent = '数据库连接失败';
    indicator.dataset.state = 'error';
    indicator.title = status.lastError;
  } else if (status.syncing) {
    indicator.textContent = '数据库同步中…';
    indicator.dataset.state = 'idle';
  } else {
    indicator.textContent = status.autoSync ? '等待数据库同步…' : '数据库未连接';
    indicator.dataset.state = 'idle';
  }
}

function render(data) {
  renderSyncIndicator(data.syncStatus);
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

const DEFAULT_TABLES = {
  workspaces: 'workspaces',
  priorities: 'priorities',
  tasks: 'tasks',
  taskBlocks: 'task_blocks',
  workSessions: 'work_sessions',
};

function setSupabaseStatus(message, state = 'idle') {
  const element = $('supabase-status');
  element.textContent = message;
  element.dataset.state = state;
}

function currentSupabaseSettings() {
  return {
    url: $('supabase-url').value.trim(),
    key: $('supabase-key').value.trim(),
    schema: $('supabase-schema').value.trim(),
    authEmail: $('auth-email').value.trim(),
    authPassword: $('auth-password').value,
    autoSync: $('auto-sync').checked,
    syncIntervalMinutes: Number($('sync-interval').value),
    tables: {
      workspaces: $('table-workspaces').value.trim(),
      priorities: $('table-priorities').value.trim(),
      tasks: $('table-tasks').value.trim(),
      task_blocks: $('table-task-blocks').value.trim(),
      work_sessions: $('table-work-sessions').value.trim(),
    },
  };
}

function renderSupabaseSettings(data) {
  $('supabase-url').value = data.url || '';
  $('supabase-key').value = data.key || '';
  $('supabase-schema').value = data.schema || 'public';
  $('auth-email').value = data.authEmail || '';
  $('auth-password').value = data.authPassword || '';
  $('auto-sync').checked = data.autoSync !== false;
  $('sync-interval').value = data.syncIntervalMinutes || 5;
  $('table-workspaces').value = data.tables?.workspaces || DEFAULT_TABLES.workspaces;
  $('table-priorities').value = data.tables?.priorities || DEFAULT_TABLES.priorities;
  $('table-tasks').value = data.tables?.tasks || DEFAULT_TABLES.tasks;
  $('table-task-blocks').value = data.tables?.task_blocks || DEFAULT_TABLES.taskBlocks;
  $('table-work-sessions').value = data.tables?.work_sessions || DEFAULT_TABLES.workSessions;
  $('settings-path').textContent = data.settingsPath || '—';
  setSupabaseStatus(data.configured ? 'Supabase 配置已填写。' : 'Supabase 尚未配置。', data.configured ? 'ok' : 'idle');
}

async function requestJson(url, options = {}) {
  const response = await fetch(url, options);
  let data = {};
  try { data = await response.json(); } catch {}
  if (!response.ok) throw new Error(data.error || `请求失败：HTTP ${response.status}`);
  return data;
}

async function loadSupabaseSettings() {
  try {
    const data = await requestJson('/api/settings/supabase');
    renderSupabaseSettings(data);
  } catch (error) {
    setSupabaseStatus(error.message, 'error');
  }
}

$('show-key').addEventListener('change', () => {
  $('supabase-key').type = $('show-key').checked ? 'text' : 'password';
});

$('show-auth-password').addEventListener('change', () => {
  $('auth-password').type = $('show-auth-password').checked ? 'text' : 'password';
});

$('reset-tables').addEventListener('click', () => {
  $('supabase-schema').value = 'public';
  $('table-workspaces').value = DEFAULT_TABLES.workspaces;
  $('table-priorities').value = DEFAULT_TABLES.priorities;
  $('table-tasks').value = DEFAULT_TABLES.tasks;
  $('table-task-blocks').value = DEFAULT_TABLES.taskBlocks;
  $('table-work-sessions').value = DEFAULT_TABLES.workSessions;
  setSupabaseStatus('已恢复默认 schema 和表名，尚未保存。');
});

$('supabase-form').addEventListener('submit', async (event) => {
  event.preventDefault();
  const saveButton = $('save-supabase');
  saveButton.disabled = true;
  setSupabaseStatus('正在保存 Supabase 设置…');
  try {
    const data = await requestJson('/api/settings/supabase', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(currentSupabaseSettings()),
    });
    renderSupabaseSettings(data);
    setSupabaseStatus('Supabase 设置已保存。', 'ok');
  } catch (error) {
    setSupabaseStatus(error.message, 'error');
  } finally {
    saveButton.disabled = false;
  }
});

$('sync-now').addEventListener('click', async () => {
  const syncButton = $('sync-now');
  syncButton.disabled = true;
  setSupabaseStatus('正在请求同步…');
  try {
    const data = await requestJson('/api/sync', { method: 'POST' });
    setSupabaseStatus(data.message || '同步完成。', data.ok ? 'ok' : 'idle');
    await refresh();
  } catch (error) {
    setSupabaseStatus(error.message, 'error');
  } finally {
    syncButton.disabled = false;
  }
});

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
loadSupabaseSettings();
</script>
</body>
</html>"##;
