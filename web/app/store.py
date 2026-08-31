import sqlite3
import threading
from pathlib import Path
from typing import Any, Iterable, Optional

from .models import TaskCreate, TaskUpdate

INIT_SQL = (Path(__file__).resolve().parent.parent.parent / "core" / "migrations" / "0001_initial.sql").read_text()

TASK_COLUMNS = (
    "id,title,notes,review_notes,estimated_active_minutes,created_at,started_at,completed_at,status,"
    "sort_order,created_device_id,updated_at,deleted_at,version,"
    "EXISTS (SELECT 1 FROM task_blocks b WHERE b.task_id = t.id AND b.ended_at IS NULL AND b.deleted_at IS NULL) AS is_blocked"
)

BLOCK_COLUMNS = "id,task_id,started_at,ended_at,reason,note,created_at,updated_at,version,deleted_at"
SESSION_COLUMNS = "id,task_id,started_at,ended_at,note,created_at"


class StoreError(Exception):
    pass


class TaskStore:
    """SQLite 访问层。与 Rust 核心共享同一迁移文件，以便后续继续迁移到 Rust/桌面端。"""

    def __init__(self, database_path: Path):
        database_path.parent.mkdir(parents=True, exist_ok=True)
        self._lock = threading.Lock()
        self._connection = sqlite3.connect(str(database_path), check_same_thread=False)
        self._connection.row_factory = sqlite3.Row
        self._connection.execute("PRAGMA foreign_keys = ON")
        self._connection.executescript(INIT_SQL)
        self._connection.commit()

    def _execute(self, sql: str, params: Iterable[Any] = ()) -> sqlite3.Cursor:
        with self._lock:
            cursor = self._connection.execute(sql, params)
            self._connection.commit()
            return cursor

    def _fetchone(self, sql: str, params: Iterable[Any] = ()) -> Optional[sqlite3.Row]:
        with self._lock:
            return self._connection.execute(sql, params).fetchone()

    def _fetchall(self, sql: str, params: Iterable[Any] = ()) -> list[sqlite3.Row]:
        with self._lock:
            return self._connection.execute(sql, params).fetchall()

    def create_task(self, input: TaskCreate, created_at: str) -> dict[str, Any]:
        task_id = new_id()
        self._execute(
            """
            INSERT INTO tasks (id, title, notes, review_notes, estimated_active_minutes, sort_order, created_device_id, created_at, updated_at, version)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, 1)
            """,
            (
                task_id, input.title.strip(), input.notes, input.review_notes,
                input.estimated_active_minutes, input.sort_order, input.created_device_id.strip(), created_at,
            ),
        )
        task = self.get_task(task_id)
        if task is None:
            raise StoreError("创建任务失败")
        return task

    def list_tasks(self, include_deleted: bool = False) -> list[dict[str, Any]]:
        where = "" if include_deleted else "WHERE t.deleted_at IS NULL"
        rows = self._fetchall(f"SELECT {TASK_COLUMNS} FROM tasks t {where} ORDER BY t.sort_order, t.created_at")
        tasks = []
        for row in rows:
            task = dict(row)
            if task["is_blocked"]:
                block = self._fetchone("SELECT id, version FROM task_blocks WHERE task_id = ?1 AND ended_at IS NULL AND deleted_at IS NULL ORDER BY started_at DESC LIMIT 1", (task["id"],))
                if block:
                    task["active_block_id"] = block["id"]
                    task["active_block_version"] = block["version"]
            tasks.append(task)
        return tasks

    def get_task(self, task_id: str) -> Optional[dict[str, Any]]:
        row = self._fetchone(f"SELECT {TASK_COLUMNS} FROM tasks t WHERE t.id = ?1", (task_id,))
        task = dict(row) if row else None
        if task is not None and task["is_blocked"]:
            block = self._fetchone("SELECT id, version FROM task_blocks WHERE task_id = ?1 AND ended_at IS NULL AND deleted_at IS NULL ORDER BY started_at DESC LIMIT 1", (task_id,))
            if block:
                task["active_block_id"] = block["id"]
                task["active_block_version"] = block["version"]
        return task

    def update_task(self, task_id: str, input: TaskUpdate, updated_at: str) -> dict[str, Any]:
        cursor = self._execute(
            """
            UPDATE tasks
            SET title = ?1, notes = ?2, review_notes = ?3, estimated_active_minutes = ?4, sort_order = ?5, updated_at = ?6, version = version + 1
            WHERE id = ?7 AND version = ?8 AND deleted_at IS NULL
            """,
            (input.title.strip(), input.notes, input.review_notes, input.estimated_active_minutes, input.sort_order, updated_at, task_id, input.expected_version),
        )
        if cursor.rowcount == 0:
            raise not_found_or_conflict("tasks", task_id)
        return self.get_task(task_id) or {}

    def set_status(self, task_id: str, expected_version: int, status: str, updated_at: str) -> dict[str, Any]:
        task = self.get_task(task_id)
        if task is None:
            raise StoreError("任务不存在")
        if task["deleted_at"]:
            raise StoreError("已删除任务不能修改")
        if task["version"] != expected_version:
            raise StoreError("版本冲突")
        if task["status"] == "completed" and status != "completed":
            raise StoreError("已完成任务不能重新打开")
        if status == "completed" and task["is_blocked"]:
            raise StoreError("请先解除任务阻塞")
        started_at = task["started_at"]
        completed_at = None
        if status == "completed":
            completed_at = updated_at
        elif status != "pending":
            started_at = started_at or updated_at
        else:
            started_at = None
        self._execute(
            "UPDATE tasks SET status = ?1, started_at = ?2, completed_at = ?3, updated_at = ?4, version = version + 1 WHERE id = ?5 AND version = ?6 AND deleted_at IS NULL",
            (status, started_at, completed_at, updated_at, task_id, expected_version),
        )
        return self.get_task(task_id) or {}

    def soft_delete_task(self, task_id: str, expected_version: int, deleted_at: str) -> None:
        cursor = self._execute(
            "UPDATE tasks SET deleted_at = ?1, updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND deleted_at IS NULL",
            (deleted_at, task_id, expected_version),
        )
        if cursor.rowcount == 0:
            raise not_found_or_conflict("tasks", task_id)

    def start_work(self, task_id: str, note: Optional[str], started_at: str) -> dict[str, Any]:
        task = self.get_task(task_id)
        if task is None:
            raise StoreError("任务不存在")
        if task["deleted_at"] or task["status"] == "completed":
            raise StoreError("该任务不能开始工作")
        if task["is_blocked"]:
            raise StoreError("该任务当前被阻塞")
        session_id = new_id()
        try:
            self._execute(
                "INSERT INTO work_sessions (id, task_id, started_at, note, created_at) VALUES (?1, ?2, ?3, ?4, ?3)",
                (session_id, task_id, started_at, note),
            )
        except sqlite3.IntegrityError as error:
            raise StoreError("任务已有进行中的工作会话") from error
        if task["status"] == "pending":
            self._execute("UPDATE tasks SET status = 'in_progress', started_at = ?1, updated_at = ?1, version = version + 1 WHERE id = ?2", (started_at, task_id))
        return self.get_session(session_id) or {}

    def end_work(self, session_id: str, ended_at: str) -> dict[str, Any]:
        cursor = self._execute("UPDATE work_sessions SET ended_at = ?1 WHERE id = ?2 AND ended_at IS NULL", (ended_at, session_id))
        if cursor.rowcount == 0:
            session = self.get_session(session_id)
            if session is None:
                raise StoreError("工作会话不存在")
            raise StoreError("工作会话已经结束")
        return self.get_session(session_id) or {}

    def start_block(self, task_id: str, reason: str, note: Optional[str], started_at: str) -> dict[str, Any]:
        task = self.get_task(task_id)
        if task is None:
            raise StoreError("任务不存在")
        if task["deleted_at"] or task["status"] == "completed":
            raise StoreError("该任务不能被阻塞")
        if task["is_blocked"]:
            raise StoreError("任务已有进行中的阻塞")
        self._execute("UPDATE work_sessions SET ended_at = ?1 WHERE task_id = ?2 AND ended_at IS NULL", (started_at, task_id))
        block_id = new_id()
        try:
            self._execute(
                "INSERT INTO task_blocks (id, task_id, started_at, reason, note, created_at, updated_at, version) VALUES (?1, ?2, ?3, ?4, ?5, ?3, ?3, 1)",
                (block_id, task_id, started_at, reason.strip(), note),
            )
        except sqlite3.IntegrityError as error:
            raise StoreError("任务已有进行中的阻塞") from error
        return self.get_block(block_id) or {}

    def end_block(self, block_id: str, expected_version: int, ended_at: str) -> dict[str, Any]:
        cursor = self._execute(
            "UPDATE task_blocks SET ended_at = ?1, updated_at = ?1, version = version + 1 WHERE id = ?2 AND version = ?3 AND ended_at IS NULL AND deleted_at IS NULL",
            (ended_at, block_id, expected_version),
        )
        if cursor.rowcount == 0:
            block = self.get_block(block_id)
            if block is None:
                raise StoreError("阻塞记录不存在")
            if block["version"] != expected_version:
                raise StoreError("版本冲突")
            raise StoreError("阻塞已经解除")
        return self.get_block(block_id) or {}

    def list_blocks(self, task_id: str, include_deleted: bool = False) -> list[dict[str, Any]]:
        where = "" if include_deleted else "AND deleted_at IS NULL"
        rows = self._fetchall(f"SELECT {BLOCK_COLUMNS} FROM task_blocks WHERE task_id = ?1 {where} ORDER BY started_at", (task_id,))
        return [dict(row) for row in rows]

    def list_sessions(self, task_id: str) -> list[dict[str, Any]]:
        rows = self._fetchall(f"SELECT {SESSION_COLUMNS} FROM work_sessions WHERE task_id = ?1 ORDER BY started_at", (task_id,))
        return [dict(row) for row in rows]

    def get_block(self, block_id: str) -> Optional[dict[str, Any]]:
        row = self._fetchone(f"SELECT {BLOCK_COLUMNS} FROM task_blocks WHERE id = ?1", (block_id,))
        return dict(row) if row else None

    def get_session(self, session_id: str) -> Optional[dict[str, Any]]:
        row = self._fetchone(f"SELECT {SESSION_COLUMNS} FROM work_sessions WHERE id = ?1", (session_id,))
        return dict(row) if row else None


def new_id() -> str:
    import uuid

    return str(uuid.uuid4())


def not_found_or_conflict(table: str, row_id: str) -> StoreError:
    return StoreError("版本冲突" if row_id else f"{table} 不存在")
