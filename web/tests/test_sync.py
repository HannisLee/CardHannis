import tempfile
import unittest
from pathlib import Path

import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.store import TaskStore
from app.sync import merge_records


class SyncMergeTests(unittest.TestCase):
    def test_newer_task_wins_and_deleted_records_are_retained(self):
        local = {"tasks": [{"id": "task-1", "title": "local", "updated_at": "2026-08-31T01:00:00.000Z"}], "task_blocks": [], "work_sessions": []}
        remote = {"tasks": [{"id": "task-1", "title": "remote", "updated_at": "2026-08-31T02:00:00.000Z", "deleted_at": None}], "task_blocks": [], "work_sessions": []}
        merged = merge_records(local, remote)
        self.assertEqual(merged["tasks"][0]["title"], "remote")

    def test_only_latest_active_session_is_left_open(self):
        local = {"tasks": [{"id": "task-1", "status": "in_progress", "updated_at": "2026-08-31T04:00:00.000Z"}], "task_blocks": [], "work_sessions": [{"id": "s1", "task_id": "task-1", "started_at": "2026-08-31T01:00:00.000Z", "ended_at": None, "created_at": "2026-08-31T01:00:00.000Z"}]}
        remote = {"tasks": [], "task_blocks": [], "work_sessions": [{"id": "s2", "task_id": "task-1", "started_at": "2026-08-31T02:00:00.000Z", "ended_at": None, "created_at": "2026-08-31T02:00:00.000Z"}]}
        merged = merge_records(local, remote)
        sessions = {row["id"]: row for row in merged["work_sessions"]}
        self.assertEqual(sessions["s1"]["ended_at"], "2026-08-31T02:00:00.000Z")
        self.assertIsNone(sessions["s2"]["ended_at"])

    def test_store_import_round_trips_records(self):
        with tempfile.TemporaryDirectory() as directory:
            store = TaskStore(Path(directory) / "cardhannis.sqlite3")
            records = {
                "tasks": [{"id": "task-1", "title": "同步任务", "notes": None, "review_notes": None, "estimated_active_minutes": 60, "created_at": "2026-08-31T01:00:00.000Z", "started_at": None, "completed_at": None, "status": "pending", "sort_order": 0, "created_device_id": "test", "updated_at": "2026-08-31T01:00:00.000Z", "deleted_at": None, "version": 1}],
                "task_blocks": [],
                "work_sessions": [],
            }
            store.upsert_sync_records(records)
            self.assertEqual(store.export_sync_records()["tasks"][0]["title"], "同步任务")


if __name__ == "__main__":
    unittest.main()
