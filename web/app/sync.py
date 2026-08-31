import json
import os
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

from .store import TaskStore, StoreError

DEFAULT_SETTINGS_PATH = Path.home() / "Library" / "Application Support" / "CardHannis" / "supabase.json"
TABLES = ("tasks", "task_blocks", "work_sessions")


class SyncError(Exception):
    pass


def settings_path() -> Path:
    return Path(os.environ.get("CARDHANNIS_SETTINGS", DEFAULT_SETTINGS_PATH))


def load_settings() -> dict[str, str]:
    path = settings_path()
    if not path.exists():
        return {"url": "", "key": ""}
    try:
        value = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise SyncError(f"读取 Supabase 设置失败: {error}") from error
    return {"url": str(value.get("url", "")), "key": str(value.get("key", ""))}


def save_settings(url: str, key: str) -> dict[str, str]:
    value = {"url": url.strip().rstrip("/"), "key": key.strip()}
    path = settings_path()
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
        path.chmod(0o600)
    except OSError as error:
        raise SyncError(f"保存 Supabase 设置失败: {error}") from error
    return value


def validate_settings(settings: dict[str, str]) -> None:
    url = settings.get("url", "").strip().rstrip("/")
    key = settings.get("key", "").strip()
    if not url or not key:
        raise SyncError("请先填写 Supabase Project URL 和 publishable/anon key")
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme not in {"https", "http"} or not parsed.netloc:
        raise SyncError("Supabase Project URL 格式不正确")
    if parsed.scheme != "https" and parsed.hostname not in {"localhost", "127.0.0.1"}:
        raise SyncError("Supabase 远程地址必须使用 HTTPS")


class SupabaseClient:
    def __init__(self, settings: dict[str, str]):
        validate_settings(settings)
        self.base_url = settings["url"].strip().rstrip("/")
        self.key = settings["key"].strip()
        self.rest_url = f"{self.base_url}/rest/v1"

    def _request(self, method: str, path: str, body: Any = None, headers: dict[str, str] | None = None) -> Any:
        request_headers = {
            "apikey": self.key,
            "Authorization": f"Bearer {self.key}",
            "Accept": "application/json",
        }
        if body is not None:
            request_headers["Content-Type"] = "application/json"
        if headers:
            request_headers.update(headers)
        request = urllib.request.Request(
            f"{self.rest_url}/{path.lstrip('/')}",
            data=json.dumps(body, ensure_ascii=False).encode() if body is not None else None,
            headers=request_headers,
            method=method,
        )
        try:
            with urllib.request.urlopen(request, timeout=20) as response:
                payload = response.read()
                if not payload:
                    return None
                return json.loads(payload)
        except urllib.error.HTTPError as error:
            detail = error.read().decode(errors="replace")
            raise SyncError(f"Supabase 返回 HTTP {error.code}: {detail[:500]}") from error
        except (urllib.error.URLError, TimeoutError) as error:
            raise SyncError(f"无法连接 Supabase: {error}") from error
        except json.JSONDecodeError as error:
            raise SyncError("Supabase 返回了无法解析的响应") from error

    def test_connection(self) -> dict[str, Any]:
        rows = self._request("GET", "tasks?select=id&limit=1")
        return {"ok": True, "task_count_sample": len(rows or [])}

    def fetch_all(self) -> dict[str, list[dict[str, Any]]]:
        result: dict[str, list[dict[str, Any]]] = {}
        for table in TABLES:
            rows: list[dict[str, Any]] = []
            offset = 0
            while True:
                query = urllib.parse.urlencode({"select": "*", "limit": 1000, "offset": offset})
                page = self._request("GET", f"{table}?{query}") or []
                if not isinstance(page, list):
                    raise SyncError(f"Supabase 表 {table} 返回格式不正确")
                rows.extend(page)
                if len(page) < 1000:
                    break
                offset += len(page)
            result[table] = rows
        return result

    def upsert_all(self, records: dict[str, list[dict[str, Any]]]) -> None:
        for table in TABLES:
            rows = records.get(table, [])
            for start in range(0, len(rows), 500):
                chunk = rows[start : start + 500]
                if not chunk:
                    continue
                self._request(
                    "POST",
                    table,
                    chunk,
                    {"Prefer": "resolution=merge-duplicates,return=minimal"},
                )


def _stamp(table: str, row: dict[str, Any]) -> str:
    if table in {"tasks", "task_blocks"}:
        return str(row.get("updated_at") or row.get("created_at") or "")
    return str(row.get("ended_at") or row.get("started_at") or row.get("created_at") or "")


def _merge_table(table: str, local: list[dict[str, Any]], remote: list[dict[str, Any]]) -> list[dict[str, Any]]:
    merged = {str(row["id"]): dict(row) for row in local}
    for incoming in remote:
        row_id = str(incoming.get("id", ""))
        if not row_id:
            continue
        current = merged.get(row_id)
        if current is None or _stamp(table, incoming) > _stamp(table, current):
            merged[row_id] = dict(incoming)
    return list(merged.values())


def _enforce_active_constraints(records: dict[str, list[dict[str, Any]]]) -> dict[str, list[dict[str, Any]]]:
    """Resolve independently-created active blocks/sessions before SQLite/Postgres enforce their unique indexes."""
    tasks = {str(task["id"]): task for task in records["tasks"]}
    for table in ("task_blocks", "work_sessions"):
        active_by_task: dict[str, list[dict[str, Any]]] = {}
        for row in records[table]:
            if row.get("ended_at") is None:
                active_by_task.setdefault(str(row["task_id"]), []).append(row)
        for task_id, active_rows in active_by_task.items():
            task = tasks.get(task_id)
            if task and task.get("status") == "completed":
                ended_at = task.get("completed_at") or task.get("updated_at")
                for row in active_rows:
                    row["ended_at"] = ended_at
                    if table == "task_blocks":
                        row["updated_at"] = max(str(row.get("updated_at") or ""), str(ended_at or ""))
                continue
            if len(active_rows) <= 1:
                continue
            active_rows.sort(key=lambda row: (str(row.get("started_at") or ""), str(row.get("id") or "")))
            winner = active_rows[-1]
            for row in active_rows[:-1]:
                row["ended_at"] = winner.get("started_at")
                if table == "task_blocks":
                    row["updated_at"] = max(str(row.get("updated_at") or ""), str(winner.get("started_at") or ""))
    return records


def merge_records(local: dict[str, list[dict[str, Any]]], remote: dict[str, list[dict[str, Any]]]) -> dict[str, list[dict[str, Any]]]:
    merged = {table: _merge_table(table, local.get(table, []), remote.get(table, [])) for table in TABLES}
    return _enforce_active_constraints(merged)


def sync_store(store: TaskStore, settings: dict[str, str]) -> dict[str, Any]:
    client = SupabaseClient(settings)
    local_before = store.export_sync_records()
    remote_before = client.fetch_all()
    merged = merge_records(local_before, remote_before)
    store.upsert_sync_records(merged)
    client.upsert_all(merged)
    return {
        "ok": True,
        "tables": {table: len(merged[table]) for table in TABLES},
        "local_before": {table: len(local_before[table]) for table in TABLES},
        "remote_before": {table: len(remote_before[table]) for table in TABLES},
        "message": "本地与 Supabase 已按最后更新时间合并",
    }
