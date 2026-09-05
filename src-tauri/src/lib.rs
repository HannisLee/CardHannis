use cardhannis_core::{TaskService, TaskStore};
use std::{fs, sync::Arc};
use tauri::{
    Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};

pub struct AppState {
    pub service: Arc<TaskService>,
    pub device_id: String,
    pub web: Arc<crate::web::WebConsoleState>,
    pub tray: tauri::tray::TrayIcon,
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

fn toggle_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
        } else {
            let _ = window.show();
            let _ = window.set_focus();
        }
    }
}

mod commands {
    use super::AppState;
    use cardhannis_core::{
        BlockTaskCommand, CreateTaskCommand, Priority, Task, TaskBlock, WorkSession, Workspace,
    };
    use tauri::State;

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

    // ===== 优先级分级 =====
    #[tauri::command]
    pub fn list_priorities(state: State<'_, AppState>) -> Result<Vec<Priority>, String> {
        service(&state)?.priorities(false).map_err(error_message)
    }

    #[tauri::command]
    pub fn create_priority(
        state: State<'_, AppState>,
        name: String,
        color: Option<String>,
    ) -> Result<Priority, String> {
        service(&state)?
            .create_priority(&name, color.as_deref())
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
        .setup(|app| {
            // 与 Web 原型共享同一数据库；路径解析跨平台（macOS/Windows/Linux 同一规则）
            let data_dir = shared_data_dir(&app);
            fs::create_dir_all(&data_dir)?;
            let database_path = data_dir.join("cardhannis.sqlite3");
            let store =
                TaskStore::open(database_path.clone()).map_err(|error| error.to_string())?;
            let service = Arc::new(TaskService::new(store));
            let web = crate::web::WebConsoleState::new(service.clone(), database_path);
            let hostname = if cfg!(target_os = "windows") {
                std::env::var("COMPUTERNAME").unwrap_or_else(|_| "device".into())
            } else {
                std::env::var("HOSTNAME").unwrap_or_else(|_| "device".into())
            };
            let device_id = format!("{}-{}", std::env::consts::OS, hostname);
            #[cfg(target_os = "macos")]
            let _ = app
                .handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory);

            let toggle_item =
                MenuItem::with_id(app, "toggle-window", "显示 / 隐藏窗口", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "退出 CardHannis", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&toggle_item, &quit_item])?;
            let tray_icon = app
                .handle()
                .default_window_icon()
                .expect("缺少应用图标")
                .clone();
            let tray = TrayIconBuilder::with_id("main")
                .icon(tray_icon)
                .icon_as_template(cfg!(target_os = "macos"))
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
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::open_web_console,
            commands::create_task,
            commands::complete_task,
            commands::delete_task,
            commands::pause_task,
            commands::list_sessions,
            commands::reopen_task,
            commands::start_work,
            commands::finish_work,
            commands::list_blocks,
            commands::block_task,
            commands::unblock_task,
            commands::list_workspaces,
            commands::create_workspace,
            commands::rename_workspace,
            commands::delete_workspace,
            commands::list_priorities,
            commands::create_priority,
            commands::update_priority,
            commands::delete_priority
        ])
        .run(tauri::generate_context!())
        .expect("运行 CardHannis 时发生错误");
}
