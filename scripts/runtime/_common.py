"""Target-agnostic helpers shared by all REST runtimes.

Per .omc/plans/attack-setup-helpers.md rev 2 — 把 req/judge 从 agent 自由度里拿走，
路径常量与 setup 便捷函数留给各 target runtime。
"""
from __future__ import annotations

import os

import requests


def _auth_header() -> dict:
    h = {"Content-Type": "application/json"}
    a = os.environ.get("TESTVDB_AUTH_HEADER", "")
    if a:
        h["Authorization"] = a
    return h


def req(base_and_prefix: str, method: str, path: str,
        body: dict | None = None, timeout: int = 30) -> tuple[int, str]:
    """Single HTTP exit for all REST runtimes. Returns (status, raw_text).

    base_and_prefix e.g. 'http://host:19530/v2/vectordb'.
    path MUST come from runtime.PATHS — agent 不写字面量路径。
    """
    try:
        r = requests.request(
            method, f"{base_and_prefix}{path}",
            headers=_auth_header(), json=body, timeout=timeout,
        )
        return r.status_code, r.text
    except Exception as e:
        return 0, str(e)


def judge_4xx(status: int, raw: str, setup_ok: bool) -> str:
    """契约违规（应 4xx 拒绝）场景的判定。

    - setup_ok=False → SCRIPT_ERROR（不混淆 setup 失败与 contract 违规）
    - setup_ok=True 且 status ∈ {400,422} → NO_DEFECT（按契约正确拒绝）
    - setup_ok=True 且 status == 200 → DEFECT_FOUND（应拒绝却接受）
    - 其它（404/5xx/0）→ SCRIPT_ERROR（环境/路径问题，不能判 contract）
    """
    if not setup_ok or status == 0 or status == 404 or 500 <= status <= 599:
        return "SCRIPT_ERROR"
    if status in (400, 422):
        return "NO_DEFECT"
    if status == 200:
        return "DEFECT_FOUND"
    return "SCRIPT_ERROR"


def judge_200(status: int, raw: str, setup_ok: bool) -> str:
    """合法输入应被接受场景的判定。

    - setup_ok=False / 5xx / 0 → SCRIPT_ERROR
    - status == 200 → NO_DEFECT
    - 其它（含 4xx）→ DEFECT_FOUND（合法输入被错误拒绝）
    """
    if not setup_ok or status == 0 or 500 <= status <= 599:
        return "SCRIPT_ERROR"
    return "NO_DEFECT" if status == 200 else "DEFECT_FOUND"


def _extract_records(raw: str):
    """通用：从响应里提取 records list。

    覆盖：qdrant result / weaviate objects / GraphQL data.Get.<Class> / 通用 points/hits。

    返回 (records_list_or_None, parsed_ok_bool)。parsed_ok=False 表示响应非 JSON 或结构不识。
    """
    import json
    try:
        b = json.loads(raw) if raw else {}
    except (json.JSONDecodeError, ValueError, TypeError):
        return None, False
    if not isinstance(b, dict):
        return None, False
    # 标准 list 字段（含 weaviate REST "objects"）
    for k in ("result", "objects", "points", "hits"):
        v = b.get(k)
        if isinstance(v, list):
            return v, True
    # 嵌套：data 子字段（GraphQL 风格 data.Get.<Class> 或 data.hits）
    data = b.get("data")
    if isinstance(data, dict):
        get_node = data.get("Get")
        if isinstance(get_node, dict):
            # weaviate GraphQL: data.Get.<ClassName> 是 list
            for cv in get_node.values():
                if isinstance(cv, list):
                    return cv, True
        for sub in ("hits", "points", "result"):
            if isinstance(data.get(sub), list):
                return data[sub], True
    return None, True  # JSON 解析成功但无已知 list 字段


def expect_records(status: int, raw: str, expected_min: int = 1,
                   setup_ok: bool = True) -> str:
    """generic HTTP-status 版——合法查询应返回 ≥ expected_min 条记录。

    用于 qdrant/weaviate 等用标准 HTTP 4xx 的 target（milvus 在 milvus.py 自己 override 解析 body code）。

    - setup_ok=False / 5xx / 0 / 4xx / 404 → SCRIPT_ERROR（查询本身失败，不混淆为记录数缺陷）
    - 200 + records 长度 ≥ expected_min → NO_DEFECT
    - 200 + records 长度 < expected_min → DEFECT_FOUND（应返回但没返回）
    - 200 + 响应非 JSON 或无 records 字段 → NO_DEFECT（无法判定长度，按调用成功处理）
    """
    if not setup_ok or status == 0 or 500 <= status <= 599:
        return "SCRIPT_ERROR"
    if status in (400, 422) or status == 404:
        return "SCRIPT_ERROR"
    if status != 200:
        return "SCRIPT_ERROR"
    records, parsed_ok = _extract_records(raw)
    if not parsed_ok:
        return "SCRIPT_ERROR"
    if records is None:
        return "NO_DEFECT"
    return "NO_DEFECT" if len(records) >= expected_min else "DEFECT_FOUND"


def expect_rejected(status: int, raw: str, setup_ok: bool = True) -> str:
    """generic 版语义别名——= judge_4xx。milvus 在 milvus.py 自己 override。"""
    return judge_4xx(status, raw, setup_ok=setup_ok)
