"""Qdrant REST runtime — PATHS 模板 + path_params + setup_default。

qdrant 用标准 HTTP 4xx（不像 milvus body code），直接用 _common 的 generic judge。
路径风格 RESTful 含 path params：/collections/{name}/points/search 等。
"""
from __future__ import annotations

import os

try:
    from ._common import (expect_records, expect_rejected, judge_200,
                          judge_4xx, req)
except ImportError:
    from _common import (expect_records, expect_rejected, judge_200,
                         judge_4xx, req)


# 路径模板——所有 path 含 {name}（qdrant RESTful 风格，path 参数是 collection name）
# ponytail: paths verified against live qdrant v1.18.3 container (2026-08-05); recover uses /recover (not /restore)
PATHS = {
    "create_collection":   "/collections/{name}",
    "describe_collection": "/collections/{name}",
    "drop_collection":     "/collections/{name}",
    "list_collections":    "/collections",
    "create_index":        "/collections/{name}/index",
    "delete_index":        "/collections/{name}/index/{field_name}",
    "upsert_points":       "/collections/{name}/points",
    "get_point":           "/collections/{name}/points/{point_id}",
    "delete_points":       "/collections/{name}/points/delete",
    "search":              "/collections/{name}/points/search",
    "search_groups":       "/collections/{name}/points/search/groups",
    "query":               "/collections/{name}/points/query",
    "query_groups":        "/collections/{name}/points/query/groups",
    "count":               "/collections/{name}/points/count",
    "update_collection":   "/collections/{name}",
    "update_aliases":      "/collections/aliases",
    "list_aliases":        "/collections/aliases",
    "list_snapshots":      "/collections/{name}/snapshots",
    "create_snapshot":     "/collections/{name}/snapshots",
    "get_snapshot":        "/collections/{name}/snapshots/{snapshot_name}",
    "delete_snapshot":     "/collections/{name}/snapshots/{snapshot_name}",
    "recover_snapshot":    "/collections/{name}/snapshots/recover",
    "cluster_status":      "/cluster",
    "collection_cluster":  "/collections/{name}/cluster",
    "update_shards":       "/collections/{name}/shards",
    "scroll":              "/collections/{name}/points/scroll",
    "discover":            "/collections/{name}/points/discover",
    "recommend":           "/collections/{name}/points/recommend",
    "root":                "/",
    "healthz":             "/healthz",
    "metrics":             "/metrics",
    # ponytail: aliases for agent-generated path_key naming variants (map to canonical paths)
    "get_cluster":         "/cluster",
    "get_collection_cluster": "/collections/{name}/cluster",
    "group_search":        "/collections/{name}/points/query/groups",
    "get_snapshot":        "/collections/{name}/snapshots/{snapshot_name}",
}

DISTANCE_MAP = {"Cosine": "Cosine", "Euclid": "Euclidean", "Dot": "Dot"}
_BASE = os.environ.get("TESTVDB_DB_URL", "").rstrip("/")


def request(method: str, path_key: str, body: dict | None = None,
            path_params: dict | None = None, timeout: int = 30) -> tuple[int, str]:
    """path_key 必须在 PATHS 里。path_params 替换模板 {field}。

    返回 (status, raw_text)。qdrant 用标准 HTTP status，直接用 _common generic judge。
    """
    if path_key not in PATHS:
        raise KeyError(
            f"path_key={path_key!r} not in qdrant.PATHS; valid keys: {sorted(PATHS)}"
        )
    path = PATHS[path_key]
    if path_params:
        try:
            path = path.format(**path_params)
        except KeyError as e:
            raise KeyError(
                f"path_key={path_key!r} template {path!r} missing param {e}; "
                f"got path_params={path_params}"
            ) from e
    return req(_BASE, method, path, body, timeout=timeout)


def setup_default(name: str, dim: int, metric: str = "Cosine",
                  wait: float | None = None) -> tuple[bool, str]:
    """便捷 setup：PUT /collections/{name}（含 vector config）。

    qdrant 单步 setup（创建时同时配 vector）——比 milvus 简单（无需 index/load）。
    distance ∈ {Cosine, Euclidean, Dot}；默认 Cosine。wait 参数为接口对齐保留（qdrant 不需要）。
    """
    if metric not in DISTANCE_MAP:
        return False, f"unsupported metric={metric!r}; valid: {sorted(DISTANCE_MAP)}"
    s, raw = request("PUT", "create_collection", {
        "vectors": {"size": dim, "distance": DISTANCE_MAP[metric]},
    }, path_params={"name": name})
    if s not in (200, 201, 409):  # 409 = 已存在，复用 OK
        return False, f"create {s}: {raw[:200]}"
    return True, ""


def insert_points(name: str, points: list[dict]) -> tuple[bool, str]:
    s, raw = request("PUT", "upsert_points", {"points": points},
                     path_params={"name": name})
    return (s in (200, 201), f"insert {s}: {raw[:200]}")


def judge_schema_attack(status: int, raw: str, collection_name: str,
                        attack_path: list, attack_value,
                        setup_ok: bool = True) -> str:
    """qdrant 版 schema 类边界攻击判定 — describe_collection 回读三态判定。

    跨 target 一致接口（同 weaviate.judge_schema_attack），qdrant 适配：
      - 标准 HTTP 4xx（无 body code，不像 milvus）
      - describe 返回 {result: {...}}，持久化字段在 result 下

    判定矩阵（同 weaviate）：
      - setup_ok=False / 0 / 5xx → SCRIPT_ERROR
      - status∈{400,422} → NO_DEFECT（正确拒绝）
      - status not in (200,201) → SCRIPT_ERROR
      - status=200/201 + describe 失败 → SCRIPT_ERROR
      - status=200/201 + 字段持久化（任意非 None 值）→ DEFECT_FOUND
      - status=200/201 + 字段被 silent-drop（None）→ NO_DEFECT
    """
    import json as _json
    if not setup_ok or status == 0 or 500 <= status <= 599:
        return "SCRIPT_ERROR"
    if status in (400, 422):
        return "NO_DEFECT"
    if status not in (200, 201):
        return "SCRIPT_ERROR"
    ds, draw = request("GET", "describe_collection",
                       path_params={"name": collection_name})
    if ds != 200:
        return "SCRIPT_ERROR"
    try:
        body = _json.loads(draw) if draw else {}
    except (_json.JSONDecodeError, ValueError, TypeError):
        return "SCRIPT_ERROR"
    persisted = body.get("result") if isinstance(body, dict) else None
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


def drop_collection(name: str) -> None:
    """Cleanup — 永远 try/except，失败不抛。"""
    try:
        request("DELETE", "drop_collection", path_params={"name": name})
    except Exception:
        pass


# ponytail: global paths (no collection name) — 模块级供 _self_check 与 tests 共用
# （run2r-01 R1' 修复后测试与实现豁免集漂移过一次，上提根除）
_NAMELESS = {"list_collections", "update_aliases", "list_aliases",
             "cluster_status", "get_cluster", "root", "healthz", "metrics"}


def _self_check() -> None:
    """ponytail: 静态自检 PATHS 模板 + path_params 替换 + bad key 抛 KeyError。"""
    for k, v in PATHS.items():
        if k in _NAMELESS:
            assert "{name}" not in v, f"{k} should not have {{name}}"
        else:
            assert "{name}" in v, f"{k}={v!r} 缺 {{name}} 占位符"
    p = PATHS["search"].format(name="test_coll")
    assert p == "/collections/test_coll/points/search", p
    p = PATHS["create_collection"].format(name="c1")
    assert p == "/collections/c1", p
    try:
        request("POST", "nonexistent_key", {})
    except KeyError:
        pass
    else:
        raise AssertionError("request() should raise KeyError on bad path_key")
    try:
        PATHS["search"].format()  # 缺 name
    except KeyError:
        pass
    else:
        raise AssertionError("template should require {name}")
    # judge helpers 用 generic 版（不解析 body code）
    assert expect_rejected(400, "", setup_ok=True) == "NO_DEFECT"
    assert expect_rejected(200, "", setup_ok=True) == "DEFECT_FOUND"
    assert expect_records(200, '{"result":[1,2,3]}', expected_min=3) == "NO_DEFECT"
    assert expect_records(200, '{"result":[]}', expected_min=1) == "DEFECT_FOUND"
    assert expect_records(400, "", expected_min=1) == "SCRIPT_ERROR"
    print("qdrant runtime self-check OK")


if __name__ == "__main__":
    _self_check()
