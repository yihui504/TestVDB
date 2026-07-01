#!/usr/bin/env python3
"""Target neutrality validator — Stage 1 gate (组件 B).

DETECTION-ONLY signature table. NOT used for script generation.
See agents/_target_api_reference.md for the contract-driven generation principle.

Reads {session_dir}/structured_contract.json -> target, scans all generated *.py
for DB signatures that DON'T match the current target. target-aware:
qdrant syntax is legal when target=qdrant; REJECT only when mismatched
(e.g. target=weaviate but script hits :6333 / /collections/.../points).

Usage: python scripts/validate_target_neutrality.py <session_dir>
Exit: 0 = all pass / no foreign signatures; 1 = REJECT (foreign DB signatures found).
"""
from __future__ import annotations

import glob
import json
import os
import re
import sys

from _pipeline_utils import setup_encoding

setup_encoding()


# DETECTION-ONLY — 不用于生成。高置信度稳定指纹（默认端口、核心路径前缀、过滤关键字、响应键访问语法）。
SIGNATURES: dict[str, dict[str, list[str]]] = {
    "qdrant": {
        "ports": [r":6333\b", r"\b6333/"],
        "paths": [r"/collections/[\w-]+/points(?:/search|/count)?",
                  r"/collections/\{[^}]+\}/points"],
        "filter_keys": [r'"must"\s*:', r"'must'\s*:", r'"match"\s*:'],
        # resp_keys 清空：result 字段非 Qdrant 独有——weaviate POST /v1/batch/objects 响应
        # item 内嵌 {result:{status,errors}} 是合法格式，`.get("result")`/`["result"]` 会误报。
        # Qdrant 靠 ports(6333)/paths(/collections/.../points)/filter_keys(must/match) 强信号检测。
        "resp_keys": [],
    },
    "weaviate": {
        "ports": [r":8080\b", r"\b8080/"],
        "paths": [r"/v1/objects", r"/v1/schema", r'"/objects"', r'"/schema"'],
        "filter_keys": [r'"where"\s*:', r'"operator"\s*:'],
        "resp_keys": [r'\[\s*"data"\s*\]', r'\.get\(\s*"data"'],
    },
    "milvus": {
        "ports": [r":19530\b", r"\b19530/"],
        "paths": [r"/v2/vectordb/"],
        "filter_keys": [r'"expr"\s*:'],
        "resp_keys": [],
    },
    "pgvector": {
        "ports": [r":5432\b", r"\b5432/"],
        "paths": [],
        "filter_keys": [],
        "resp_keys": [],
    },
    "meilisearch": {
        "ports": [r":7700\b", r"\b7700/"],
        "paths": [r"/indexes/[\w-]+/(?:documents|search|settings|tasks)",
                  r"/indexes/\{[^}]+\}"],
        # hybrid/semanticRatio 是 Meilisearch 向量混合搜索独有的请求键
        "filter_keys": [r'"semanticRatio"\s*:', r'"hybrid"\s*:\s*\{'],
        "resp_keys": [],
    },
    "chroma": {
        "ports": [],  # 8000 太通用（django/jupyter 等共用），不用端口指纹避免误报
        # Task 4c fix: Add whitelist for /v1/schema/{class}/indexes/{prop} pattern (Weaviate reindex API)
        # to avoid matching Meilisearch /indexes/{uid} pattern
        "paths": [r"/api/v2/tenants", r"/api/v2/databases",
                  r"/api/v[12]/[^/]*/collections/\{[^}]+\}"],
        # Chroma where 用 mongo 风格操作符 $contains/$and/$or（区别于 weaviate 的 "where"）
        "filter_keys": [r'"\$contains"\s*:', r"'\$contains'\s*:",
                        r'"\$and"\s*:', r"'\$or'\s*:"],
        "resp_keys": [],
    },
}


def _load_target(session_dir: str) -> str | None:
    # 契约可能在 session_dir（timestamp 目录）或父级 version 目录。
    # pipeline v3 布局: results/<target>/<version>/<timestamp>/  ← session_dir（脚本所在）
    #          契约在: results/<target>/<version>/              ← 父级 version 目录
    session_dir_norm = session_dir.rstrip(os.sep)
    candidates = [
        os.path.join(session_dir_norm, "structured_contract.json"),
        os.path.join(os.path.dirname(session_dir_norm), "structured_contract.json"),
    ]
    for path in candidates:
        try:
            with open(path, encoding="utf-8") as fh:
                data = json.load(fh)
            target = str(data.get("target", "")).lower()
            if target:
                return target
        except (FileNotFoundError, json.JSONDecodeError, OSError):
            continue
    return None


def _scan_file(content: str) -> dict[str, list[str]]:
    """Return {db: [matched pattern strings]} for all DB signatures hit in content."""
    hits: dict[str, list[str]] = {}
    for db, groups in SIGNATURES.items():
        matched: list[str] = []
        for group_name in ("ports", "paths", "filter_keys", "resp_keys"):
            for pat in groups.get(group_name, []):
                if re.search(pat, content):
                    matched.append(f"{group_name}: {pat}")
        if matched:
            hits[db] = matched
    return hits


def validate(session_dir: str, target: str) -> list[dict]:
    """Return list of findings: scripts with foreign-DB signatures (not target)."""
    findings: list[dict] = []
    for f in sorted(glob.glob(os.path.join(session_dir, "**/*.py"), recursive=True)):
        if "/mre/" in f.replace("\\", "/"):
            continue
        try:
            with open(f, encoding="utf-8", errors="replace") as fh:
                content = fh.read()
        except OSError:
            continue
        hits = _scan_file(content)
        # foreign = any DB signature that is NOT the current target
        foreign = {db: ev for db, ev in hits.items() if db != target}
        if foreign:
            findings.append({
                "file": os.path.relpath(f, session_dir),
                "foreign_dbs": foreign,
            })
    return findings


def main() -> int:
    if len(sys.argv) < 2:
        print("Usage: python scripts/validate_target_neutrality.py <session_dir>", file=sys.stderr)
        return 2
    session_dir = sys.argv[1]
    if not os.path.isdir(session_dir):
        print(f"ERROR: {session_dir} not found", file=sys.stderr)
        return 2

    target = _load_target(session_dir)
    if not target:
        print("[Stage 1] Target Neutrality: skipped (no target in structured_contract.json)")
        return 0

    findings = validate(session_dir, target)

    if findings:
        print(json.dumps({"target": target, "target_neutrality_violations": [
            {"file": x["file"], "foreign_dbs": x["foreign_dbs"]} for x in findings
        ]}, indent=2, ensure_ascii=False))
        print(f"[Stage 1] Target Neutrality Check: {len(findings)} script(s) REJECTED "
              f"(contain {target}-foreign DB signatures; target={target})")
        for x in findings:
            dbs = ", ".join(x["foreign_dbs"].keys())
            print(f"  REJECT: {x['file']} — foreign DB signature(s): {dbs}")
        return 1

    print(f"[Stage 1] Target Neutrality Check: all scripts consistent with target={target}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
