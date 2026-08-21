#!/usr/bin/env python3
"""Pre-fetch OpenAPI spec for a target/version into .sourcedeps/ (deterministic, no LLM).

Root-cause fix (pilot qdrant v1.18.2, 2026-08-20): knowledge-extractor Step 6b
的 OpenAPI cross-check 依赖 `.sourcedeps/...` 存在，"如不存在则跳过"是无条件
逃逸舱门——qdrant spec 从未被 fetch，extractor 只抓了 12 页却自报
doc_coverage_pct 100%。主进程在派 extractor **之前**跑本脚本把 spec 备好，
Step 6b 从"可选"变"有据可依"。

用法：
    py -3 scripts/fetch_openapi_spec.py {target} {version}

产出：
    .sourcedeps/{target}/{version}/openapi.json   # 合并后的单文件 spec（paths+components）

per-target 规则：
    qdrant    https://api.qdrant.tech/openapi/ 目录页 → api-reference-{N}.json 分片合并
              （注意 spec 是 latest（1.19.x）；记录 spec_version 供 extractor 版本对齐用）
    weaviate  https://raw.githubusercontent.com/weaviate/weaviate/{tag}/openapi-specs/schema.json
    milvus    GitHub docs 仓库 openapi（若无公开 spec 则报 not_supported，退出 3）
    其余      not_supported（退出 3，主进程继续跑，不阻塞）
"""
import json
import os
import re
import sys
import urllib.request
from pathlib import Path

UA = {"User-Agent": "TestVDB-fetch-openapi/1.0"}
TIMEOUT = 30


def _get(url: str, binary: bool = False):
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        data = r.read()
    return data if binary else data.decode("utf-8", errors="replace")


def _fetch_qdrant() -> dict:
    """qdrant: 目录页列分片 → 逐片拉取 → 合并 paths + components.schemas。"""
    index = _get("https://api.qdrant.tech/openapi.json")
    shards = sorted(set(re.findall(r'href="(openapi/[^"]+\.json)"', index)))
    if not shards:
        raise RuntimeError("qdrant 目录页无分片链接")
    merged: dict = {"openapi": "3.0", "info": {"title": "qdrant (merged shards)"}, "paths": {}, "components": {"schemas": {}}}
    for s in shards:
        spec = json.loads(_get(f"https://api.qdrant.tech/{s}"))
        merged["paths"].update(spec.get("paths") or {})
        comp = spec.get("components") or {}
        merged["components"]["schemas"].update(comp.get("schemas") or {})
    return merged


def _fetch_weaviate(version: str) -> dict:
    tag = version if version.startswith("v") else f"v{version}"
    raw = _get(f"https://raw.githubusercontent.com/weaviate/weaviate/{tag}/openapi-specs/schema.json")
    return json.loads(raw)


def main() -> int:
    if len(sys.argv) < 3:
        print("usage: fetch_openapi_spec.py {target} {version}", file=sys.stderr)
        return 2
    target, version = sys.argv[1], sys.argv[2]
    out_dir = Path(f".sourcedeps/{target}/{version}")
    out_path = out_dir / "openapi.json"

    if target == "qdrant":
        spec = _fetch_qdrant()
    elif target == "weaviate":
        spec = _fetch_weaviate(version)
    else:
        print(f"[fetch-openapi] {target}: no deterministic spec rule, skip")
        return 3

    out_dir.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(spec, ensure_ascii=False), encoding="utf-8")
    n_paths = len(spec.get("paths") or {})
    print(f"[fetch-openapi] {target}/{version}: {n_paths} paths -> {out_path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as e:
        print(f"[fetch-openapi] FAIL: {e}", file=sys.stderr)
        raise SystemExit(1)
