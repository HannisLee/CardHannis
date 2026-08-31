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
