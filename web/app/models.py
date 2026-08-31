from datetime import datetime, timezone
from typing import Literal, Optional

from pydantic import BaseModel, ConfigDict, Field

TaskStatus = Literal["pending", "in_progress", "completed"]


def utc_now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f")[:-3] + "Z"


class TaskBase(BaseModel):
    title: str = Field(min_length=1, max_length=200)
    notes: Optional[str] = None
    review_notes: Optional[str] = None
    estimated_active_minutes: Optional[int] = Field(default=None, ge=0)
    sort_order: int = 0


class TaskCreate(TaskBase):
    created_device_id: str = Field(default="web-ui", min_length=1)


class TaskUpdate(BaseModel):
    title: str = Field(min_length=1, max_length=200)
    notes: Optional[str] = None
    review_notes: Optional[str] = None
    estimated_active_minutes: Optional[int] = Field(default=None, ge=0)
    sort_order: int = 0
    expected_version: int


class Task(TaskBase):
    model_config = ConfigDict(populate_by_name=True)

    id: str
    created_at: str
    started_at: Optional[str] = None
    completed_at: Optional[str] = None
    status: TaskStatus
    created_device_id: str
    updated_at: str
    deleted_at: Optional[str] = None
    version: int
    is_blocked: bool


class BlockCreate(BaseModel):
    reason: str = Field(min_length=1, max_length=300)
    note: Optional[str] = None


class TaskBlock(BaseModel):
    id: str
    task_id: str
    started_at: str
    ended_at: Optional[str] = None
    reason: str
    note: Optional[str] = None
    created_at: str
    updated_at: str
    version: int
    deleted_at: Optional[str] = None


class WorkSession(BaseModel):
    id: str
    task_id: str
    started_at: str
    ended_at: Optional[str] = None
    note: Optional[str] = None
    created_at: str


class StartWorkInput(BaseModel):
    note: Optional[str] = None
