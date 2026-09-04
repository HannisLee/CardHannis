import os
from pathlib import Path

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from . import models
from .store import StoreError, TaskStore
from .sync import SyncError, load_settings, save_settings, SupabaseClient, sync_store, validate_settings

def normalize_task(task: dict) -> dict:
    task["is_blocked"] = bool(task.get("is_blocked"))
    return task

def _default_db_path() -> Path:
    """与桌面端 shared_data_dir 同一套跨平台规则。"""
    import sys

    if sys.platform == "darwin":
        return Path.home() / "Library" / "Application Support" / "CardHannis" / "cardhannis.sqlite3"
    if os.name == "nt":
        base = Path(os.environ.get("APPDATA", str(Path.home())))
        return base / "CardHannis" / "cardhannis.sqlite3"
    return Path.home() / ".local" / "share" / "CardHannis" / "cardhannis.sqlite3"


DATABASE_PATH = Path(os.environ.get("CARDHANNIS_DB", str(_default_db_path())))

app = FastAPI(title="CardHannis WebUI", version="0.1.0")
app.state.store = TaskStore(DATABASE_PATH)


def store(request: Request) -> TaskStore:
    return request.app.state.store


def fail(error: StoreError) -> HTTPException:
    return HTTPException(status_code=400, detail=str(error))


@app.get("/api/tasks")
def list_tasks(include_deleted: bool = False, request: Request = None):
    return [normalize_task(task) for task in store(request).list_tasks(include_deleted)]


@app.post("/api/tasks", response_model=models.Task, status_code=201)
def create_task(input: models.TaskCreate, request: Request):
    return normalize_task(store(request).create_task(input, models.utc_now_iso()))


@app.get("/api/tasks/{task_id}")
def get_task(task_id: str, request: Request):
    task = store(request).get_task(task_id)
    if task is None:
        raise HTTPException(status_code=404, detail="任务不存在")
    return normalize_task(task)


@app.patch("/api/tasks/{task_id}", response_model=models.Task)
def update_task(task_id: str, input: models.TaskUpdate, request: Request):
    try:
        return normalize_task(store(request).update_task(task_id, input, models.utc_now_iso()))
    except StoreError as error:
        raise fail(error) from error


@app.post("/api/tasks/{task_id}/start", response_model=models.Task)
def start_task(task_id: str, expected_version: int, request: Request):
    try:
        return normalize_task(store(request).set_status(task_id, expected_version, "in_progress", models.utc_now_iso()))
    except StoreError as error:
        raise fail(error) from error


@app.post("/api/tasks/{task_id}/complete", response_model=models.Task)
def complete_task(task_id: str, expected_version: int, request: Request):
    try:
        return normalize_task(store(request).set_status(task_id, expected_version, "completed", models.utc_now_iso()))
    except StoreError as error:
        raise fail(error) from error


@app.delete("/api/tasks/{task_id}", status_code=204)
def delete_task(task_id: str, expected_version: int, request: Request):
    try:
        store(request).soft_delete_task(task_id, expected_version, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.get("/api/tasks/{task_id}/blocks", response_model=list[models.TaskBlock])
def list_blocks(task_id: str, include_deleted: bool = False, request: Request = None):
    return store(request).list_blocks(task_id, include_deleted)


@app.post("/api/tasks/{task_id}/blocks", response_model=models.TaskBlock, status_code=201)
def create_block(task_id: str, input: models.BlockCreate, request: Request):
    try:
        return store(request).start_block(task_id, input.reason, input.note, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.post("/api/blocks/{block_id}/end", response_model=models.TaskBlock)
def end_block(block_id: str, expected_version: int, request: Request):
    try:
        return store(request).end_block(block_id, expected_version, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.get("/api/tasks/{task_id}/sessions", response_model=list[models.WorkSession])
def list_sessions(task_id: str, request: Request):
    return store(request).list_sessions(task_id)


@app.post("/api/tasks/{task_id}/sessions", response_model=models.WorkSession, status_code=201)
def start_work(task_id: str, input: models.StartWorkInput, request: Request):
    try:
        return store(request).start_work(task_id, input.note, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.post("/api/tasks/{task_id}/reopen", response_model=models.Task)
def reopen_task(task_id: str, expected_version: int, request: Request):
    try:
        return normalize_task(store(request).reopen_task(task_id, expected_version, models.utc_now_iso()))
    except StoreError as error:
        raise fail(error) from error


@app.get("/api/workspaces", response_model=list[models.Workspace])
def list_workspaces(request: Request):
    return store(request).list_workspaces(False)


@app.post("/api/workspaces", response_model=models.Workspace, status_code=201)
def create_workspace(input: models.WorkspaceCreate, request: Request):
    try:
        return store(request).create_workspace(input.name, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.patch("/api/workspaces/{workspace_id}", response_model=models.Workspace)
def rename_workspace(workspace_id: str, input: models.WorkspaceRename, request: Request):
    try:
        return store(request).rename_workspace(workspace_id, input.expected_version, input.name, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.delete("/api/workspaces/{workspace_id}", status_code=204)
def delete_workspace(workspace_id: str, expected_version: int, request: Request):
    try:
        store(request).soft_delete_workspace(workspace_id, expected_version, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.get("/api/priorities", response_model=list[models.Priority])
def list_priorities(request: Request):
    return store(request).list_priorities(False)


@app.post("/api/priorities", response_model=models.Priority, status_code=201)
def create_priority(input: models.PriorityCreate, request: Request):
    try:
        return store(request).create_priority(input.name, input.color, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.patch("/api/priorities/{priority_id}", response_model=models.Priority)
def update_priority(priority_id: str, input: models.PriorityUpdate, request: Request):
    try:
        return store(request).update_priority(priority_id, input.expected_version, input.name, input.color, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.delete("/api/priorities/{priority_id}", status_code=204)
def delete_priority(priority_id: str, expected_version: int, request: Request):
    try:
        store(request).soft_delete_priority(priority_id, expected_version, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.post("/api/sessions/{session_id}/end", response_model=models.WorkSession)
def end_work(session_id: str, request: Request):
    try:
        return store(request).end_work(session_id, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.get("/api/settings/supabase")
def get_supabase_settings():
    settings = load_settings()
    return {
        "configured": bool(settings.get("url") and settings.get("key")),
        "url": settings.get("url", ""),
        "key_set": bool(settings.get("key")),
    }


@app.put("/api/settings/supabase")
def update_supabase_settings(input: models.SupabaseSettingsInput):
    try:
        existing = load_settings()
        key = existing.get("key", "") if input.api_key == "__KEEP_EXISTING__" else input.api_key.strip()
        settings = {"url": input.url.strip().rstrip("/"), "key": key}
        if settings["url"] or settings["key"]:
            validate_settings(settings)
        save_settings(settings["url"], settings["key"])
        return {
            "configured": bool(settings["url"] and settings["key"]),
            "url": settings["url"],
            "key_set": bool(settings["key"]),
        }
    except SyncError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error


@app.post("/api/settings/supabase/test")
def test_supabase_connection():
    try:
        return SupabaseClient(load_settings()).test_connection()
    except SyncError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error


@app.post("/api/sync")
def sync(request: Request):
    try:
        result = sync_store(store(request), load_settings())
        return {**result, "synced_at": models.utc_now_iso()}
    except SyncError as error:
        raise HTTPException(status_code=400, detail=str(error)) from error
    except Exception as error:
        raise HTTPException(status_code=500, detail=f"同步失败: {error}") from error


@app.get("/")
def index():
    return FileResponse(Path(__file__).resolve().parent / "static" / "index.html")


app.mount("/static", StaticFiles(directory=Path(__file__).resolve().parent / "static"), name="static")
