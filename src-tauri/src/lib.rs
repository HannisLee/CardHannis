use cardhannis_core::{TaskService, TaskStore};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{
    Emitter, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub struct AppState {
    pub service: Arc<TaskService>,
    pub device_id: String,
    pub web: Arc<crate::web::WebConsoleState>,
    pub tray: tauri::tray::TrayIcon,
    pub settings: Mutex<AppSettings>,
    pub settings_path: PathBuf,
}

fn create_tray_icon() -> tauri::image::Image<'static> {
    let image = tauri::include_image!("icons/128x128.png");
    let width = image.width() as usize;
    let height = image.height() as usize;
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for (index, pixel) in image.rgba().chunks_exact(4).enumerate() {
        // 忽略几乎透明的边缘，让图案充分占用系统固定的托盘区域。
        if pixel[3] <= 16 {
            continue;
        }
        let (x, y) = (index % width, index / width);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    if left > right || top > bottom {
        return image;
    }

    // 保留少量边缘和方形画布，避免裁掉抗锯齿或改变图案比例。
    left = left.saturating_sub(2);
    top = top.saturating_sub(2);
    right = (right + 2).min(width - 1);
    bottom = (bottom + 2).min(height - 1);
    let crop_width = right - left + 1;
    let crop_height = bottom - top + 1;
    let size = crop_width.max(crop_height);
    let offset_x = (size - crop_width) / 2;
    let offset_y = (size - crop_height) / 2;
    let mut rgba = vec![0; size * size * 4];
    for y in 0..crop_height {
        let source = ((top + y) * width + left) * 4;
        let target = ((offset_y + y) * size + offset_x) * 4;
        rgba[target..target + crop_width * 4]
            .copy_from_slice(&image.rgba()[source..source + crop_width * 4]);
    }
    tauri::image::Image::new_owned(rgba, size as u32, size as u32)
}

/// 桌面端与 Web 原型共用的数据目录。
/// macOS: ~/Library/Application Support/CardHannis
/// Windows: %APPDATA%/CardHannis
/// Linux: ~/.local/share/CardHannis
/// 兜底: Tauri app_data_dir
fn shared_data_dir(app: &tauri::App) -> std::path::PathBuf {
    use std::path::PathBuf;
    if cfg!(target_os = "macos") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Application Support/CardHannis");
        }
    } else if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("CardHannis");
        }
    } else if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local/share/CardHannis");
    }
    app.path().app_data_dir().expect("无法定位应用数据目录")
}

mod web;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub launch_at_login: bool,
    pub start_hidden: bool,
    pub always_on_top: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            start_hidden: false,
            always_on_top: true,
        }
    }
}

fn load_app_settings(path: &PathBuf) -> AppSettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn save_app_settings(path: &PathBuf, settings: AppSettings) -> Result<(), String> {
    let contents = serde_json::to_vec_pretty(&settings)
        .map_err(|error| format!("无法序列化应用设置: {error}"))?;
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, contents).map_err(|error| format!("无法写入应用设置: {error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("无法保存应用设置: {error}"))
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    fn test_settings_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cardhannis-{name}-{}.json", std::process::id()))
    }

    #[test]
    fn app_settings_round_trip() {
        let path = test_settings_path("settings-round-trip");
        let settings = AppSettings {
            launch_at_login: true,
            start_hidden: true,
            always_on_top: false,
        };
        save_app_settings(&path, settings).expect("保存设置应该成功");
        assert_eq!(load_app_settings(&path), settings);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn invalid_app_settings_fall_back_to_defaults() {
        let path = test_settings_path("settings-invalid");
        fs::write(&path, "{invalid").expect("写入测试设置应该成功");
        assert_eq!(load_app_settings(&path), AppSettings::default());
        let _ = fs::remove_file(path);
    }
}

fn toggle_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let visible = window.is_visible().unwrap_or(false);
        let focused = window.is_focused().unwrap_or(false);
        if !visible || !focused {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        } else {
            let _ = window.hide();
        }
    }
}

mod commands {
    use super::AppState;
    use super::{AppSettings, save_app_settings};
    use cardhannis_core::{
        BlockTaskCommand, CreateTaskCommand, Priority, Task, TaskBlock, UpdateTaskCommand,
        WorkSession, Workspace,
    };
    use tauri::State;
    use tauri_plugin_autostart::ManagerExt;

    fn service<'a>(
        state: &'a State<'_, AppState>,
    ) -> Result<&'a cardhannis_core::TaskService, String> {
        Ok(&state.service)
    }

    fn error_message(error: cardhannis_core::CoreError) -> String {
        error.to_string()
    }

    #[tauri::command]
    pub fn list_tasks(state: State<'_, AppState>) -> Result<Vec<Task>, String> {
        service(&state)?.list(false).map_err(error_message)
    }

    #[tauri::command]
    pub async fn open_web_console(state: State<'_, AppState>) -> Result<(), String> {
        state.web.start().await?;
        let url = state.web.url();
        webbrowser::open(&url).map_err(|error| format!("无法打开浏览器: {error}"))?;
        Ok(())
    }

    #[tauri::command]
    pub fn sync_status(state: State<'_, AppState>) -> crate::web::SyncRuntimeStatus {
        state.web.sync_status()
    }

    #[tauri::command]
    pub fn list_system_fonts() -> Vec<String> {
        #[cfg(target_os = "windows")]
        {
            use std::collections::BTreeSet;
            use winreg::{RegKey, enums::*};

            let mut fonts = BTreeSet::new();
            for root in [
                RegKey::predef(HKEY_CURRENT_USER),
                RegKey::predef(HKEY_LOCAL_MACHINE),
            ] {
                let Ok(key) =
                    root.open_subkey("Software\\Microsoft\\Windows NT\\CurrentVersion\\Fonts")
                else {
                    continue;
                };
                for value in key.enum_values().flatten() {
                    let name = value.0;
                    let family = name
                        .split_once(" (")
                        .map(|(family, _)| family)
                        .unwrap_or(name.as_str())
                        .trim();
                    if !family.is_empty() {
                        fonts.insert(family.to_owned());
                    }
                }
            }
            return fonts.into_iter().collect();
        }

        #[cfg(not(target_os = "windows"))]
        {
            Vec::new()
        }
    }

    #[tauri::command]
    pub fn get_app_settings(
        app: tauri::AppHandle,
        state: State<'_, AppState>,
    ) -> Result<AppSettings, String> {
        let enabled = app
            .autolaunch()
            .is_enabled()
            .map_err(|error| format!("无法读取开机自启动状态: {error}"))?;
        let mut settings = state.settings.lock().expect("应用设置锁中毒");
        if settings.launch_at_login != enabled {
            settings.launch_at_login = enabled;
            save_app_settings(&state.settings_path, *settings)?;
        }
        Ok(*settings)
    }

    #[tauri::command]
    pub fn set_app_settings(
        app: tauri::AppHandle,
        state: State<'_, AppState>,
        settings: AppSettings,
    ) -> Result<AppSettings, String> {
        let autolaunch = app.autolaunch();
        if settings.launch_at_login {
            autolaunch
                .enable()
                .map_err(|error| format!("无法开启开机自启动: {error}"))?;
        } else {
            autolaunch
                .disable()
                .map_err(|error| format!("无法关闭开机自启动: {error}"))?;
        }

        let mut current = state.settings.lock().expect("应用设置锁中毒");
        *current = settings;
        let path = state.settings_path.clone();
        save_app_settings(&path, settings)?;
        Ok(settings)
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CreateTaskInput {
        pub title: String,
        pub notes: Option<String>,
        pub estimated_active_minutes: Option<i64>,
        pub due_date: Option<String>,
        pub workspace_id: Option<String>,
        pub priority_id: Option<String>,
    }

    #[tauri::command]
    pub fn create_task(state: State<'_, AppState>, input: CreateTaskInput) -> Result<Task, String> {
        service(&state)?
            .create(CreateTaskCommand {
                title: input.title,
                notes: input.notes,
                estimated_active_minutes: input.estimated_active_minutes,
                due_date: input.due_date,
                sort_order: 0,
                created_device_id: state.device_id.clone(),
                workspace_id: input.workspace_id,
                priority_id: input.priority_id,
            })
            .map_err(error_message)
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct UpdateTaskInput {
        pub title: String,
        pub notes: Option<String>,
        pub review_notes: Option<String>,
        pub estimated_active_minutes: Option<i64>,
        pub due_date: Option<String>,
        pub workspace_id: Option<String>,
        pub priority_id: Option<String>,
    }

    #[tauri::command]
    pub fn update_task(
        state: State<'_, AppState>,
        id: String,
        expected_version: i64,
        input: UpdateTaskInput,
    ) -> Result<Task, String> {
        let service = service(&state)?;
        let existing = service
            .get(&id)
            .map_err(error_message)?
            .ok_or_else(|| "任务不存在".to_string())?;
        service
            .update(
                &id,
                expected_version,
                UpdateTaskCommand {
                    title: input.title,
                    notes: input.notes,
                    review_notes: input.review_notes,
                    estimated_active_minutes: input.estimated_active_minutes,
                    due_date: input.due_date,
                    sort_order: existing.sort_order,
                    workspace_id: input.workspace_id,
                    priority_id: input.priority_id,
                },
            )
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn complete_task(
        state: State<'_, AppState>,
        id: String,
        expected_version: i64,
    ) -> Result<Task, String> {
        service(&state)?
            .complete(&id, expected_version)
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn delete_task(
        state: State<'_, AppState>,
        id: String,
        expected_version: i64,
    ) -> Result<(), String> {
        service(&state)?
            .delete(&id, expected_version)
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn start_work(state: State<'_, AppState>, task_id: String) -> Result<WorkSession, String> {
        service(&state)?
            .begin_work(&task_id, None)
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn finish_work(
        state: State<'_, AppState>,
        session_id: String,
    ) -> Result<WorkSession, String> {
        service(&state)?
            .finish_work(&session_id)
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn correct_work_time(
        state: State<'_, AppState>,
        task_id: String,
        expected_version: i64,
        target_active_minutes: i64,
    ) -> Result<Task, String> {
        service(&state)?
            .correct_work_time(&task_id, expected_version, target_active_minutes)
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn block_task(
        state: State<'_, AppState>,
        task_id: String,
        reason: String,
        note: Option<String>,
    ) -> Result<TaskBlock, String> {
        service(&state)?
            .block(&task_id, BlockTaskCommand { reason, note })
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn unblock_task(
        state: State<'_, AppState>,
        block_id: String,
        expected_version: i64,
        resolution_reason: Option<String>,
    ) -> Result<TaskBlock, String> {
        service(&state)?
            .unblock(&block_id, expected_version, resolution_reason.as_deref())
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn pause_task(
        state: State<'_, AppState>,
        id: String,
        expected_version: i64,
    ) -> Result<Task, String> {
        service(&state)?
            .pause(&id, expected_version)
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn list_sessions(
        state: State<'_, AppState>,
        task_id: String,
    ) -> Result<Vec<WorkSession>, String> {
        service(&state)?.sessions(&task_id).map_err(error_message)
    }

    #[tauri::command]
    pub fn reopen_task(
        state: State<'_, AppState>,
        id: String,
        expected_version: i64,
    ) -> Result<Task, String> {
        service(&state)?
            .reopen(&id, expected_version)
            .map_err(error_message)
    }

    // ===== 工作区 =====
    #[tauri::command]
    pub fn list_workspaces(state: State<'_, AppState>) -> Result<Vec<Workspace>, String> {
        service(&state)?.workspaces(false).map_err(error_message)
    }

    #[tauri::command]
    pub fn create_workspace(state: State<'_, AppState>, name: String) -> Result<Workspace, String> {
        service(&state)?
            .create_workspace(&name)
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn rename_workspace(
        state: State<'_, AppState>,
        id: String,
        expected_version: i64,
        name: String,
    ) -> Result<Workspace, String> {
        service(&state)?
            .rename_workspace(&id, expected_version, &name)
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn delete_workspace(
        state: State<'_, AppState>,
        id: String,
        expected_version: i64,
    ) -> Result<(), String> {
        service(&state)?
            .delete_workspace(&id, expected_version)
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn reorder_workspaces(
        state: State<'_, AppState>,
        ordered_ids: Vec<String>,
        expected_versions: Vec<i64>,
    ) -> Result<Vec<Workspace>, String> {
        service(&state)?
            .reorder_workspaces(&ordered_ids, &expected_versions)
            .map_err(error_message)
    }

    // ===== 优先级分级 =====
    #[tauri::command]
    pub fn list_priorities(state: State<'_, AppState>) -> Result<Vec<Priority>, String> {
        service(&state)?.priorities(false).map_err(error_message)
    }

    #[tauri::command]
    pub fn create_priority(
        state: State<'_, AppState>,
        workspace_id: String,
        name: String,
        color: Option<String>,
    ) -> Result<Priority, String> {
        service(&state)?
            .create_priority(&workspace_id, &name, color.as_deref())
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn update_priority(
        state: State<'_, AppState>,
        id: String,
        expected_version: i64,
        name: String,
        color: Option<String>,
    ) -> Result<Priority, String> {
        service(&state)?
            .update_priority(&id, expected_version, &name, color.as_deref())
            .map_err(error_message)
    }

    #[tauri::command]
    pub fn delete_priority(
        state: State<'_, AppState>,
        id: String,
        expected_version: i64,
    ) -> Result<(), String> {
        service(&state)?
            .delete_priority(&id, expected_version)
            .map_err(error_message)
    }
    #[tauri::command]
    pub fn list_blocks(
        state: State<'_, AppState>,
        task_id: String,
    ) -> Result<Vec<TaskBlock>, String> {
        service(&state)?
            .blocks(&task_id, false)
            .map_err(error_message)
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("CardHannis")
                .build(),
        )
        .setup(|app| {
            // 与 Web 原型共享同一数据库；路径解析跨平台（macOS/Windows/Linux 同一规则）
            let data_dir = shared_data_dir(&app);
            fs::create_dir_all(&data_dir)?;
            let database_path = data_dir.join("cardhannis.sqlite3");
            let settings_path = data_dir.join("settings.json");
            let app_settings = load_app_settings(&settings_path);
            let store =
                TaskStore::open(database_path.clone()).map_err(|error| error.to_string())?;
            let service = Arc::new(TaskService::new(store));
            let web = crate::web::WebConsoleState::new(service.clone(), database_path);
            let startup_show_window = !app_settings.start_hidden;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.with_webview(|webview| {
                    #[cfg(target_os = "macos")]
                    unsafe {
                        use objc2_app_kit::NSWindowCollectionBehavior as Behavior;

                        let ns_window: &objc2_app_kit::NSWindow = &*webview.ns_window().cast();
                        let mut behavior = ns_window.collectionBehavior();
                        // 浮动层级默认可能不参与 Mission Control；显式纳入系统窗口管理。
                        // 同组行为互斥，先清除冲突标记，并保留配置中的所有桌面显示行为。
                        behavior &=
                            !(Behavior::Transient | Behavior::Stationary | Behavior::IgnoresCycle);
                        behavior |= Behavior::Managed | Behavior::ParticipatesInCycle;
                        ns_window.setCollectionBehavior(behavior);
                    }
                });
                if let Ok(size) = window.outer_size() {
                    if size.width < 100 || size.height < 100 {
                        let _ = window
                            .set_size(tauri::Size::Logical(tauri::LogicalSize::new(340.0, 400.0)));
                    }
                }
                let _ = window.set_always_on_top(app_settings.always_on_top);
                if startup_show_window {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            let startup_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(300)).await;
                if !startup_show_window {
                    return;
                }
                if let Some(window) = startup_handle.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            });
            let auto_sync = web.clone();
            tauri::async_runtime::spawn(async move {
                auto_sync.auto_sync_loop().await;
            });

            // 后台兜底执行 2 小时计时上限；即使窗口隐藏或应用重启，也能把会话
            // 精确结束在 started_at + 2h，并让任务回到待办。
            let expiration_service = service.clone();
            let expiration_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut ticker = tokio::time::interval(Duration::from_secs(5));
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                loop {
                    ticker.tick().await;
                    match expiration_service.expire_work_sessions() {
                        Ok(expired) if !expired.is_empty() => {
                            let _ = expiration_handle.emit("work-sessions-expired", &expired);
                        }
                        Ok(_) => {}
                        Err(error) => eprintln!("检查工作会话计时上限失败: {error}"),
                    }
                }
            });
            let hostname = if cfg!(target_os = "windows") {
                std::env::var("COMPUTERNAME").unwrap_or_else(|_| "device".into())
            } else {
                std::env::var("HOSTNAME").unwrap_or_else(|_| "device".into())
            };
            let device_id = format!("{}-{}", std::env::consts::OS, hostname);
            let toggle_item =
                MenuItem::with_id(app, "toggle-window", "显示 / 隐藏窗口", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出 CardHannis", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;
            let tray = TrayIconBuilder::with_id("main")
                .icon(create_tray_icon())
                .icon_as_template(false)
                .tooltip("CardHannis")
                .menu(&tray_menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    if event.id() == "toggle-window" {
                        toggle_main_window(app);
                    } else if event.id() == "quit" {
                        app.exit(0);
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            app.manage(AppState {
                service,
                device_id,
                web,
                tray,
                settings: Mutex::new(app_settings),
                settings_path,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_settings,
            commands::set_app_settings,
            commands::list_tasks,
            commands::open_web_console,
            commands::sync_status,
            commands::list_system_fonts,
            commands::create_task,
            commands::update_task,
            commands::complete_task,
            commands::delete_task,
            commands::pause_task,
            commands::list_sessions,
            commands::reopen_task,
            commands::start_work,
            commands::finish_work,
            commands::correct_work_time,
            commands::list_blocks,
            commands::block_task,
            commands::unblock_task,
            commands::list_workspaces,
            commands::create_workspace,
            commands::rename_workspace,
            commands::delete_workspace,
            commands::reorder_workspaces,
            commands::list_priorities,
            commands::create_priority,
            commands::update_priority,
            commands::delete_priority
        ])
        .run(tauri::generate_context!())
        .expect("运行 CardHannis 时发生错误");
}
