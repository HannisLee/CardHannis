from pathlib import Path

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from . import models
from .store import StoreError, TaskStore

def normalize_task(task: dict) -> dict:
    task["is_blocked"] = bool(task.get("is_blocked"))
    return task

DATABASE_PATH = Path.home() / "Library" / "Application Support" / "CardHannis" / "cardhannis.sqlite3"

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


@app.post("/api/sessions/{session_id}/end", response_model=models.WorkSession)
def end_work(session_id: str, request: Request):
    try:
        return store(request).end_work(session_id, models.utc_now_iso())
    except StoreError as error:
        raise fail(error) from error


@app.get("/")
def index():
    return FileResponse(Path(__file__).resolve().parent / "static" / "index.html")


app.mount("/static", StaticFiles(directory=Path(__file__).resolve().parent / "static"), name="static")
