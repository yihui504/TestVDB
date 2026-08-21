"""Weaviate REST runtime — PATHS 模板 + path_params + setup_default。

weaviate 用标准 HTTP 4xx（同 qdrant，不像 milvus body code），直接用 _common 的 generic judge。
path 风格 `/v1/...`，class name 在 path 里：/v1/schema/{name} 等。
"""
from __future__ import annotations

import os

try:
    from ._common import (expect_records, expect_rejected, judge_200,
                          judge_4xx, req)
except ImportError:
    from _common import (expect_records, expect_rejected, judge_200,
                         judge_4xx, req)


# 路径模板——weaviate REST 风格 /v1/...，class name 在 {name}（部分 list/graphql 无 param）
PATHS = {
    "create_schema":     "/v1/schema",
    "list_schema":       "/v1/schema",
    "describe_schema":   "/v1/schema/{name}",
    "drop_schema":       "/v1/schema/{name}",
    "add_property":      "/v1/schema/{name}/properties",
    "create_object":     "/v1/objects",
    "batch_objects":     "/v1/batch/objects",
    "get_object":        "/v1/objects/{id}",
    "delete_object":     "/v1/objects/{id}",
    "graphql":           "/v1/graphql",
}

DISTANCE_MAP = {"cosine": "cosine", "l2": "l2-squared", "dot": "dot",
                "manhattan": "manhattan"}
_BASE = os.environ.get("TESTVDB_DB_URL", "").rstrip("/")


def request(method: str, path_key: str, body: dict | None = None,
            path_params: dict | None = None, timeout: int = 30) -> tuple[int, str]:
    """path_key 必须在 PATHS 里。path_params 替换模板 {field}。

    返回 (status, raw_text)。weaviate 用标准 HTTP status，直接用 _common generic judge。
    """
    if path_key not in PATHS:
        raise KeyError(
            f"path_key={path_key!r} not in weaviate.PATHS; valid keys: {sorted(PATHS)}"
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


def setup_default(name: str, dim: int, metric: str = "cosine",
                  wait: float | None = None) -> tuple[bool, str]:
    """便捷 setup：POST /v1/schema body 含 class 定义 + vector config。

    weaviate 1.38 单步 setup（POST schema 即可创建 class），无 qdrant/milvus 的多步。
    distance ∈ {cosine, l2-squared, dot, manhattan}；默认 cosine。
    dim 当前由 weaviate 自动推断（首个 vector 写入时），不强制传 vectorIndexConfig.size。
    """
    if metric not in DISTANCE_MAP:
        return False, f"unsupported metric={metric!r}; valid: {sorted(DISTANCE_MAP)}"
    s, raw = request("POST", "create_schema", {
        "class": name,
        "vectorIndexType": "hnsw",
        "vectorIndexConfig": {"distance": DISTANCE_MAP[metric]},
    })
    if s not in (200, 422):  # 422 = already exists (weaviate 返回 422 而非 409)
        return False, f"create_schema {s}: {raw[:200]}"
    return True, ""


def judge_schema_attack(status: int, raw: str, class_name: str,
                        attack_path: list, attack_value,
                        setup_ok: bool = True) -> str:
    """schema 类边界攻击判定 — describe_schema 回读区分 Type1 / silent-drop / Type2 norm。

    expect_rejected 的 schema 专用增强版。weaviate 对 schema 字段有三种"接受"行为：
      - 持久化原值 → Type1_IllegalSuccess（真 bug）
      - silent-drop 字段（unknown field，含 agent 字段放错位置）→ NO_DEFECT（设计行为）
      - silent normalize（如 factor=0→1）→ DEFECT_FOUND（Type2 bug 信号）

    status=200 时**必须 describe 回读**才能区分这三种，否则 silent-drop 会误判 Type1。

    判定矩阵：
      - setup_ok=False / 0 / 5xx → SCRIPT_ERROR
      - status∈{400,422} → NO_DEFECT（正确拒绝）
      - status != 200（除上述） → SCRIPT_ERROR
      - status=200 + describe 失败 → SCRIPT_ERROR
      - status=200 + 字段持久化 == attack_value → DEFECT_FOUND (Type1)
      - status=200 + 字段持久化 != attack_value 且 != None → DEFECT_FOUND (Type2 norm)
      - status=200 + 字段被 silent-drop（None）→ NO_DEFECT

    attack_path 是字段路径 list（如 ["vectorIndexConfig","vectorCacheMaxObjects"]），
    attack_value 是攻击 payload 里的非法值（用于回读比对）。

    实战教训（2026-07-05 weaviate mine）: cleanupIntervalSeconds 放在 vectorIndexConfig
    下被 silent-drop，旧 expect_rejected 看到 status=200 误判 DEFECT_FOUND（25% false positive）。
    """
    import json as _json
    if not setup_ok or status == 0 or 500 <= status <= 599:
        return "SCRIPT_ERROR"
    if status in (400, 422):
        return "NO_DEFECT"
    if status != 200:
        return "SCRIPT_ERROR"
    ds, draw = request("GET", "describe_schema", path_params={"name": class_name})
    if ds != 200:
        return "SCRIPT_ERROR"
    try:
        persisted = _json.loads(draw) if draw else {}
    except (_json.JSONDecodeError, ValueError, TypeError):
        return "SCRIPT_ERROR"
    node = persisted
    for k in attack_path:
        if isinstance(node, dict):
            node = node.get(k)
        else:
            return "SCRIPT_ERROR"
    if node is None:
        return "NO_DEFECT"  # silent-drop = weaviate 设计行为，非 bug
    return "DEFECT_FOUND"  # persist (Type1 原值 或 Type2 norm)


def insert_points(name: str, points: list[dict]) -> tuple[bool, str]:
    """批量插入对象。points: [{"class": name, "properties": {...}, "vector": [...]}, ...]"""
    s, raw = request("POST", "batch_objects", {"objects": points})
    return (s in (200, 201), f"batch {s}: {raw[:200]}")


def drop_schema(name: str) -> None:
    """Cleanup — DELETE /v1/schema/{name}，try/except 不抛。"""
    try:
        request("DELETE", "drop_schema", path_params={"name": name})
    except Exception:
        pass


def _self_check() -> None:
    """ponytail: 静态自检 PATHS 模板 + path_params 替换 + bad key 抛 KeyError。"""
    name_keys = {"describe_schema", "drop_schema", "add_property"}
    id_keys = {"get_object", "delete_object"}
    no_param_keys = {"create_schema", "list_schema", "create_object",
                     "batch_objects", "graphql"}
    for k in name_keys:
        assert "{name}" in PATHS[k], f"{k} should contain {{name}}"
    for k in id_keys:
        assert "{id}" in PATHS[k], f"{k} should contain {{id}}"
    for k in no_param_keys:
        assert "{" not in PATHS[k], f"{k} should be no-param, got {PATHS[k]!r}"
    assert PATHS["drop_schema"].format(name="MyClass") == "/v1/schema/MyClass"
    assert PATHS["get_object"].format(id="abc-123") == "/v1/objects/abc-123"
    try:
        request("POST", "nonexistent_key", {})
    except KeyError:
        pass
    else:
        raise AssertionError("request() should raise KeyError on bad path_key")
    assert expect_rejected(400, "", setup_ok=True) == "NO_DEFECT"
    assert expect_rejected(200, "", setup_ok=True) == "DEFECT_FOUND"
    # weaviate GraphQL 风格：{"data": {"Get": {"Article": [...]}}}
    assert expect_records(200, '{"data":{"Get":{"Article":[1,2,3]}}}', expected_min=3) == "NO_DEFECT"
    # weaviate REST list 风格：{"objects": [...]}
    assert expect_records(200, '{"objects":[],"totalObjects":0}', expected_min=1) == "DEFECT_FOUND"
    assert expect_records(400, "", expected_min=1) == "SCRIPT_ERROR"
    # judge_schema_attack 早退路径（describe 回读分支由 pytest monkeypatch 覆盖）
    assert judge_schema_attack(422, "", "X", ["k"], -1, setup_ok=True) == "NO_DEFECT"
    assert judge_schema_attack(400, "", "X", ["k"], -1, setup_ok=True) == "NO_DEFECT"
    assert judge_schema_attack(0, "", "X", ["k"], -1, setup_ok=True) == "SCRIPT_ERROR"
    assert judge_schema_attack(500, "", "X", ["k"], -1, setup_ok=True) == "SCRIPT_ERROR"
    assert judge_schema_attack(200, "", "X", ["k"], -1, setup_ok=False) == "SCRIPT_ERROR"
    assert judge_schema_attack(404, "", "X", ["k"], -1, setup_ok=True) == "SCRIPT_ERROR"
    print("weaviate runtime self-check OK")


if __name__ == "__main__":
    _self_check()
