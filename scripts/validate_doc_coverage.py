#!/usr/bin/env python3
"""Document coverage cross-check: raw_knowledge.json vs OpenAPI spec.

诊断脚本（不阻塞主流程）。读 raw_knowledge.json 已覆盖的端点/字段，
对比 OpenAPI spec 的 /paths + 主要 schema 字段，输出覆盖率报告 + 缺失列表。
（v3.4 §A：md → json 迁移；md 解析路径已删，旧 md 缓存需重新生成。）

用法：
    py -3 scripts/validate_doc_coverage.py {target} {version}
    py -3 scripts/validate_doc_coverage.py qdrant v1.18.3

原则：OpenAPI 仅用于发现（"有哪些"），不提取约束。约束从文档页提取。
"""
from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path

# 默认排除的路径前缀（运维/internal 端点，非公开 API 面）。
# 注意 /cluster 不在此列（pilot 2026-08-20 修正）：GT bug 出现在 /cluster/recover
# （standalone 500→4xx 可触发），排掉它会系统性低估 reach 分母。cluster 面的
# "standalone 大多不可用"由 attack 侧对照组验证兜底，不靠覆盖率排除。
DEFAULT_EXCLUDE_PREFIXES = ["/internal", "/admin", "/telemetry", "/metrics", "/healthz"]


def load_openapi(target: str, version: str) -> dict | None:
    """定位并加载 OpenAPI spec。"""
    candidates = [
        f".sourcedeps/{target}/{version}/openapi.json",
        f".sourcedeps/{target}/{version}/docs/redoc/master/openapi.json",
        f"intelligence/{target}/{version}_openapi.json",
        f"intelligence/{target}/v{version.lstrip('v')}_openapi.json" if not version.startswith("v") else None,
    ]
    for c in candidates:
        if c and os.path.exists(c):
            try:
                return json.loads(Path(c).read_text(encoding="utf-8"))
            except Exception as e:
                print(f"  warn: {c} parse fail: {e}", file=sys.stderr)
    return None


def extract_openapi_endpoints(openapi: dict, exclude_prefixes: list[str]) -> set[tuple[str, str]]:
    """从 OpenAPI /paths 提取 (method, path)。"""
    out = set()
    for path, methods in (openapi.get("paths") or {}).items():
        if not path or path == "/":
            continue  # 根路径/空条目（qdrant spec 合并分片带入 GET /）
        if any(path.startswith(p) for p in exclude_prefixes):
            continue
        # path 参数归一化：{name} → {name}（保留，便于匹配 raw_knowledge）
        for method in methods:
            if method.lower() in ("get", "post", "put", "patch", "delete"):
                out.add((method.upper(), path))
    return out


def extract_openapi_fields(openapi: dict) -> set[str]:
    """从 OpenAPI components.schemas 提取主要字段名（粗粒度，用于发现 strict_mode_config 这类字段）。"""
    out = set()
    schemas = (openapi.get("components") or {}).get("schemas") or {}
    for sname, schema in schemas.items():
        props = (schema.get("properties") or {})
        for pname in props:
            out.add(pname)
        # 也收 required 字段
        for rname in (schema.get("required") or []):
            out.add(rname)
    return out


def _load_raw_knowledge(raw_path: str) -> dict | None:
    """加载 raw_knowledge.json（v3.4 §A：md → json）。缺失/解析失败返回 None（诊断脚本不阻塞）。"""
    if not os.path.exists(raw_path):
        return None
    try:
        return json.loads(Path(raw_path).read_text(encoding="utf-8"))
    except Exception as e:
        print(f"  warn: {raw_path} parse fail: {e}", file=sys.stderr)
        return None


def extract_raw_knowledge_endpoints(raw: dict) -> set[tuple[str, str]]:
    """从 raw_knowledge.json 的 api_endpoints[] 提取已覆盖端点。"""
    out = set()
    for ep in (raw.get("api_endpoints") or []):
        method = str(ep.get("method") or "").upper()
        path = str(ep.get("path") or "").rstrip("/")
        if method in ("GET", "POST", "PUT", "PATCH", "DELETE", "SQL") and path:
            out.add((method, path))
    return out


def _iter_strings(obj):
    """递归收集 JSON 内全部字符串值（字段名发现的粗粒度 token 源）。"""
    if isinstance(obj, str):
        yield obj
    elif isinstance(obj, dict):
        for v in obj.values():
            yield from _iter_strings(v)
    elif isinstance(obj, list):
        for v in obj:
            yield from _iter_strings(v)


def extract_raw_knowledge_fields(raw: dict) -> set[str]:
    """从 raw_knowledge.json 全部字符串值提取已提及字段名（粗粒度 — 参数名、config 字段）。

    与旧 md 版语义等价：正文 token 集对应 json 的 description/assertion/constraints 字符串。
    """
    fields = set()
    for s in _iter_strings(raw):
        for m in re.finditer(r"\b([a-z][a-z0-9_]{4,})\b", s):
            fields.add(m.group(1))
    return fields


def norm_path(p: str) -> str:
    """归一化 path 以对比：去 trailing slash，{param} 统一。"""
    p = p.rstrip("/")
    # {collection_name} vs {name} vs {collection} — 归一化 param 占位符
    p = re.sub(r"\{[^}]+\}", "{param}", p)
    return p


def main():
    if len(sys.argv) < 3:
        print("usage: validate_doc_coverage.py {target} {version}", file=sys.stderr)
        sys.exit(2)
    target, version = sys.argv[1], sys.argv[2]
    exclude = DEFAULT_EXCLUDE_PREFIXES

    openapi = load_openapi(target, version)
    if not openapi:
        print(f"OpenAPI spec not found for {target}/{version} (checked .sourcedeps + intelligence/)")
        print("跳过覆盖率自检（OpenAPI 不可用）")
        sys.exit(0)

    oa_eps = extract_openapi_endpoints(openapi, exclude)
    oa_fields = extract_openapi_fields(openapi)

    raw_path = f"results/{target}/{version}/raw_knowledge.json"
    raw = _load_raw_knowledge(raw_path)
    if raw is None:
        print(f"raw_knowledge.json not found/unparseable: {raw_path}（旧 md 缓存或未生成）")
        print("跳过覆盖率自检（knowledge 缺失）")
        sys.exit(0)
    rk_eps = extract_raw_knowledge_endpoints(raw)
    rk_fields = extract_raw_knowledge_fields(raw)

    # 端点对比（path 归一化后匹配）
    oa_eps_norm = {(m, norm_path(p)) for m, p in oa_eps}
    rk_eps_norm = {(m, norm_path(p)) for m, p in rk_eps}
    missing_eps = sorted(oa_eps_norm - rk_eps_norm)
    covered_eps = oa_eps_norm & rk_eps_norm
    ep_pct = 100.0 * len(covered_eps) / len(oa_eps_norm) if oa_eps_norm else 0.0

    # 字段对比（只报 OpenAPI 有 / raw_knowledge 无 的关键字段，过滤过短/通用）
    # ponytail: 字段覆盖粗粒度（raw_knowledge 的 token 集噪声大），只报 OpenAPI schema 的字段名不在 raw_knowledge
    missing_fields = sorted(f for f in oa_fields if f not in rk_fields and len(f) >= 6)
    field_pct = 100.0 * (len(oa_fields) - len(missing_fields)) / len(oa_fields) if oa_fields else 0.0

    print("=" * 70)
    print(f"Document Coverage Report: {target} {version}")
    print("=" * 70)
    print(f"OpenAPI endpoints (excl {exclude}): {len(oa_eps_norm)}")
    print(f"raw_knowledge endpoints: {len(rk_eps_norm)}")
    print(f"Endpoint coverage: {len(covered_eps)}/{len(oa_eps_norm)} = {ep_pct:.1f}%")
    print()
    if missing_eps:
        print(f"Missing Endpoints ({len(missing_eps)}):")
        for m, p in missing_eps[:30]:
            print(f"  {m:6} {p}")
        if len(missing_eps) > 30:
            print(f"  ... ({len(missing_eps)-30} more)")
    print()
    print(f"OpenAPI schema fields: {len(oa_fields)}")
    print(f"Field coverage (rough): {field_pct:.1f}%")
    if missing_fields:
        print(f"Missing Fields ({len(missing_fields)}, first 20):")
        for f in missing_fields[:20]:
            print(f"  {f}")
    print()
    print(f"doc_coverage_pct: {ep_pct:.1f}% (endpoints)")
    if ep_pct < 90 or any("strict_mode" in p or "strict_mode" in f for f in missing_fields):
        print("\n⚠️  Coverage gap detected — knowledge-extractor Step 6b 应补全这些端点/字段")

    # 机械写回 raw_knowledge.json 的 openapi_coverage（根因修复 2026-08-20；
    # v3.4 §A 格式迁移：md 正则行覆写 → JSON 字段覆写。LLM 自报数字不被采信，
    # 分母 = spec paths 真实计数；报告 JSON 落盘供主进程门控读。）
    report = {
        "target": target,
        "version": version,
        "spec_paths": len(oa_eps_norm),
        "covered": len(covered_eps),
        "doc_coverage_pct": round(ep_pct, 1),
        "missing_endpoints": [f"{m} {p}" for m, p in missing_eps],
        "missing_fields": missing_fields,
        "llm_self_report_overridden": True,
    }
    rp = Path(f"results/{target}/{version}/doc_coverage_report.json")
    if Path(f"results/{target}/{version}").exists():
        rp.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
        print(f"\nreport -> {rp}")
        raw["openapi_coverage"] = {
            "doc_coverage_pct": round(ep_pct, 1),
            "spec_paths": len(oa_eps_norm),
            "covered": len(covered_eps),
            "missing_endpoints": [f"{m} {p}" for m, p in missing_eps],
            "missing_fields": missing_fields,
            "source": "machine-verified vs OpenAPI paths",
        }
        Path(raw_path).write_text(json.dumps(raw, ensure_ascii=False, indent=2), encoding="utf-8")
        print("raw_knowledge.json openapi_coverage 已机械覆写")


if __name__ == "__main__":
    main()
