# Target API reference (contract-driven — shared principles)

> Shared reference. Attack agents must be **contract-driven**; hardcoding any DB's ports/paths/syntax/data fields is forbidden.
> ⛔ Do not write per-DB if/else branches or hardcoded tables — that replaces "hardcoded qdrant" with "hardcoded 4 DBs", goes stale and misleads across version changes, and crashes when a new DB is added.

## Core principles

1. **The single source of truth = `structured_contract.json`**. Read every DB-specific piece of information from the contract:
   - the `target` field → current DB (weaviate / qdrant / milvus / pgvector / meilisearch / chroma)
   - `api_endpoints` → endpoint paths (method + path + category + parameters + source_url)
   - `data_types` → data structures (field naming, vector formats, e.g. weaviate's `properties`/`Class`/`vector`)
   - `constraints` / `assertions` → constraints under test and expected behavior
2. **Hardcoding DB-specific values is forbidden**: no hardcoded ports (6333/8080), paths (`/collections/x/points`), data fields (`payload`), filter syntax (`must`/`match`), or response keys (`result`). Derive all of these from the contract or use placeholders.
3. **Example code uses placeholders**: write paths as `<path from contract for X>`, commented "read from `contract.api_endpoints`; request-body/response parsing derived from `contract.target` and `contract.data_types`".
4. **BASE_URL from environment**: `TESTVDB_DB_URL` (docker-executor sets the correct port); if unset, exit with `VERDICT: SCRIPT_ERROR`. **Any default port is forbidden**.
5. **Generalized response parsing**: `print(raw_text)` first; defect adjudication keys primarily on the HTTP `status_code`; body parsing is auxiliary — select key names dynamically per `contract.target`, never assume a fixed structure.
6. **target comes from the contract**: if a script needs the target variable, read it from `structured_contract.json`'s `target` field (**do not** use `os.environ.get("TESTVDB_TARGET", ...)` with a default — a default assumes the wrong DB).

## Why no per-DB syntax tables
Endpoint paths / request-body syntax change across DB versions; hardcoded tables go stale, mislead, and `else: raise` crashes scripts when a new DB appears. The contract already carries `target` + `api_endpoints` + `data_types`, which is enough for the LLM to derive the correct syntax for the current target.

## Authoritative safe_request definition (shared by the three attack agents)

All HTTP calls in attack scripts **must** use this wrapper. It returns the triple `(status_code, body_or_None, raw_text)`.
The three attack agents' "output format" sections reference this definition instead of rewriting it.

Module-level variable sources:
- `BASE_URL = os.environ.get("TESTVDB_DB_URL")` — docker-executor sets the correct port; **no default port**; if missing, print `VERDICT: SCRIPT_ERROR` and exit.
- `AUTH_HEADER = os.environ.get("TESTVDB_AUTH_HEADER", "")` — optional auth header.

```python
import requests, json, sys, os

BASE_URL = os.environ.get("TESTVDB_DB_URL")
if not BASE_URL:
    print("VERDICT: SCRIPT_ERROR — TESTVDB_DB_URL not set (see agents/_target_api_reference.md)")
    sys.exit(2)
AUTH_HEADER = os.environ.get("TESTVDB_AUTH_HEADER", "")

def safe_request(method, path, **kwargs):
    """Resilient HTTP wrapper. Returns (status_code, body_or_None, raw_text).
    Connection failure: prints REQUEST_ERROR, returns (0, None, "").
    JSON parse failure: prints JSON_DECODE_ERROR, returns (status, None, text)."""
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

Adjudication keys primarily on the HTTP `status` + `print(raw)`; response-body parsing selects keys dynamically per `contract.target`, never assuming a fixed structure.

## Mandatory runtime protocol (Milvus target — added v2.2; violation = pipeline REJECT)

> Mandatory only when `contract.target == "milvus"`; other targets keep using `safe_request` above until the runtime extends.
> **milvus v2.6.19 measured root cause**: 26 scripts, 0 confirmed. 10/10 boundary scripts created collections at `/entities/create` (should be `/collections/create`) → all 404; several scripts used `if status not in (400,422)`, misjudging the setup-failure 404 as a contract violation. This protocol removes path translation + verdict logic from agent discretion.

### The 4 core rules (violating any = pipeline REJECT)

1. You **must** obtain the request function from the runtime; writing your own `safe_request` and literal paths are forbidden:
   ```python
   import os, sys
   _sd = os.environ.get("TESTVDB_SCRIPTS_DIR") or os.path.join(
       os.path.dirname(os.path.abspath(__file__)), "..", "scripts")
   sys.path.insert(0, _sd)
   from runtime import get_runtime
   rt = get_runtime()  # dispatches on TESTVDB_TARGET; milvus → runtime.milvus
   ```
2. **All HTTP calls go through `rt.request(method, path_key, body)`** — `path_key` must be a key of `rt.PATHS` (full list in "PATHS" below). **Literal paths** such as `/entities/create` appearing in source = REJECT.
3. **Any status-based if is forbidden** — `if status not in (400, 422)`, `if status != 404`, `if status == 200`, `if len(data) == 0` are all forbidden. **Round 1 field lesson**: milvus REST v2 expresses errors as HTTP 200 + body `code` (describe after drop returns `code:100`, not HTTP 404), so every status comparison misjudges; empty `data:[]` is not necessarily a defect. Choose the helper by scenario:
   - Should be rejected (illegal param / access after delete / nonexistent resource): `v = rt.expect_rejected(status, raw, setup_ok=ok)`
   - Should be accepted (legal input): `v = rt.judge_200(status, raw, setup_ok=ok)`
   - Should return ≥ N records (search / query): `v = rt.expect_records(status, raw, expected_min=N, setup_ok=ok)`
   - The legacy alias `rt.judge_4xx` still works (= expect_rejected); new scripts prefer expect_rejected for clarity

   **Helper decision tree (round 2 field lesson — must read)**:

   | Is the tested input legal or illegal? | Expected milvus behavior | Helper |
   |---|---|---|
   | **Illegal** (above max / below min / wrong type / nonexistent resource) | Should be **rejected** | `expect_rejected` |
   | **Legal**, and you care whether it is accepted | Should be **accepted** | `judge_200` |
   | **Legal**, and you care how many records return | Should return ≥ N | `expect_records` |

   Key distinction when testing `limit/offset` boundaries —
   - `limit=0` / `offset+limit > 16384` (**illegal**) → `expect_rejected` (milvus should reject)
   - `limit=16384` (**legal upper bound**) → `judge_200` or `expect_records` (should be accepted / return records)
   - **Do not** use `expect_records` for illegal queries (milvus rejecting an illegal query is NO_DEFECT, but expect_records would judge SCRIPT_ERROR)
4. Scripts must end with `print(f"VERDICT: {v}")`, v ∈ {DEFECT_FOUND, NO_DEFECT, SCRIPT_ERROR}, and `sys.exit(0 if v=="NO_DEFECT" else 1 if v=="DEFECT_FOUND" else 2)`.

### Three usage patterns

**Pattern A — default setup convenience combo (the vast majority of boundary / semantic scripts)**:
```python
COLL = "boundary_test_001"
ok, err = rt.setup_default(COLL, 128)  # create + index + load in one step
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

**Pattern B — testing the setup itself at its boundary (boundary-only: dimension=0 / metricType=illegal etc. should be rejected by `create_collection`)**:
```python
# setup_default would exit SCRIPT_ERROR on setup failure, so issue the atomic request directly
status, raw = rt.request("POST", "create_collection", {
    "collectionName": "t", "dimension": 0, "metricType": "L2",
    "idType": "Int64", "autoID": True, "vectorFieldType": "FloatVector"})
print("VERDICT:", rt.judge_4xx(status, raw, setup_ok=True))
```

**Pattern C — attack-state free combination (timing scenarios such as concurrency during index/load)**:
```python
COLL = "state_test_001"
rt.request("POST", "create_collection", {<same create_collection payload as setup_default>})  # bypasses setup_default
async_idx = threading.Thread(target=lambda: rt.request("POST", "create_index", {...}))
async_idx.start()
# ← fire search/insert/delete while the index build is in progress
async_idx.join()
rt.drop_collection(COLL)
```

### PATHS (milvus)

`create_collection` / `describe_collection` / `drop_collection` / `load_collection` / `release_collection` / `create_index` / `insert_points` / `upsert_points` / `search` / `query` / `delete`

### Relationship to the legacy safe_request

- milvus target: `safe_request` is **forbidden**; everything goes through `rt.request`. Internally `rt.request` uses the same HTTP wrapper (triple return).
- Other targets: keep using `safe_request`; switch when the runtime extends.
- `BASE_URL` / `AUTH_HEADER` still come from the same env var names (the runtime reads them internally; agents no longer fetch them).

---

## Mandatory runtime protocol (Qdrant target — added v2.3)

> Mandatory only when `contract.target == "qdrant"`. **Key difference vs milvus**: qdrant expresses errors with standard HTTP 4xx (unlike milvus's HTTP 200 + body code), so judging uses the `_common` generic version (HTTP-status based) without parsing body codes.

### The 4 core rules (same as milvus)

1. `from runtime import get_runtime; rt = get_runtime()` (`TESTVDB_TARGET=qdrant`)
2. `rt.request(method, path_key, body, path_params=...)`, **literal paths forbidden**. The return value is a **2-tuple** `(status, raw_text)` — `status, raw = rt.request(...)` (fullrun#4 measured lesson: triple unpacking `s, body, raw = rt.request(...)` raises ValueError; to parse the body, `json.loads(raw)` yourself)
3. **Any status-based if is forbidden** — choose the helper by scenario (same decision tree as milvus):
   - Should be rejected: `rt.expect_rejected(status, raw, setup_ok=ok)`
   - Should be accepted: `rt.judge_200(status, raw, setup_ok=ok)`
   - Should return ≥ N records: `rt.expect_records(status, raw, expected_min=N, setup_ok=ok)`
4. End with `print(f"VERDICT: {v}")` + exit by v

### Qdrant-specific differences (vs milvus)

**PATHS are templates containing `{name}`** — qdrant is RESTful-style; the collection name lives in the URL path:
```python
# ❌ wrong (literal path)
safe_request("PUT", f"/collections/{COLL}/points", ...)
# ✅ right (path_key + path_params)
rt.request("PUT", "upsert_points", {"points": [...]}, path_params={"name": COLL})
```

**setup_default is a single step** (no index/load phases; simpler than milvus):
```python
ok, err = rt.setup_default(COLL, dim=128, metric="Cosine")  # PUT /collections/{name} including vectors config
```

**Distance metric naming**: qdrant uses `Cosine` / `Euclidean` / `Dot` (not milvus's `L2`).

### PATHS (qdrant)

`create_collection` / `describe_collection` / `drop_collection` / `list_collections` / `create_index` / `upsert_points` / `delete_points` / `search` / `query` / `count`

Except `list_collections`, all are `/collections/{name}/...` templates and must pass `path_params={"name": COLL}`.

---

## Mandatory runtime protocol (Weaviate target — added v2.4)

> Mandatory only when `contract.target == "weaviate"`. Same as qdrant: standard HTTP 4xx, generic judge does not parse body codes. Differences: `/v1/...` path prefix + capitalized class names + GraphQL search style.

### The 5 core rules (4 same as milvus/qdrant + the 5th weaviate-specific)

1. `from runtime import get_runtime; rt = get_runtime()` (`TESTVDB_TARGET=weaviate`)
2. `rt.request(method, path_key, body, path_params=...)`, **literal paths forbidden**. The return value is a **2-tuple** `(status, raw_text)` — `status, raw = rt.request(...)` (fullrun#4 measured lesson: triple unpacking `s, body, raw = rt.request(...)` raises ValueError; to parse the body, `json.loads(raw)` yourself)
3. **Any status-based if is forbidden** (in verdict adjudication scenarios)
4. End with `print(f"VERDICT: {v}")` + exit by v
5. **Schema-class boundary attacks (illegal values of vectorIndexConfig / invertedIndexConfig / replicationConfig fields) must use `rt.judge_schema_attack(...)`; `expect_rejected` is forbidden** (see "Weaviate-specific differences · schema-class boundary adjudication" below)

### Weaviate-specific differences

**PATHS carry two kinds of path_params**: `{name}` (class name, schema paths) + `{id}` (object uuid)
```python
# schema class
rt.request("DELETE", "drop_schema", path_params={"name": "Article"})
# object class
rt.request("GET", "get_object", path_params={"id": "abc-123"})
# no-param class (list_schema / graphql / create_object / batch_objects)
rt.request("POST", "graphql", {"query": "{ Get { Article { ... } } }"})
```

**setup_default is a single step** (POST /v1/schema with a body containing class + vectorIndexConfig):
```python
ok, err = rt.setup_default("Article", dim=128, metric="cosine")  # class name starts capitalized
```

**Distance metric naming**: weaviate uses lowercase `cosine` / `l2-squared` / `dot` / `manhattan` (not qdrant's `Cosine`, not milvus's `L2`).

**Search goes through GraphQL**: weaviate's primary search interface is `/v1/graphql` (POST body contains the GraphQL query string), not a REST path. `expect_records` already supports the GraphQL response nesting `{"data":{"Get":{"<Class>":[...]}}}`.

**Already-exists returns 422** (not 409): creating a duplicate weaviate class returns 422; setup_default already handles it.

**Schema-class boundary adjudication (core rule 5 — round 3 field lesson)**: weaviate has three-state behavior for illegal schema fields; **using `expect_rejected` and judging Type1 from status=200 alone is forbidden**:
- Value persisted as-is (e.g. `vectorCacheMaxObjects=-1`) → genuine Type1_IllegalSuccess (bug)
- Field silently dropped (agent misplacing a field also counts, e.g. `cleanupIntervalSeconds` under `vectorIndexConfig`) → weaviate designed behavior, **not a bug**
- Silently normalized (e.g. `replicationConfig.factor=0`→`1`) → Type2 bug signal

Use `rt.judge_schema_attack(status, raw, class_name, attack_path, attack_value, setup_ok=ok)`:
- internally re-reads via `describe_schema` + compares the field path, automatically distinguishing the three states
- `attack_path` = field-path list (e.g. `["vectorIndexConfig", "vectorCacheMaxObjects"]`)
- `attack_value` = the illegal value in the attack payload (used for read-back comparison)
- silent-drop → `NO_DEFECT` (avoids false positives); persisted → `DEFECT_FOUND`

```python
# ✅ right: schema-class boundaries use judge_schema_attack
status, raw = rt.request("POST", "create_schema", {
    "class": CLS, "vectorIndexType": "hnsw",
    "vectorIndexConfig": {"distance": "cosine", "vectorCacheMaxObjects": -1}})
v = rt.judge_schema_attack(status, raw, CLS,
    ["vectorIndexConfig", "vectorCacheMaxObjects"], -1, setup_ok=True)

# ❌ wrong: judging DEFECT_FOUND from status=200 alone (silent-drop misjudges Type1; 25% false positive)
# v = rt.expect_rejected(status, raw, setup_ok=True)
```

Non-schema boundaries (object / batch_objects / graphql) still use the general helpers (`expect_rejected` / `judge_200` / `expect_records`).

### PATHS (weaviate)

`create_schema` / `list_schema` / `describe_schema` (`{name}`) / `drop_schema` (`{name}`) / `add_property` (`{name}`) / `create_object` / `batch_objects` / `get_object` (`{id}`) / `delete_object` (`{id}`) / `graphql`

---

## DB-specific API selection guide (added v2.2 — the Chroma SDK lesson)

**Core rule: choose the correct API access method per `contract.target`; never default to REST for everything.**

| target | API method | Reason |
|--------|---------|------|
| **chroma** | **chromadb SDK (`chromadb.HttpClient`)** | Chroma is SDK-first; the v1 REST API is deprecated (returns 405); `raw_knowledge.json` explicitly records "Chroma is primarily a Python SDK-based vector database". Connection code: `client = chromadb.HttpClient(host='localhost', port=8000)` |
| **milvus** | REST API v2 (`/v2/vectordb/`) | Milvus supports both REST v2 + gRPC; REST v2 is more stable. Use the pymilvus SDK only for dynamic-schema operations |
| **qdrant** | REST API (`requests`) | Standard REST API; endpoint paths come from the contract |
| **weaviate** | REST API (`requests`) | Standard REST API; search uses GraphQL |
| **pgvector** | psycopg2 SQL | PostgreSQL extension; SQL access |
| **meilisearch** | REST API (`requests`) | Standard REST API |

### Milvus REST v2 path translation rules (v2.2.2 — 2026-07-04 milvus mine lesson)

**Contract `api_endpoints[].path` joins logical resource and action with `+`**; the REST URL uses `/`. Translate before calling safe_request:

| contract path | REST URL (safe_request 2nd argument) | Purpose |
|---------------|--------------------------------|------|
| `collections+create` | `/collections/create` | **Create collection** (the only correct path) |
| `collections+describe` | `/collections/describe` | Read collection schema |
| `collections+load` | `/collections/load` | Load into memory |
| `collections+release` | `/collections/release` | Release |
| `collections+drop` | `/collections/drop` | Delete |
| `collections+get_stats` | `/collections/get_stats` | Row count stats |
| `entities+insert` | `/entities/insert` | **Insert data** |
| `entities+upsert` | `/entities/upsert` | Upsert data |
| `entities+search` | `/entities/search` | Vector search |
| `entities+query` | `/entities/query` | Scalar-filtered query |
| `entities+delete` | `/entities/delete` | Delete data |
| `indexes+create` | `/indexes/create` | Create index |

safe_request internally already does `url = f"{BASE_URL}/v2/vectordb{path}"`, so pass `/collections/create` (do not concatenate `/v2/vectordb` again).

⛔ **Anti-patterns (measured 2026-07-04 on milvus v2.6.19; caused 100% of boundary scripts to 404)**:
- ❌ `safe_request("POST", "/entities/create", payload)` — **an invented path**. `entities` is data manipulation (insert/search/query/delete), **not collection creation**. Milvus REST v2 has no such endpoint → 404 page not found.
- ❌ Writing paths from memory/analogy (seeing `entities+insert` and guessing `entities+create`) — if the contract has no such path, it does not exist.
- ✅ `safe_request("POST", "/collections/create", payload)` — the only correct create-collection path.
- ✅ When unsure: `py -c "import json; c=json.load(open('structured_contract.json')); [print(ep['path']) for ep in c['api_endpoints']]"` lists all contract paths; extract from there and translate `+ → /`.

**Required fields of the setup collection-create payload** (missing → `code:1100 dimension is not defined`, cascading into all-subsequent `collection not found`):
`collectionName` / `dimension` (vector dimension, Int) / `metricType` (L2/IP/COSINE) / `idType` (Int64/Varchar) / `autoID` (bool) / `vectorFieldType` (FloatVector/BinaryVector/SparseFloatVector).

**Chroma-specific code template** (replaces `safe_request` — Chroma does not use raw HTTP):
```python
import os, sys, json
import chromadb
from chromadb.config import Settings

BASE_URL = os.environ.get("TESTVDB_DB_URL", "http://localhost:8000")
# parse host/port from BASE_URL
# chromadb.HttpClient(host='localhost', port=8000, settings=Settings(anonymized_telemetry=False))

client = chromadb.HttpClient(
    host=BASE_URL.split("://")[1].split(":")[0] if "://" in BASE_URL else BASE_URL.split(":")[0],
    port=int(BASE_URL.split(":")[-1]) if ":" in BASE_URL.split("://")[-1] else 8000,
    settings=Settings(anonymized_telemetry=False)
)
```

**Common chromadb SDK API mapping** (replaces REST safe_request):
- `GET /collections` → `client.list_collections()`
- `POST /collections` → `client.create_collection(name=..., metadata=...)` or `client.get_or_create_collection(name=...)`
- `DELETE /collections/{name}` → `client.delete_collection(name)`
- `POST /collections/{name}/add` → `collection.add(ids=..., embeddings=..., documents=..., metadatas=...)`
- `POST /collections/{name}/query` → `collection.query(query_embeddings=..., n_results=...)`

## Mandatory script-cleanup spec (added v2.2 — the delete_collection NotFoundError lesson)

**⛔ Every script's teardown/cleanup phase must follow this spec. Violation = SCRIPT_ERROR.**

### Rules

1. **Every `delete_collection` / `delete` / `drop` operation must be wrapped in `try/except`**, catching the corresponding NotFound exception
2. **Cleanup failure must not make the script exit nonzero** — the main logic has already completed; cleanup is best-effort
3. **Check existence before deleting** — avoids pointless exceptions

### Chroma example

```python
# ✅ correct cleanup pattern
def cleanup():
    try:
        client.delete_collection(COLLECTION_NAME)
    except chromadb.errors.NotFoundError:
        pass  # collection doesn't exist or was already deleted; the cleanup goal is met
    except Exception as e:
        print(f"Cleanup warning: {e}")  # log but don't crash

# call after the main logic completes
# ... test logic ...
cleanup()  # at the end of the script, best-effort
```

### REST DB example (Qdrant/Weaviate/Milvus)

```python
def cleanup():
    status, _, raw = safe_request("DELETE", f"/collections/{COLLECTION_NAME}")
    if status not in (200, 204, 404):
        print(f"Cleanup warning: DELETE returned {status}: {raw[:200]}")

cleanup()
```

### Forbidden cleanup anti-patterns

```python
# ❌ calling delete_collection directly with no exception handling
client.delete_collection(name)  # NotFoundError → script crashes

# ❌ calling cleanup at the start of the script (before setup), when the resource doesn't exist yet
client.delete_collection(COLLECTION_NAME)  # not yet created → NotFoundError → crash
```

## Reference exemplar
`agents/attack-boundary.md` already follows this contract-driven pattern (placeholders + reading from the contract, 0 if/else TARGET branches). `attack-state.md` and `attack-semantic.md` should follow the same pattern.
