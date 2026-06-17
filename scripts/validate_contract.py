#!/usr/bin/env python3
"""通用契约验证器（批次 B2，替代 pre-v2.0 的 validate_weaviate_contract.py）。

参数化 target/version（不硬编码），适配 v2.0（api_endpoints list）+ legacy
（api_endpoint 单数/字符串）schema。

新增 bug #3 检测侧：target-aware category 污染警告（collections/points 出现在
非 qdrant target）——这是契约层 bug #3 的检测，根因（formalizer 映射）留专项。

Usage: python scripts/validate_contract.py <contract_path>
Exit: 0=pass, 1=errors, 2=usage/load error
"""
from __future__ import annotations

import json
import sys

if sys.platform == "win32":
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")


def load_contract(path):
    """读契约，返回 (contract_dict, error_msg)。成功则 error_msg=None。"""
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f), None
    except FileNotFoundError:
        return None, f"contract not found: {path}"
    except (json.JSONDecodeError, OSError) as e:
        return None, f"contract parse error: {e}"


def get_endpoints(contract):
    """适配 v2.0 (api_endpoints list) + legacy (api_endpoint 单数/list)。"""
    eps = contract.get("api_endpoints")
    if isinstance(eps, list):
        return eps
    single = contract.get("api_endpoint")
    if isinstance(single, list):
        return single
    return []


def validate_contract(contract):
    """返回 (errors, warnings)。"""
    errors, warnings = [], []

    # required top-level
    for field in ("target", "version"):
        if not contract.get(field):
            errors.append(f"missing/empty required field: {field}")

    endpoints = get_endpoints(contract)
    if not endpoints:
        warnings.append("no api_endpoints (或为空) — 跳过端点级检查")
    else:
        for ep in endpoints:
            for rf in ("path", "method", "category", "source_url"):
                if not ep.get(rf):
                    errors.append(f"endpoint {ep.get('path', '?')} missing {rf}")
        missing_src = [e.get("path", "?") for e in endpoints if not e.get("source_url")]
        if missing_src:
            errors.append(f"endpoints missing source_url: {missing_src[:5]}")

    if not contract.get("data_types"):
        warnings.append("empty/missing data_types")

    # constraints（v2.0 dict 含 type/range/state_constraints；legacy 顶层 list）
    constraints = contract.get("constraints")
    cids = []
    if isinstance(constraints, dict):
        for ct in ("type_constraints", "range_constraints", "state_constraints"):
            for con in constraints.get(ct, []):
                cids.append(con.get("constraint_id"))
                if not con.get("source_url"):
                    warnings.append(
                        f"constraint {con.get('constraint_id')} missing source_url")
    elif isinstance(constraints, list):
        cids = [c.get("constraint_id") for c in constraints if isinstance(c, dict)]

    if cids:
        dupes = {c for c in cids if cids.count(c) > 1}
        if dupes:
            errors.append(f"duplicate constraint IDs: {dupes}")

    # _passport（可选，pre-v2.0 无）
    if "_passport" not in contract:
        warnings.append("missing _passport (pre-v2.0 contract; passport 验证跳过)")

    # bug #3 检测侧：category 应在通用词表内（schema/data/search/index/admin/other）
    # 旧 Qdrant 倾向词（collections/points/ddl/dml/management 等）→ 警告
    valid_categories = {"schema", "data", "search", "index", "admin", "other"}
    if endpoints:
        invalid = [(e.get("path", "?"), e.get("category")) for e in endpoints
                   if e.get("category") and e.get("category") not in valid_categories]
        if invalid:
            warnings.append(
                f"category 非通用词表: {len(invalid)} 个端点用了非标准 category "
                f"(应为 schema/data/search/index/admin/other) — bug #3 检测侧: "
                f"{[(p, c) for p, c in invalid[:3]]}"
            )

    return errors, warnings


# [5] 端点完整度检测：raw_knowledge 中的 HTTP 路径引用模式
import re as _re
_PATH_REF_RE = _re.compile(r'/(?:v\d+/)?[a-z][\w{}.-]*(?:/[\w{}.-]+)+', _re.IGNORECASE)


def check_endpoint_completeness(endpoints_count, raw_knowledge_path):
    """端点完整度检测：契约端点数 vs raw_knowledge HTTP 路径引用数。
    返回 warning 字符串或 None。启发式（raw_knowledge 非结构化，路径引用是粗略上界）。"""
    try:
        with open(raw_knowledge_path, encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return None  # 无 raw_knowledge，跳过
    refs = {m.group(0).rstrip(".,;)\"'") for m in _PATH_REF_RE.finditer(text)}
    refs = {r for r in refs if len(r) > 3}  # 过滤过短
    ref_count = len(refs)
    if ref_count == 0:
        return None
    if endpoints_count < ref_count * 0.5:
        return (
            f"端点完整度可能不全: 契约 {endpoints_count} 端点 vs raw_knowledge {ref_count} 路径引用 "
            f"(<50%)。contract-formalizer 可能漏提取（见规则 1 提取完整度）。"
        )
    return None


def main():
    if len(sys.argv) < 2:
        print("Usage: python scripts/validate_contract.py <contract_path>", file=sys.stderr)
        return 2
    path = sys.argv[1]
    contract, err = load_contract(path)
    if err:
        print(f"ERROR: {err}", file=sys.stderr)
        return 2

    errors, warnings = validate_contract(contract)
    # [5] 端点完整度检测（vs raw_knowledge，cross-file）
    from pathlib import Path as _P
    _raw = _P(path).resolve().parent / "raw_knowledge.md"
    _comp = check_endpoint_completeness(len(get_endpoints(contract)), str(_raw))
    if _comp:
        warnings.append(_comp)
    target = contract.get("target", "?")
    print(f"contract: {path} (target={target})")
    if warnings:
        print(f"warnings ({len(warnings)}):")
        for w in warnings:
            print(f"  ⚠ {w}")
    if errors:
        print(f"errors ({len(errors)}):")
        for e in errors:
            print(f"  ✗ {e}")
        print(f"\nRESULT: FAIL ({len(errors)} errors)")
        return 1
    print(f"\nRESULT: PASS ({len(warnings)} warnings)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
