use cardhannis_core::{TaskService, TaskStore};
use std::{fs, sync::Mutex};
use tauri::Manager;

pub struct AppState {
    pub service: Mutex<TaskService>,
    pub device_id: String,
}

mod commands {
    use super::AppState;
    use cardhannis_core::{
        BlockTaskCommand, CreateTaskCommand, Priority, Task, TaskBlock, WorkSession, Workspace,
    };
    use tauri::State;

    fn service<'a>(
        state: &'a State<'_, AppState>,
    ) -> Result<std::sync::MutexGuard<'a, cardhannis_core::TaskService>, String> {
        state
            .service
            .lock()
            .map_err(|_| "核心服务不可用".to_string())
    }

    fn error_message(error: cardhannis_core::CoreError) -> String {
        error.to_string()
    }

    #[tauri::command]
    pub fn list_tasks(state: State<'_, AppState>) -> Result<Vec<Task>, String> {
        service(&state)?.list(false).map_err(error_message)
    }

    #[derive(Debug, serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CreateTaskInput {
        pub title: String,
        pub notes: Option<String>,
        pub estimated_active_minutes: Option<i64>,
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
    ) -> Result<TaskBlock, String> {
        service(&state)?
            .unblock(&block_id, expected_version)
            .map_err(error_message)
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
            // 与 Web 原型共享同一数据库：~/Library/Application Support/CardHannis/
            let data_dir = std::env::var_os("HOME")
                .map(|home| {
                    std::path::PathBuf::from(home).join("Library/Application Support/CardHannis")
                })
                .unwrap_or_else(|| app.path().app_data_dir().expect("无法定位应用数据目录"));
            fs::create_dir_all(&data_dir)?;
            let database_path = data_dir.join("cardhannis.sqlite3");
            let store = TaskStore::open(database_path).map_err(|error| error.to_string())?;
            let device_id = format!(
                "macos-{}",
                std::env::var("HOSTNAME").unwrap_or_else(|_| "device".into())
            );
            app.manage(AppState {
                service: Mutex::new(TaskService::new(store)),
                device_id,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::create_task,
            commands::complete_task,
            commands::delete_task,
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
