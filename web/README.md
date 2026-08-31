# CardHannis WebUI

基于 FastAPI 的本地 Web 原型，用于快速调整功能。

## 启动

```bash
cd web
python3 -m venv .venv
source .venv/bin/activate
python3 -m pip install -r requirements.txt
python3 -m uvicorn app.main:app --reload --port 8321
```

打开：

```text
http://127.0.0.1:8321
```

API 文档：

```text
http://127.0.0.1:8321/docs
```

数据库文件默认位于：

```text
~/Library/Application Support/CardHannis/cardhannis.sqlite3
```

也可以设置 `CARDHANNIS_DB` 指定数据库路径。

## Supabase 同步

1. 在 Supabase SQL Editor 执行仓库根目录的 `supabase-schema.sql`。
2. 启动 WebUI 后打开右上角「设置」，填写 Supabase Project URL 和 publishable/anon key。
3. 保存并测试连接，然后点击「立即同步」。

WebUI 会把本地 SQLite 与 Supabase 的 `tasks`、`task_blocks`、`work_sessions` 三张表按 `updated_at` 合并。配置保存在本机应用数据目录的 `supabase.json`，不会提交到仓库。

当前同步方案适合个人/受信任的单项目使用；不要把 `service_role` 密钥填入 WebUI。多人或敏感数据场景应在 Supabase 增加 Auth、按用户隔离的 RLS policy 后再使用。
