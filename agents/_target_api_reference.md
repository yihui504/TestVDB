# 目标 API 参考（契约驱动 — 通用原则）

> 共享参考。攻击 Agent 必须**契约驱动**，禁止硬编码任何 DB 的端口/路径/语法/数据字段。
> ⛔ 不要写 per-DB 的 if/else 分支或写死表——那会把"硬编码 qdrant"换成"硬编码 4 个 DB"，版本变化时会过时并误导，且新增 DB 时会崩溃。

## 核心原则

1. **唯一真理源 = `structured_contract.json`**。从契约读取一切 DB 特定信息：
   - `target` 字段 → 当前 DB（weaviate / qdrant / milvus / pgvector / meilisearch / chroma）
   - `api_endpoints` → 端点路径（method + path + category + parameters + source_url）
   - `data_types` → 数据结构（字段命名、向量格式，如 weaviate 的 `properties`/`Class`/`vector`）
   - `constraints` / `assertions` → 待测约束与预期行为
2. **禁止硬编码 DB 特定值**：不写死端口（6333/8080）、不写死路径（`/collections/x/points`）、不写死数据字段（`payload`）、不写死过滤语法（`must`/`match`）、不写死响应键（`result`）。这些一律从契约推导或用占位符。
3. **示例代码用占位符**：路径写成 `<path from contract for X>`，并注释"从 `contract.api_endpoints` 读取；请求体/响应解析依据 `contract.target` 与 `contract.data_types` 推导"。
4. **BASE_URL 从环境变量**：`TESTVDB_DB_URL`（由 docker-executor 设置正确端口），未设置则 `VERDICT: SCRIPT_ERROR` 退出。**禁止任何默认端口**。
5. **响应解析通用化**：先 `print(raw_text)`，以 HTTP `status_code` 判定缺陷为主；响应体解析作为辅助，按 `contract.target` 动态选择键名，不要假设固定结构。
6. **target 来源 = 契约**：若脚本需要 target 变量，从 `structured_contract.json` 的 `target` 字段读取（**不要**用 `os.environ.get("TESTVDB_TARGET", ...)` 带默认值——默认值会假设错误 DB）。

## 为何不写 per-DB 语法表
不同 DB 版本的端点路径/请求体语法会变化；写死表会过时、会误导、新增 DB 时 `else: raise` 会让脚本崩溃。契约已包含 `target` + `api_endpoints` + `data_types`，足够 LLM 据此推导出当前 target 的正确语法。

## safe_request 权威定义（三 attack agent 共用）

所有攻击脚本的 HTTP 调用**必须**用此包装器。返回三元组 `(status_code, body_or_None, raw_text)`。
三个 attack agent 的「输出格式」section 引用本定义，不再各自重写。

模块级变量来源：
- `BASE_URL = os.environ.get("TESTVDB_DB_URL")` —— 由 docker-executor 设置正确端口；**无默认端口**，缺失则打印 `VERDICT: SCRIPT_ERROR` 退出。
- `AUTH_HEADER = os.environ.get("TESTVDB_AUTH_HEADER", "")` —— 可选鉴权头。

```python
import requests, json, sys, os

BASE_URL = os.environ.get("TESTVDB_DB_URL")
if not BASE_URL:
    print("VERDICT: SCRIPT_ERROR — TESTVDB_DB_URL not set (see agents/_target_api_reference.md)")
    sys.exit(2)
AUTH_HEADER = os.environ.get("TESTVDB_AUTH_HEADER", "")

def safe_request(method, path, **kwargs):
    """Resilient HTTP wrapper. Returns (status_code, body_or_None, raw_text).
    连接失败: 打印 REQUEST_ERROR, 返回 (0, None, "")。
    JSON 解析失败: 打印 JSON_DECODE_ERROR, 返回 (status, None, text)。"""
    url = f"{BASE_URL}{path}"
    headers = kwargs.pop("headers", {"Content-Type": "application/json"})
    if AUTH_HEADER:
        headers["Authorization"] = AUTH_HEADER
    try:
        resp = requests.request(method, url, headers=headers, timeout=30, **kwargs)
        status = resp.status_code
        text = resp.text
        try:
            body = resp.json() if text else {}
        except (json.JSONDecodeError, ValueError):
            print(f"JSON_DECODE_ERROR: {text[:200]}")
            return status, None, text
        return status, body, text
    except requests.exceptions.RequestException as e:
        print(f"REQUEST_ERROR: {e}")
        return 0, None, ""
```

判定以 HTTP `status` 为主 + `print(raw)`；响应体解析按 `contract.target` 动态选键，不假设固定结构。

## DB 特定 API 选择指南（v2.2 新增 — Chroma SDK 教训）

**核心规则：根据 `contract.target` 选择正确的 API 接入方式，不可一律用 REST。**

| target | API 方式 | 原因 |
|--------|---------|------|
| **chroma** | **chromadb SDK (`chromadb.HttpClient`)** | Chroma 是 SDK-first；v1 REST API 已废弃（返回 405）；`raw_knowledge.md` 明确记载 "Chroma is primarily a Python SDK-based vector database"。连接代码: `client = chromadb.HttpClient(host='localhost', port=8000)` |
| **milvus** | REST API v2 (`/v2/vectordb/`) | Milvus 同时支持 REST v2 + gRPC；REST v2 更稳定。仅在动态 schema 操作时用 pymilvus SDK |
| **qdrant** | REST API (`requests`) | 标准 REST API，端点路径从 contract 取 |
| **weaviate** | REST API (`requests`) | 标准 REST API，搜索用 GraphQL |
| **pgvector** | psycopg2 SQL | PostgreSQL 扩展，SQL 访问 |
| **meilisearch** | REST API (`requests`) | 标准 REST API |

**Chroma 专用代码模板**（覆盖 `safe_request`——Chroma 不使用原始 HTTP）：
```python
import os, sys, json
import chromadb
from chromadb.config import Settings

BASE_URL = os.environ.get("TESTVDB_DB_URL", "http://localhost:8000")
# 从 BASE_URL 解析 host/port
# chromadb.HttpClient(host='localhost', port=8000, settings=Settings(anonymized_telemetry=False))

client = chromadb.HttpClient(
    host=BASE_URL.split("://")[1].split(":")[0] if "://" in BASE_URL else BASE_URL.split(":")[0],
    port=int(BASE_URL.split(":")[-1]) if ":" in BASE_URL.split("://")[-1] else 8000,
    settings=Settings(anonymized_telemetry=False)
)
```

**Chromadb SDK 常用 API 映射**（替代 REST safe_request）：
- `GET /collections` → `client.list_collections()`
- `POST /collections` → `client.create_collection(name=..., metadata=...)` 或 `client.get_or_create_collection(name=...)`
- `DELETE /collections/{name}` → `client.delete_collection(name)`
- `POST /collections/{name}/add` → `collection.add(ids=..., embeddings=..., documents=..., metadatas=...)`
- `POST /collections/{name}/query` → `collection.query(query_embeddings=..., n_results=...)`

## 脚本 Cleanup 强制规范（v2.2 新增 — delete_collection NotFoundError 教训）

**⛔ 所有脚本的 teardown/cleanup 阶段必须遵循此规范。违反 = SCRIPT_ERROR。**

### 规则

1. **每个 `delete_collection` / `delete` / `drop` 操作必须包裹在 `try/except` 中**，捕获对应的 NotFound 异常
2. **Cleanup 失败不得导致脚本退出码非零**——主逻辑已执行完毕，cleanup 是 best-effort
3. **先检查资源是否存在再删除**——避免无意义的异常

### Chroma 示例

```python
# ✅ 正确的 cleanup 模式
def cleanup():
    try:
        client.delete_collection(COLLECTION_NAME)
    except chromadb.errors.NotFoundError:
        pass  # 集合不存在或已被删除，cleanup 目标已达成
    except Exception as e:
        print(f"Cleanup warning: {e}")  # 记录但不崩溃

# 主逻辑完成后调用
# ... test logic ...
cleanup()  # 在脚本末尾，best-effort
```

### REST DB 示例（Qdrant/Weaviate/Milvus）

```python
def cleanup():
    status, _, raw = safe_request("DELETE", f"/collections/{COLLECTION_NAME}")
    if status not in (200, 204, 404):
        print(f"Cleanup warning: DELETE returned {status}: {raw[:200]}")

cleanup()
```

### 禁止的 Cleanup 反模式

```python
# ❌ 直接调用 delete_collection 无异常处理
client.delete_collection(name)  # NotFoundError → 脚本崩溃

# ❌ 在脚本开头（setup 前）调用 cleanup，但资源尚未创建
client.delete_collection(COLLECTION_NAME)  # 尚未 create → NotFoundError → 崩溃
```

## 参考样板
`agents/attack-boundary.md` 已采用此契约驱动模式（占位符 + 从契约读取，0 个 if/else TARGET 分支）。`attack-state.md` 与 `attack-semantic.md` 应遵循同一模式。
