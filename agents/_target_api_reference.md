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

## 强制 runtime 协议（Milvus target — v2.2 新增，违反 = pipeline REJECT）

> 仅 `contract.target == "milvus"` 时强制；其它 target 暂用上文 `safe_request`，等 runtime 扩展。
> **milvus v2.6.19 实测根因**：26 脚本 0 confirmed。10/10 boundary 用 `/entities/create` 建集合（应 `/collections/create`）→ 全 404；多个脚本 `if status not in (400,422)` 把 setup 失败的 404 误判为 contract 违规。本协议把路径翻译 + verdict 逻辑从 agent 自由度里拿走。

### 核心 4 条（违反任意 = pipeline REJECT）

1. **必须**通过 runtime 拿请求函数，禁止自写 `safe_request`、禁止字面量路径：
   ```python
   import os, sys
   _sd = os.environ.get("TESTVDB_SCRIPTS_DIR") or os.path.join(
       os.path.dirname(os.path.abspath(__file__)), "..", "scripts")
   sys.path.insert(0, _sd)
   from runtime import get_runtime
   rt = get_runtime()  # 按 TESTVDB_TARGET 分发；milvus → runtime.milvus
   ```
2. **所有 HTTP 调用走 `rt.request(method, path_key, body)`**——`path_key` 必须是 `rt.PATHS` 的 key（全量见下文"PATHS 全量"）。**字面量路径**如 `/entities/create` 出现在源码 = REJECT。
3. **禁止任何 status-based if 判定**——`if status not in (400, 422)`、`if status != 404`、`if status == 200`、`if len(data) == 0` 全部禁止。**round 1 实战教训**：milvus REST v2 用 HTTP 200 + body `code` 表达错误（drop 后 describe 返回 `code:100` 而非 HTTP 404），所有 status 比较都会误判；空 `data:[]` 不一定是缺陷。必须按场景选 helper：
   - 应被拒绝（非法参数 / delete 后再访问 / 资源不存在场景）：`v = rt.expect_rejected(status, raw, setup_ok=ok)`
   - 应被接受（合法输入）：`v = rt.judge_200(status, raw, setup_ok=ok)`
   - 应返回 ≥ N 条记录（search / query）：`v = rt.expect_records(status, raw, expected_min=N, setup_ok=ok)`
   - 旧 alias `rt.judge_4xx` 仍可用（= expect_rejected），新脚本推荐 expect_rejected 命名清晰

   **helper 选择决策树（round 2 实战教训 — 必读）**：

   | 测试的输入是合法还是非法？ | 期望 milvus 行为 | 用哪个 helper |
   |---|---|---|
   | **非法输入**（超上限 / 低于下限 / 类型错 / 不存在资源） | 应被**拒绝** | `expect_rejected` |
   | **合法输入**且关心是否接受 | 应被**接受** | `judge_200` |
   | **合法输入**且关心返回多少条 | 应返回 ≥ N 条 | `expect_records` |

   关键区分：测 `limit/offset` 边界时——
   - `limit=0` / `offset+limit > 16384`（**非法**）→ `expect_rejected`（milvus 应拒绝）
   - `limit=16384`（**合法上限**）→ `judge_200` 或 `expect_records`（应被接受/返回记录）
   - **不要**对非法查询用 `expect_records`（milvus 拒绝非法查询是 NO_DEFECT，但 expect_records 会判 SCRIPT_ERROR）
4. 脚本末尾必须 `print(f"VERDICT: {v}")`，v ∈ {DEFECT_FOUND, NO_DEFECT, SCRIPT_ERROR}，并 `sys.exit(0 if v=="NO_DEFECT" else 1 if v=="DEFECT_FOUND" else 2)`。

### 三种用法模式

**模式 A — 默认 setup 便捷组合（boundary / semantic 绝大多数脚本）**：
```python
COLL = "boundary_test_001"
ok, err = rt.setup_default(COLL, 128)  # create + index + load 一体
if not ok:
    print(f"VERDICT: SCRIPT_ERROR — setup: {err}"); sys.exit(2)
try:
    status, raw = rt.request("POST", "search",
        {"collectionName": COLL, "data": [[0.1]*128], "limit": 0})
    print(f"Status: {status}\nRaw: {raw}")
    print("VERDICT:", rt.judge_4xx(status, raw, setup_ok=ok))
finally:
    rt.drop_collection(COLL)
```

**模式 B — 测 setup 本身边界（boundary 专属：dimension=0 / metricType=非法 等应被 `create_collection` 拒绝）**：
```python
# setup_default 会因 setup 失败 SCRIPT_ERROR 退出，故直接原子 request
status, raw = rt.request("POST", "create_collection", {
    "collectionName": "t", "dimension": 0, "metricType": "L2",
    "idType": "Int64", "autoID": True, "vectorFieldType": "FloatVector"})
print("VERDICT:", rt.judge_4xx(status, raw, setup_ok=True))
```

**模式 C — attack-state 自由组合（索引/load 期间并发等时序场景）**：
```python
COLL = "state_test_001"
rt.request("POST", "create_collection", {<同 setup_default 的 create_collection payload>})  # 不走 setup_default
async_idx = threading.Thread(target=lambda: rt.request("POST", "create_index", {...}))
async_idx.start()
# ← index 进行中触发 search/insert/delete
async_idx.join()
rt.drop_collection(COLL)
```

### PATHS 全量（milvus）

`create_collection` / `describe_collection` / `drop_collection` / `load_collection` / `release_collection` / `create_index` / `insert_points` / `upsert_points` / `search` / `query` / `delete`

### 与既有 safe_request 的关系

- milvus target：**禁止**再用 `safe_request`，全部走 `rt.request`。`rt.request` 内部调用同一 HTTP 包装（三元组返回）。
- 其它 target：继续用 `safe_request`，runtime 扩展后切换。
- `BASE_URL` / `AUTH_HEADER` 仍从同名 env var 取（runtime 内部已读，agent 不再自取）。

---

## 强制 runtime 协议（Qdrant target — v2.3 新增）

> 仅 `contract.target == "qdrant"` 时强制。**与 milvus 的关键差异**：qdrant 用标准 HTTP 4xx 表达错误（不像 milvus 的 HTTP 200+body code），所以 judge 走 `_common` generic 版（按 HTTP status 判），不解析 body code。

### 核心 4 条（同 milvus）

1. `from runtime import get_runtime; rt = get_runtime()`（`TESTVDB_TARGET=qdrant`）
2. `rt.request(method, path_key, body, path_params=...)`，**禁止字面量路径**。返回值是**二元组** `(status, raw_text)`——`status, raw = rt.request(...)`（fullrun#4 实测教训：三元组解包 `s, body, raw = rt.request(...)` 会 ValueError；要解析 body 自己 `json.loads(raw)`）
3. **禁止任何 status-based if**——按场景选 helper（同 milvus 决策树）：
   - 应被拒绝：`rt.expect_rejected(status, raw, setup_ok=ok)`
   - 应被接受：`rt.judge_200(status, raw, setup_ok=ok)`
   - 应返回 ≥ N 条：`rt.expect_records(status, raw, expected_min=N, setup_ok=ok)`
4. 末尾 `print(f"VERDICT: {v}")` + 按 v 退出

### Qdrant 特定差异（vs milvus）

**PATHS 是模板含 `{name}`**——qdrant RESTful 风格，collection name 在 URL path 里：
```python
# ❌ 错（字面量路径）
safe_request("PUT", f"/collections/{COLL}/points", ...)
# ✅ 对（path_key + path_params）
rt.request("PUT", "upsert_points", {"points": [...]}, path_params={"name": COLL})
```

**setup_default 单步**（无 index/load 阶段，比 milvus 简单）：
```python
ok, err = rt.setup_default(COLL, dim=128, metric="Cosine")  # PUT /collections/{name} 含 vectors config
```

**距离 metric 命名**：qdrant 用 `Cosine` / `Euclidean` / `Dot`（不是 milvus 的 `L2`）。

### PATHS 全量（qdrant）

`create_collection` / `describe_collection` / `drop_collection` / `list_collections` / `create_index` / `upsert_points` / `delete_points` / `search` / `query` / `count`

除 `list_collections` 外都是 `/collections/{name}/...` 模板，必须传 `path_params={"name": COLL}`。

---

## 强制 runtime 协议（Weaviate target — v2.4 新增）

> 仅 `contract.target == "weaviate"` 时强制。同 qdrant：标准 HTTP 4xx，generic judge 不解析 body code。差异：path 用 `/v1/...` 前缀 + class name 大写 + GraphQL search 风格。

### 核心 5 条（4 条同 milvus/qdrant + 第 5 条 weaviate 专属）

1. `from runtime import get_runtime; rt = get_runtime()`（`TESTVDB_TARGET=weaviate`）
2. `rt.request(method, path_key, body, path_params=...)`，**禁止字面量路径**。返回值是**二元组** `(status, raw_text)`——`status, raw = rt.request(...)`（fullrun#4 实测教训：三元组解包 `s, body, raw = rt.request(...)` 会 ValueError；要解析 body 自己 `json.loads(raw)`）
3. **禁止任何 status-based if**（verdict 判定场景）
4. 末尾 `print(f"VERDICT: {v}")` + 按 v 退出
5. **schema 类边界攻击（vectorIndexConfig / invertedIndexConfig / replicationConfig 字段非法值）必须用 `rt.judge_schema_attack(...)`，禁止用 `expect_rejected`**（详见下方"Weaviate 特定差异 · schema 类边界判定"）

### Weaviate 特定差异

**PATHS 含两类 path_params**：`{name}`（class name，schema 路径）+ `{id}`（object uuid）
```python
# schema 类
rt.request("DELETE", "drop_schema", path_params={"name": "Article"})
# object 类
rt.request("GET", "get_object", path_params={"id": "abc-123"})
# 无 param 类（list_schema / graphql / create_object / batch_objects）
rt.request("POST", "graphql", {"query": "{ Get { Article { ... } } }"})
```

**setup_default 单步**（POST /v1/schema body 含 class + vectorIndexConfig）：
```python
ok, err = rt.setup_default("Article", dim=128, metric="cosine")  # class name 大写开头
```

**距离 metric 命名**：weaviate 用 lowercase `cosine` / `l2-squared` / `dot` / `manhattan`（不是 qdrant 的 `Cosine` 也不是 milvus 的 `L2`）。

**search 走 GraphQL**：weaviate 主搜索接口是 `/v1/graphql`（POST body 含 GraphQL query 字符串），不是 REST path。`expect_records` 已支持 GraphQL 响应嵌套 `{"data":{"Get":{"<Class>":[...]}}}`。

**已存在响应 422**（不是 409）：weaviate 创建重复 class 返回 422，setup_default 已兼容。

**schema 类边界判定（核心第 5 条 — round 3 实战教训）**：weaviate 对 schema 非法字段有三态行为，**禁止用 `expect_rejected` 只看 status=200 判 Type1**：
- 持久化原值（如 `vectorCacheMaxObjects=-1`）→ 真 Type1_IllegalSuccess（bug）
- silent-drop 字段（agent 字段放错位置也算，如 `cleanupIntervalSeconds` 放在 `vectorIndexConfig` 下）→ weaviate 设计行为，**非 bug**
- silent normalize（如 `replicationConfig.factor=0`→`1`）→ Type2 bug 信号

用 `rt.judge_schema_attack(status, raw, class_name, attack_path, attack_value, setup_ok=ok)`：
- 内部 `describe_schema` 回读 + 字段路径比对，自动区分三态
- `attack_path` = 字段路径 list（如 `["vectorIndexConfig", "vectorCacheMaxObjects"]`）
- `attack_value` = 攻击 payload 里的非法值（用于回读比对）
- silent-drop → `NO_DEFECT`（避免 false positive）；persist → `DEFECT_FOUND`

```python
# ✅ 正确：schema 类边界用 judge_schema_attack
status, raw = rt.request("POST", "create_schema", {
    "class": CLS, "vectorIndexType": "hnsw",
    "vectorIndexConfig": {"distance": "cosine", "vectorCacheMaxObjects": -1}})
v = rt.judge_schema_attack(status, raw, CLS,
    ["vectorIndexConfig", "vectorCacheMaxObjects"], -1, setup_ok=True)

# ❌ 错误：只看 status=200 就判 DEFECT_FOUND（silent-drop 会误判 Type1，25% false positive）
# v = rt.expect_rejected(status, raw, setup_ok=True)
```

非 schema 类边界（object / batch_objects / graphql）仍用通用 helper（`expect_rejected` / `judge_200` / `expect_records`）。

### PATHS 全量（weaviate）

`create_schema` / `list_schema` / `describe_schema`（`{name}`）/ `drop_schema`（`{name}`）/ `add_property`（`{name}`）/ `create_object` / `batch_objects` / `get_object`（`{id}`）/ `delete_object`（`{id}`）/ `graphql`

---

## DB 特定 API 选择指南（v2.2 新增 — Chroma SDK 教训）

**核心规则：根据 `contract.target` 选择正确的 API 接入方式，不可一律用 REST。**

| target | API 方式 | 原因 |
|--------|---------|------|
| **chroma** | **chromadb SDK (`chromadb.HttpClient`)** | Chroma 是 SDK-first；v1 REST API 已废弃（返回 405）；`raw_knowledge.json` 明确记载 "Chroma is primarily a Python SDK-based vector database"。连接代码: `client = chromadb.HttpClient(host='localhost', port=8000)` |
| **milvus** | REST API v2 (`/v2/vectordb/`) | Milvus 同时支持 REST v2 + gRPC；REST v2 更稳定。仅在动态 schema 操作时用 pymilvus SDK |
| **qdrant** | REST API (`requests`) | 标准 REST API，端点路径从 contract 取 |
| **weaviate** | REST API (`requests`) | 标准 REST API，搜索用 GraphQL |
| **pgvector** | psycopg2 SQL | PostgreSQL 扩展，SQL 访问 |
| **meilisearch** | REST API (`requests`) | 标准 REST API |

### Milvus REST v2 path 翻译规则（v2.2.2 — 2026-07-04 milvus mine 教训）

**契约 `api_endpoints[].path` 用 `+` 分隔逻辑资源与动作**，REST URL 用 `/`。safe_request 调用前必须翻译：

| contract path | REST URL（safe_request 第二参数） | 用途 |
|---------------|--------------------------------|------|
| `collections+create` | `/collections/create` | **建集合**（唯一正确路径） |
| `collections+describe` | `/collections/describe` | 查集合 schema |
| `collections+load` | `/collections/load` | 加载到内存 |
| `collections+release` | `/collections/release` | 释放 |
| `collections+drop` | `/collections/drop` | 删除 |
| `collections+get_stats` | `/collections/get_stats` | 行数统计 |
| `entities+insert` | `/entities/insert` | **插数据** |
| `entities+upsert` | `/entities/upsert` | upsert 数据 |
| `entities+search` | `/entities/search` | 向量搜索 |
| `entities+query` | `/entities/query` | 标量过滤查询 |
| `entities+delete` | `/entities/delete` | 删除数据 |
| `indexes+create` | `/indexes/create` | 建索引 |

safe_request 内部已 `url = f"{BASE_URL}/v2/vectordb{path}"`，所以传 `/collections/create` 即可（不要再拼 `/v2/vectordb`）。

⛔ **Anti-pattern（2026-07-04 milvus v2.6.19 实测 bug，致 100% boundary 脚本 404）**：
- ❌ `safe_request("POST", "/entities/create", payload)` —— **发明路径**。`entities` 是数据操作（insert/search/query/delete），**不是集合创建**。Milvus REST v2 无此端点 → 404 page not found。
- ❌ 凭记忆/类比写路径（看到 `entities+insert` 就猜 `entities+create`）—— contract 无此 path 即不存在。
- ✅ `safe_request("POST", "/collections/create", payload)` —— 建集合唯一正确路径。
- ✅ 不确定时：`py -c "import json; c=json.load(open('structured_contract.json')); [print(ep['path']) for ep in c['api_endpoints']]"` 列全契约路径，从中提取并按 `+ → /` 翻译。

**setup collection create payload 必填字段**（缺则 `code:1100 dimension is not defined`，级联致后续全 `collection not found`）：
`collectionName` / `dimension`（向量维度，Int）/ `metricType`（L2/IP/COSINE）/ `idType`（Int64/Varchar）/ `autoID`（bool）/ `vectorFieldType`（FloatVector/BinaryVector/SparseFloatVector）。

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
