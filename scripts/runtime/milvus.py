"""Milvus REST v2 runtime — PATHS 常量 + setup 便捷函数 + 原子 request。

agent 通过 runtime.get_runtime() 拿到本模块，不直接接触字面量路径。
路径常量固定在此文件——改 milvus REST base 改一处而非 65 处。
"""
from __future__ import annotations

import os
import time

try:
    from ._common import req  # 包内 import（推荐入口）
except ImportError:
    # 直接 `py scripts/runtime/milvus.py` 跑 self-check 时无 parent package
    from _common import req


def _body_code(raw: str) -> int:
    """解析 milvus REST v2 body code。0=成功；非 0=业务错误；非 JSON/无 code=-1。

    milvus REST v2 用 {code:0, data:...} 封装——HTTP 永远 200，
    错误用 body code 表达（如 1100=invalid parameter）。
    """
    try:
        import json
        b = json.loads(raw) if raw else {}
        return int(b.get("code", 0)) if isinstance(b, dict) else 0
    except (json.JSONDecodeError, ValueError, TypeError):
        return -1


def judge_4xx(status: int, raw: str, setup_ok: bool) -> str:
    """milvus 版本——HTTP 200 + body code != 0 也算"被拒绝"（覆盖 REST v2 封装）。

    - setup_ok=False / 5xx / 0 / 404 / 非响应 → SCRIPT_ERROR
    - 契约要求拒绝 + milvus 真拒绝了（4xx 或 200+code!=0）→ NO_DEFECT
    - 契约要求拒绝 + milvus 接受了（200+code==0）→ DEFECT_FOUND
    """
    if not setup_ok or status == 0 or status == 404 or 500 <= status <= 599:
        return "SCRIPT_ERROR"
    bc = _body_code(raw)
    if bc == -1:
        return "SCRIPT_ERROR"  # 响应非 JSON，无法判
    rejected = status in (400, 422) or (status == 200 and bc != 0)
    accepted = status == 200 and bc == 0
    if rejected:
        return "NO_DEFECT"
    if accepted:
        return "DEFECT_FOUND"
    return "SCRIPT_ERROR"


def judge_200(status: int, raw: str, setup_ok: bool) -> str:
    """milvus 版本——合法输入应被接受：HTTP 200 + body code==0 才算真接受。

    - setup_ok=False / 5xx / 0 / 404 / 非 JSON → SCRIPT_ERROR
    - 200 + code==0 → NO_DEFECT
    - 否则（含 4xx、200+code!=0）→ DEFECT_FOUND（合法输入被错误拒绝）
    """
    if not setup_ok or status == 0 or status == 404 or 500 <= status <= 599:
        return "SCRIPT_ERROR"
    bc = _body_code(raw)
    if bc == -1:
        return "SCRIPT_ERROR"
    if status == 200 and bc == 0:
        return "NO_DEFECT"
    return "DEFECT_FOUND"


def _data(raw: str):
    """解析 milvus body 的 data 字段（list 或 dict）。失败返回 None。"""
    try:
        import json
        b = json.loads(raw) if raw else {}
        return b.get("data") if isinstance(b, dict) else None
    except (json.JSONDecodeError, ValueError, TypeError):
        return None


def expect_records(status: int, raw: str, expected_min: int = 1,
                   setup_ok: bool = True) -> str:
    """合法查询应返回 ≥ expected_min 条记录。

    round 1 实战教训：脚本断言 "应返回 N 条" 时常见误判——把空 data:[] 当 DEFECT。
    本 helper 解析 milvus data 字段长度，统一判定：
    - setup_ok=False / 5xx / 0 / 非 JSON → SCRIPT_ERROR
    - milvus 拒绝（200+code!=0 或 4xx）→ SCRIPT_ERROR（查询本身失败，不是记录数问题）
    - 200+code==0 + len(data) ≥ expected_min → NO_DEFECT
    - 200+code==0 + len(data) < expected_min → DEFECT_FOUND（应返回但没返回）
    """
    if not setup_ok or status == 0 or 500 <= status <= 599:
        return "SCRIPT_ERROR"
    bc = _body_code(raw)
    if bc == -1:
        return "SCRIPT_ERROR"
    if status in (400, 422) or (status == 200 and bc != 0):
        return "SCRIPT_ERROR"  # 查询被拒绝，不混淆为记录数缺陷
    data = _data(raw)
    if data is None:
        # data 字段缺失（如 insert/upsert 响应）— 不是记录数场景，回退 judge_200 语义
        return "NO_DEFECT" if bc == 0 else "DEFECT_FOUND"
    n = len(data) if isinstance(data, list) else (1 if data else 0)
    return "NO_DEFECT" if n >= expected_min else "DEFECT_FOUND"


def judge_schema_attack(status: int, raw: str, collection_name: str,
                        attack_path: list, attack_value,
                        setup_ok: bool = True) -> str:
    """milvus 版 schema 类边界攻击判定 — describe_collection 回读三态判定。

    跨 target 一致接口（同 weaviate/qdrant），milvus 适配：
      - body code 模式（HTTP 200 + code != 0 = 拒绝）
      - describe 返回 {code:0, data: {...}}，持久化字段在 data 下

    判定矩阵：
      - setup_ok=False / 0 / 5xx / 非响应 → SCRIPT_ERROR
      - 拒绝（4xx 或 200+code!=0）→ NO_DEFECT
      - 接受（200+code==0）+ describe 失败 → SCRIPT_ERROR
      - 接受 + 字段持久化（任意非 None 值）→ DEFECT_FOUND
      - 接受 + 字段被 silent-drop（None）→ NO_DEFECT
    """
    import json as _json
    if not setup_ok or status == 0 or status == 404 or 500 <= status <= 599:
        return "SCRIPT_ERROR"
    bc = _body_code(raw)
    if bc == -1 and status not in (400, 422):
        return "SCRIPT_ERROR"
    rejected = status in (400, 422) or (status == 200 and bc != 0)
    if rejected:
        return "NO_DEFECT"
    if status != 200 or bc != 0:
        return "SCRIPT_ERROR"
    ds, draw = request("POST", "describe_collection",
                       {"collectionName": collection_name})
    if ds != 200 or _body_code(draw) != 0:
        return "SCRIPT_ERROR"
    try:
        body = _json.loads(draw) if draw else {}
    except (_json.JSONDecodeError, ValueError, TypeError):
        return "SCRIPT_ERROR"
    persisted = body.get("data") if isinstance(body, dict) else None
    if not isinstance(persisted, dict):
        return "SCRIPT_ERROR"
    node = persisted
    for k in attack_path:
        if isinstance(node, dict):
            node = node.get(k)
        else:
            return "SCRIPT_ERROR"
    if node is None:
        return "NO_DEFECT"  # silent-drop
    return "DEFECT_FOUND"


def expect_rejected(status: int, raw: str, setup_ok: bool = True) -> str:
    """语义别名——应被 milvus 拒绝的场景。等价于 judge_4xx，命名更清晰。

    round 1 实战教训：脚本测 "delete 后 describe 应失败" 时，agent 写
    `if status != 404` 误判（milvus 用 HTTP 200+code:100 拒绝）。本 helper 强制走 body code。
    """
    return judge_4xx(status, raw, setup_ok=setup_ok)

# 路径常量——agent 永远不写字面量路径，只引用 PATHS[key]。
# ponytail: 手写常量；等 qdrant/weaviate 各写一份后，发现重复模式再抽 compile_paths。
PATHS = {
    # 标准命名（推荐）
    "create_collection":      "/collections/create",
    "describe_collection":    "/collections/describe",
    "drop_collection":        "/collections/drop",
    "load_collection":        "/collections/load",
    "release_collection":     "/collections/release",
    "create_index":           "/indexes/create",
    "insert_points":          "/entities/insert",
    "upsert_points":          "/entities/upsert",
    "search":                 "/entities/search",
    "query":                  "/entities/query",
    "delete":                 "/entities/delete",
    "get_points":             "/entities/get",
    # ponytail: alias 覆盖 agent 从 contract path 直译的命名（contract path 风格）
    "collections/create":     "/collections/create",
    "collections/describe":   "/collections/describe",
    "collections/drop":       "/collections/drop",
    "collections/load":       "/collections/load",
    "collections/get_stats":  "/collections/get_stats",
    "collections/release":    "/collections/release",
    "indexes/create":         "/indexes/create",
    "entities/insert":        "/entities/insert",
    "entities/upsert":        "/entities/upsert",
    "entities/search":        "/entities/search",
    "entities/query":         "/entities/query",
    "entities/delete":        "/entities/delete",
    "entities/get":           "/entities/get",
    "collections/has": "/collections/describe",
    "collections/get_load_state": "/collections/get_load_state",
    # ── Round 2 扩展：admin/partitions/aliases/import 端点（contract 全 77）──
    "databases+create": "/databases/create",
    "databases/create": "/databases/create",
    "databases+drop": "/databases/drop",
    "databases/drop": "/databases/drop",
    "databases+describe": "/databases/describe",
    "databases/describe": "/databases/describe",
    "roles+create": "/roles/create",
    "roles/create": "/roles/create",
    "roles+drop": "/roles/drop",
    "roles/drop": "/roles/drop",
    "roles+describe": "/roles/describe",
    "roles/describe": "/roles/describe",
    "roles+grant_privilege": "/roles/grant_privilege",
    "roles/grant_privilege": "/roles/grant_privilege",
    "roles+revoke_privilege": "/roles/revoke_privilege",
    "roles/revoke_privilege": "/roles/revoke_privilege",
    "roles+grant_privilege_v2": "/roles/grant_privilege_v2",
    "roles/grant_privilege_v2": "/roles/grant_privilege_v2",
    "roles+revoke_privilege_v2": "/roles/revoke_privilege_v2",
    "roles/revoke_privilege_v2": "/roles/revoke_privilege_v2",
    "users+create": "/users/create",
    "users/create": "/users/create",
    "users+drop": "/users/drop",
    "users/drop": "/users/drop",
    "users+describe": "/users/describe",
    "users/describe": "/users/describe",
    "users+grant_role": "/users/grant_role",
    "users/grant_role": "/users/grant_role",
    "users+revoke_role": "/users/revoke_role",
    "users/revoke_role": "/users/revoke_role",
    "privilege_groups+create": "/privilege_groups/create",
    "privilege_groups/create": "/privilege_groups/create",
    "privilege_groups+drop": "/privilege_groups/drop",
    "privilege_groups/drop": "/privilege_groups/drop",
    "privilege_groups+list": "/privilege_groups/list",
    "privilege_groups/list": "/privilege_groups/list",
    "privilege_groups+add_privileges": "/privilege_groups/add_privileges",
    "privilege_groups/add_privileges": "/privilege_groups/add_privileges",
    "privilege_groups+remove_privileges": "/privilege_groups/remove_privileges",
    "privilege_groups/remove_privileges": "/privilege_groups/remove_privileges",
    "resource_groups+create": "/resource_groups/create",
    "resource_groups/create": "/resource_groups/create",
    "resource_groups+drop": "/resource_groups/drop",
    "resource_groups/drop": "/resource_groups/drop",
    "resource_groups+describe": "/resource_groups/describe",
    "resource_groups/describe": "/resource_groups/describe",
    "partitions+create": "/partitions/create",
    "partitions/create": "/partitions/create",
    "partitions+drop": "/partitions/drop",
    "partitions/drop": "/partitions/drop",
    "partitions+has": "/partitions/has",
    "partitions/has": "/partitions/has",
    "partitions+list": "/partitions/list",
    "partitions/list": "/partitions/list",
    "partitions+load": "/partitions/load",
    "partitions/load": "/partitions/load",
    "partitions+release": "/partitions/release",
    "partitions/release": "/partitions/release",
    "partitions+get_stats": "/partitions/get_stats",
    "partitions/get_stats": "/partitions/get_stats",
    "aliases+create": "/aliases/create",
    "aliases/create": "/aliases/create",
    "aliases+drop": "/aliases/drop",
    "aliases/drop": "/aliases/drop",
    "aliases+describe": "/aliases/describe",
    "aliases/describe": "/aliases/describe",
    "aliases+list": "/aliases/list",
    "aliases/list": "/aliases/list",
    "aliases+alter": "/aliases/alter",
    "aliases/alter": "/aliases/alter",
    "collections+rename": "/collections/rename",
    "collections/rename": "/collections/rename",
    "collections+compact": "/collections/compact",
    "collections/compact": "/collections/compact",
    "collections+flush": "/collections/flush",
    "collections/flush": "/collections/flush",
    "collections+truncate": "/collections/truncate",
    "collections/truncate": "/collections/truncate",
    "collections+refresh_load": "/collections/refresh_load",
    "collections/refresh_load": "/collections/refresh_load",
    "collections+get_compaction_state": "/collections/get_compaction_state",
    "collections/get_compaction_state": "/collections/get_compaction_state",
    "collections+alter_properties": "/collections/alter_properties",
    "collections/alter_properties": "/collections/alter_properties",
    "collections+drop_properties": "/collections/drop_properties",
    "collections/drop_properties": "/collections/drop_properties",
    "collections+add_function": "/collections/add_function",
    "collections/add_function": "/collections/add_function",
    "collections+alter_function": "/collections/alter_function",
    "collections/alter_function": "/collections/alter_function",
    "collections+drop_function": "/collections/drop_function",
    "collections/drop_function": "/collections/drop_function",
    "collections+fields+add": "/collections/fields/add",
    "collections/fields/add": "/collections/fields/add",
    "collections+fields+alter_properties": "/collections/fields/alter_properties",
    "collections/fields/alter_properties": "/collections/fields/alter_properties",
    "indexes+describe": "/indexes/describe",
    "indexes/describe": "/indexes/describe",
    "indexes+drop": "/indexes/drop",
    "indexes/drop": "/indexes/drop",
    "indexes+list": "/indexes/list",
    "indexes/list": "/indexes/list",
    "indexes+alter_properties": "/indexes/alter_properties",
    "indexes/alter_properties": "/indexes/alter_properties",
    "indexes+drop_properties": "/indexes/drop_properties",
    "indexes/drop_properties": "/indexes/drop_properties",
    "entities+hybridsearch": "/entities/hybrid_search",
    "entities/hybridsearch": "/entities/hybrid_search",
    "jobs+import+create": "/jobs/import/create",
    "jobs/import/create": "/jobs/import/create",
    "jobs+import+describe": "/jobs/import/describe",
    "jobs/import/describe": "/jobs/import/describe",
    "jobs+import+list": "/jobs/import/list",
    "jobs/import/list": "/jobs/import/list",
    "jobs+external_collection+describe": "/jobs/external_collection/describe",
    "jobs/external_collection/describe": "/jobs/external_collection/describe",
    "jobs+external_collection+list": "/jobs/external_collection/list",
    "jobs/external_collection/list": "/jobs/external_collection/list",
    "jobs+external_collection+refresh": "/jobs/external_collection/refresh",
    "jobs/external_collection/refresh": "/jobs/external_collection/refresh",
    "common+run_analyzer": "/common/run_analyzer",
    "common/run_analyzer": "/common/run_analyzer",
    # ── 自动 +别名（collections/entities/indexes 等）──
    "collections+create": "/collections/create",
    "collections+describe": "/collections/describe",
    "collections+drop": "/collections/drop",
    "collections+load": "/collections/load",
    "collections+get_stats": "/collections/get_stats",
    "collections+release": "/collections/release",
    "indexes+create": "/indexes/create",
    "entities+insert": "/entities/insert",
    "entities+upsert": "/entities/upsert",
    "entities+search": "/entities/search",
    "entities+query": "/entities/query",
    "entities+delete": "/entities/delete",
    "entities+get": "/entities/get",
    "collections+has": "/collections/describe",
    "collections+get_load_state": "/collections/get_load_state",

}

_PREFIX = os.environ.get("TESTVDB_DB_URL", "").rstrip("/") + "/v2/vectordb"


def request(method: str, path_key: str, body: dict | None = None,
            timeout: int = 30) -> tuple[int, str]:
    """path_key 必须在 PATHS 里——字面量路径 = KeyError = pipeline reject。

    返回二元组 (status, raw_text)，agent 用 rt.judge_4xx/judge_200 判定。
    """
    if path_key not in PATHS:
        # 显式报错让 agent 看清是 path_key 错，不是网络错
        raise KeyError(
            f"path_key={path_key!r} not in milvus.PATHS; valid keys: {sorted(PATHS)}"
        )
    return req(_PREFIX, method, PATHS[path_key], body, timeout=timeout)



def parse_response(raw: str | bytes | None, *keys):
    """ponytail: 兼容 attack agent 调用 rt.parse_response(raw)。返回 dict 或按 keys 取嵌套值。"""
    import json as _json
    if raw is None: return None
    if isinstance(raw, (dict, list)): data = raw
    else:
        try: data = _json.loads(raw)
        except Exception: return None
    for k in keys:
        if isinstance(data, dict): data = data.get(k)
        else: return None
    return data

def setup_default(name: str, dim: int, metric: str = "L2",
                  wait: float | None = None, skip_load: bool = False) -> tuple[bool, str]:
    """便捷组合：create + index + load。boundary/semantic 默认用；attack-state 不强制。

    测 setup 本身边界的脚本（如 dimension=0 应被拒）不要用此函数——直接 request()。
    返回 (True, '') 或 (False, reason)；reason 含阶段名 + status + 截断 raw。
    """
    if wait is None:
        wait = float(os.environ.get("TESTVDB_LOAD_WAIT", "2"))

    s, raw = request("POST", "create_collection", {
        "collectionName": name, "dimension": dim, "metricType": metric,
        "idType": "Int64", "autoID": True, "vectorFieldType": "FloatVector",
    })
    if s not in (200, 409):  # 409 = 已存在，复用 OK
        return False, f"create {s}: {raw[:200]}"
    if s == 409:
        # ponytail: 已存在则 release 后 reload（idempotent setup）
        request("POST", "release_collection", {"collectionName": name})

    s, raw = request("POST", "create_index", {"collectionName": name, "indexParams": [{
        "fieldName": "vector", "indexName": "vector_idx",
        "indexType": "HNSW", "metricType": metric,
        "params": {"M": 16, "efConstruction": 256},
    }]})
    if s not in (200, 409):
        return False, f"index {s}: {raw[:200]}"

    if skip_load:


        return True, "skip_load"


    s, raw = request("POST", "load_collection", {"collectionName": name})
    if s != 200:
        return False, f"load {s}: {raw[:200]}"

    if wait > 0:
        time.sleep(wait)  # ponytail: milvus load 异步；慢机器调 TESTVDB_LOAD_WAIT
    return True, ""


def insert_points(name: str, points: list[dict]) -> tuple[bool, str]:
    s, raw = request("POST", "insert_points", {"collectionName": name, "data": points})
    return (s in (200, 201, 204), f"insert {s}: {raw[:200]}")


def drop_collection(name: str) -> None:
    """Cleanup — 永远 try/except，失败不抛（spec 已规定）。"""
    try:
        request("POST", "drop_collection", {"collectionName": name})
    except Exception:
        pass


def _self_check() -> None:
    """ponytail: 静态自检 milvus body code 判定 + PATHS 完整性（不连真实 DB）。"""
    # milvus REST v2 body code 模式
    OK200 = '{"code":0,"data":{}}'                       # milvus 真正成功
    ERR200 = '{"code":1100,"message":"invalid param"}'  # milvus 业务错误（HTTP 200）
    # judge_4xx 应拒绝场景
    assert judge_4xx(200, OK200, setup_ok=True) == "DEFECT_FOUND"   # 应拒绝但接受了
    assert judge_4xx(200, ERR200, setup_ok=True) == "NO_DEFECT"     # milvus 用 code 拒绝了 ✓
    assert judge_4xx(400, OK200, setup_ok=True) == "NO_DEFECT"      # HTTP 4xx 拒绝
    assert judge_4xx(422, OK200, setup_ok=True) == "NO_DEFECT"
    assert judge_4xx(404, "", setup_ok=True) == "SCRIPT_ERROR"      # ← 实测根因 ①
    assert judge_4xx(500, OK200, setup_ok=True) == "SCRIPT_ERROR"
    assert judge_4xx(0, "", setup_ok=True) == "SCRIPT_ERROR"
    assert judge_4xx(200, OK200, setup_ok=False) == "SCRIPT_ERROR"  # ← 实测根因 ③
    assert judge_4xx(200, "not json", setup_ok=True) == "SCRIPT_ERROR"
    # judge_200 应接受场景
    assert judge_200(200, OK200, setup_ok=True) == "NO_DEFECT"
    assert judge_200(200, ERR200, setup_ok=True) == "DEFECT_FOUND"  # 应接受但 milvus 拒绝
    assert judge_200(400, OK200, setup_ok=True) == "DEFECT_FOUND"
    assert judge_200(500, OK200, setup_ok=True) == "SCRIPT_ERROR"
    assert judge_200(200, OK200, setup_ok=False) == "SCRIPT_ERROR"
    # expect_records 应返回 N 条记录场景（round 1 实战：boundary_002/semantic_003/005/007 把空 data 当 DEFECT）
    DATA_N3 = '{"code":0,"data":[1,2,3]}'         # 3 条记录
    DATA_EMPTY = '{"code":0,"data":[]}'           # 0 条（应触发 DEFECT_FOUND）
    assert expect_records(200, DATA_N3, expected_min=3, setup_ok=True) == "NO_DEFECT"
    assert expect_records(200, DATA_N3, expected_min=5, setup_ok=True) == "DEFECT_FOUND"  # 应≥5 但只3
    assert expect_records(200, DATA_EMPTY, expected_min=1, setup_ok=True) == "DEFECT_FOUND"  # 空 data
    assert expect_records(200, ERR200, expected_min=1, setup_ok=True) == "SCRIPT_ERROR"  # 拒绝≠记录数
    assert expect_records(200, DATA_N3, setup_ok=False) == "SCRIPT_ERROR"
    # expect_rejected 语义别名 = judge_4xx
    assert expect_rejected(200, OK200, setup_ok=True) == "DEFECT_FOUND"
    assert expect_rejected(200, ERR200, setup_ok=True) == "NO_DEFECT"
    assert expect_rejected(200, OK200, setup_ok=False) == "SCRIPT_ERROR"
    # PATHS 关键 key 齐全
    must_have = ("create_collection", "drop_collection", "load_collection",
                 "create_index", "insert_points", "search")
    assert all(k in PATHS for k in must_have), f"missing: {set(must_have) - set(PATHS)}"
    # path_key 错误抛 KeyError（agent 拿不到字面量路径）
    try:
        request("POST", "nonexistent_key", {})
    except KeyError:
        pass
    else:
        raise AssertionError("request() should raise KeyError on bad path_key")
    print("milvus runtime self-check OK")


if __name__ == "__main__":
    _self_check()
